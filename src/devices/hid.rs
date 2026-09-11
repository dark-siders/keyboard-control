// --- Общий HID-'мост' для всех моделей ---
// Упрощает работу с hidapi: открытие устройства, отправка фичур-репортов,
// определение типа подключения по PID.

use crate::types::ConnectionType;
use hidapi::{HidApi, HidDevice};

/// Малый помощник поверх hidapi.
/// Держит открытое устройство + карточку DeviceInfo.
pub struct HidBridge {
    pub device: HidDevice,
    pub vid: u16,
    pub pid: u16,
    pub connection: ConnectionType,
}

impl HidBridge {
    /// Открыть устройство по VID/PID.
    /// Для AULA F75/F99 командный канал — интерфейс с вендорским usage page
    /// (hidraw6), а не обычный клавиатурный. Ищем такой интерфейс,
    /// иначе открываем первый попавшийся.
    pub fn open(api: &HidApi, vid: u16, pid: u16) -> hidapi::HidResult<Self> {
        let device: HidDevice = {
            let mut vendor: Option<HidDevice> = None;
            for d in api.device_list() {
                if d.vendor_id() != vid || d.product_id() != pid {
                    continue;
                }
                if d.usage_page() >= 0xFF00 {
                    let p = d.path();
                    if p.is_empty() {
                        continue;
                    }
                    let cstr = match std::ffi::CString::new(p.to_bytes()) {
                        Ok(c) => c,
                        // Путь с NUL невозможен для hidraw; просто пропускаем.
                        Err(_) => continue,
                    };
                    match api.open_path(&cstr) {
                        Ok(h) => {
                            vendor = Some(h);
                            break;
                        }
                        Err(_) => continue,
                    }
                }
            }
            match vendor {
                Some(h) => h,
                None => api.open(vid, pid)?,
            }
        };
        let connection = match pid {
            // --- Ajazz ---
            0x8207 => ConnectionType::Usb,
            0x8208 => ConnectionType::Wireless2_4G,
            0x8209 => ConnectionType::Bluetooth,
            // --- AULA ---
            0x004B | 0x004E => ConnectionType::Usb,
            0x004C | 0x004F => ConnectionType::Wireless2_4G,
            0x004D | 0x0050 => ConnectionType::Bluetooth,
            // Ревизия «BY Tech Gaming Keyboard» — USB
            0x010C => ConnectionType::Usb,
            _ => ConnectionType::Usb,
        };
        Ok(Self {
            device,
            vid,
            pid,
            connection,
        })
    }

    /// Отправить feature-репорт.
    /// `data[0]` — номер репорта (обычно 0x00), остальное — полезная нагрузка.
    pub fn send_feature(&self, data: &[u8]) -> hidapi::HidResult<()> {
        self.device.send_feature_report(data)
    }

    /// Прочитать feature-репорт заданного размера.
    pub fn read_feature(&self, report_id: u8, max_len: usize) -> hidapi::HidResult<Vec<u8>> {
        let mut buf = vec![0u8; max_len + 1];
        buf[0] = report_id;
        let n = self.device.get_feature_report(&mut buf)?;
        buf.truncate(n);
        Ok(buf)
    }

    /// Отправить обычный output-репорт (если клавиатура требует именно его).
    pub fn send_output(&self, data: &[u8]) -> hidapi::HidResult<()> {
        self.device.send_output_report(data)
    }

    /// Чтение input-репорта с таймаутом.
    /// Используется для батареи, состояния и т.д.
    pub fn read_input(&self, buf: &mut [u8], timeout_ms: i32) -> hidapi::HidResult<usize> {
        self.device.read_timeout(buf, timeout_ms)
    }
}

/// Разбить строку SSTRL/SSND в байты (для прямых HID-команд).
#[allow(dead_code)]
fn hex(s: &str) -> Vec<u8> {
    s.bytes()
        .collect::<Vec<_>>()
        .chunks(2)
        .filter_map(|c| {
            let t = std::str::from_utf8(c).ok()?;
            u8::from_str_radix(t, 16).ok()
        })
        .collect()
}