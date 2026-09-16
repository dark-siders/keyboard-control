// --- Сканер HIDRAW-устройств с hot-plug ---
// Перечисляет устройства, находит поддерживаемые клавиатуры по VID/PID,
// а также следит за подключением/отключением (горячая замена).

use std::sync::mpsc;

use hidapi::HidApi;

use super::ajazz_ak820::AjazzAk820;
use super::aula_f75::AulaF75;
use super::aula_f99::AulaF99;
use super::hid::HidBridge;
use super::{find_device_info, DeviceInfo, KeyboardDevice, Layout};

/// Событие hot-plug.
#[derive(Debug, Clone)]
pub enum HotplugEvent {
    Connected {
        vid: u16,
        pid: u16,
        path: Option<String>,
    },
    Disconnected {
        vid: u16,
        pid: u16,
        path: Option<String>,
    },
}

/// Одна найденная клавиатура (перечислимая, с карточкой).
#[derive(Debug, Clone)]
pub struct FoundDevice {
    pub info: &'static DeviceInfo,
    pub path: Option<String>,
    pub busy: bool,
}

/// Сканер: держит HidApi и опрашивает список каждые N мс.
pub struct Scanner {
    api: HidApi,
    /// VID/PID поддерживаемых устройств.
    supported: Vec<(u16, u16)>,
    /// Канал для событий подключения/отключения.
    event_tx: mpsc::Sender<HotplugEvent>,
    /// Последний известный набор устройств: (path, vid, pid).
    previous: Vec<(Option<String>, u16, u16)>,
}

impl Scanner {
    pub fn new(event_tx: mpsc::Sender<HotplugEvent>) -> Result<Self, hidapi::HidError> {
        let api = HidApi::new()?;
        let supported = super::DEVICES
            .iter()
            .map(|d| (d.vid, d.pid))
            .collect::<Vec<_>>();
        Ok(Self {
            api,
            supported,
            event_tx,
            previous: Vec::new(),
        })
    }

    /// Список всех подключённых поддерживаемых клавиатур.
    pub fn scan(&self) -> Result<Vec<FoundDevice>, Box<dyn std::error::Error>> {
        let devices = self.api.device_list();
        let mut found = Vec::new();
        // Дедупликация по (VID, PID): у AULA F75 два HID-интерфейса
        // (hidraw5 — клавиатура, hidraw6 — вендорский канал), оба с одним PID.
        // В списке должна быть одна карточка на устройство.
        let mut seen: Vec<(u16, u16)> = Vec::new();

        for dev in devices {
            let vid = dev.vendor_id();
            let pid = dev.product_id();
            if !self.supported.contains(&(vid, pid)) {
                continue;
            }
            if seen.contains(&(vid, pid)) {
                continue;
            }
            seen.push((vid, pid));
            let c = dev.path();
            let path = if c.is_empty() {
                None
            } else {
                Some(c.to_string_lossy().to_string())
            };
            found.push(FoundDevice {
                info: find_device_info(vid, pid).expect("известный VID/PID"),
                path,
                busy: false,
            });
        }
        Ok(found)
    }

    /// Открыть модель по (VID, PID, path) и вернуть boxed KeyboardDevice.
    pub fn open_device(
        &self,
        vid: u16,
        pid: u16,
        path: Option<&str>,
    ) -> Result<Box<dyn KeyboardDevice>, Box<dyn std::error::Error>> {
let bridge = match path {
            Some(p) => {
                let cstr = std::ffi::CString::new(p)?;
                let mut bridge = HidBridge {
                    device: self.api.open_path(&cstr)?,
                    vid,
                    pid,
                    connection: find_device_info(vid, pid)
                        .map(|d| d.connection)
                        .unwrap_or(crate::types::ConnectionType::Usb),
                    model: None,
                };
                // Общий PID 0x010C у F75/F99: определяем модель по ходу,
                // чтобы матчер ниже собрал правильный драйвер.
                if vid == 0x258A && pid == 0x010C {
                    if let Ok(m) = bridge.detect_model() {
                        bridge.model = Some(m);
                    }
                }
                bridge
            }
            None => HidBridge::open(&self.api, vid, pid)?,
        };

        let info = find_device_info(vid, pid).expect("известный VID/PID");
        let model = bridge.model;
        // Выбираем конкретную модель по VID/PID.
        match (info.vid, info.pid) {
            (0x0C45, _) => Ok(Box::new(AjazzAk820::open(bridge))),
            // AULA F75 PID: 4B, 4C, 4D + ревизия 010C (010C делят F75/F99 —
            // модель определяется по feature-запросу, см. HidBridge::detect_model)
            (0x258A, 0x004B..=0x004D) => Ok(Box::new(AulaF75::open(bridge))),
            (0x258A, 0x010C) => match model {
                Some(0xA4) => Ok(Box::new(AulaF99::open(bridge))),
                _ => Ok(Box::new(AulaF75::open(bridge))),
            },
            // AULA F99 PID: 4E, 4F, 50
            (0x258A, 0x004E..=0x0050) => Ok(Box::new(AulaF99::open(bridge))),
            // 2.4G-ресивер Compx (3554:FA09) — общий для F75/F99
            (0x3554, 0xFA09) => Ok(Box::new(AulaF75::open(bridge))),
            _ => Err("неподдерживаемый VID/PID".into()),
        }
    }

    /// Раскладка по VID/PID.
    pub fn layout_of(vid: u16, pid: u16) -> Option<Layout> {
        find_device_info(vid, pid).map(|d| d.layout)
    }

    /// Сравнить текущий список с предыдущим и послать события.
    pub fn poll_hotplug(&mut self) {
        let devices = self.api.device_list();
        let mut current: Vec<(Option<String>, u16, u16)> = Vec::new();
        for dev in devices {
            let vid = dev.vendor_id();
            let pid = dev.product_id();
            let c = dev.path();
            let path = if c.is_empty() {
                None
            } else {
                Some(c.to_string_lossy().to_string())
            };
            if self.supported.contains(&(vid, pid)) {
                current.push((path, vid, pid));
            }
        }

        // Подключённые
        for d in &current {
            if !self.previous.contains(d) {
                let (path, vid, pid) = d.clone();
                let _ = self.event_tx.send(HotplugEvent::Connected { vid, pid, path });
            }
        }
        // Отключённые
        for d in &self.previous {
            if !current.contains(d) {
                let (path, vid, pid) = d.clone();
                let _ = self.event_tx.send(HotplugEvent::Disconnected { vid, pid, path });
            }
        }
        self.previous = current;
    }
}

/// Вспомогательная функция: вывести VID/PID в шестнадцатеричном виде.
pub fn hex_id(vid: u16, pid: u16) -> String {
    format!("{:04X}:{:04X}", vid, pid)
}

// Проверка на компиляцию типажей для устройств:
#[allow(dead_code)]
fn _assert_send_sync() {
    fn assert_send<T: Send>() {}
    assert_send::<AjazzAk820>();
    assert_send::<AulaF75>();
    assert_send::<AulaF99>();
}

/// Получить Hint-имена из sysfs для реальной диагностики.
/// Работает только под Linux.
#[allow(dead_code)]
pub fn sysfs_uevent(path: &std::path::Path) -> Option<String> {
    let uevent = path.join("device/uevent");
    std::fs::read_to_string(uevent).ok()
}