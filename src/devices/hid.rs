// --- Общий HID-'мост' для всех моделей ---
// Упрощает работу с hidapi: открытие устройства, отправка фичур-репортов,
// определение типа подключения по PID.

use crate::types::ConnectionType;
use hidapi::{HidApi, HidDevice};

/// Размер фича-пакета командного канала AULA SinoWealth (report id + 519 payload).
const PACKET_LEN: usize = 520;

/// Малый помощник поверх hidapi.
/// Держит открытое устройство + карточку DeviceInfo.
///
/// Поле `model` — идентификатор модели SinoWealth (0xCD = AULA F75, 0xA4 =
/// AULA F99), если его удалось определить при открытии. Проводной F75 и F99
/// СИДЯТ НА ОДНОМ VID:PID 0x258A:0x010C (KB.ini, журнал 10.51), поэтому выбор
/// драйвера делается по `model`, а не по PID. Заполняется только для 0x010C;
/// для остальных PID — None.
pub struct HidBridge {
    pub device: HidDevice,
    pub vid: u16,
    pub pid: u16,
    pub connection: ConnectionType,
    pub model: Option<u8>,
}

impl HidBridge {
    /// Открыть устройство по VID/PID.
    /// Для AULA F75/F99 командный канал — интерфейс с вендорским usage page
    /// (hidraw6), а не обычный клавиатурный. Ищем такой интерфейс,
    /// иначе открываем первый попавшийся.
    pub fn open(api: &HidApi, vid: u16, pid: u16) -> hidapi::HidResult<Self> {
        let device: HidDevice = {
            // Командный канал — интерфейс с ВЕНДОРСКИМ usage page:
            //   - ресивер 2.4G (3554:FA09): строго 0xFF02 (dnviti), клавиатурный
            //     hidraw имеет 0xFF04 (0x13-репорт живёт только там, PROTOCOL.md);
            //   - AULA F75/F99 USB: ≥0xFF00 (hidraw6), либо interface_number>=1
            //     (оба USB-интерфейса могут иметь usage page 0x0001, журнал 10.20).
            // Три уровня предпочтения: точный 0xFF02 → ≥0xFF00 → iface≥1.
            // Сначала выбираем путь-победителя (ранг), затем открываем его —
            // чтобы не делить одну HidDevice между слотами (двигается).
            let mut best_rank = 0u8; // 0 = ничего не выбрано
            let mut best_path: Option<Vec<u8>> = None;
            for d in api.device_list() {
                if d.vendor_id() != vid || d.product_id() != pid {
                    continue;
                }
                let p = d.path();
                if p.to_bytes().is_empty() {
                    continue;
                }
                let rank = match d.usage_page() {
                    // Командный канал Ajazz AK820 Pro: control endpoint — usage
                    // page 0xFF68 (wsclx, журнал 10.53). 0xFF67 — это TFT-канал,
                    // он НЕ годится для control-команд.
                    0xFF68 => 4,
                    // Точный командный канал ресивера / вендорских коллекций.
                    0xFF02 => 3,
                    // Вендорский (командный hidraw6 у AULA USB).
                    pp if pp >= 0xFF00 => 2,
                    // Первый не-клавиатурный живой интерфейс (interface 1+).
                    _ if d.interface_number() >= 1 => 1,
                    _ => 0,
                };
                if rank > best_rank {
                    best_rank = rank;
                    best_path = Some(p.to_bytes().to_vec());
                }
            }
            match best_path {
                Some(bytes) => match std::ffi::CString::new(bytes) {
                    Ok(cstr) => api.open_path(&cstr)?,
                    Err(_) => api.open(vid, pid)?,
                },
                None => api.open(vid, pid)?,
            }
        };
        let connection = match pid {
            // --- Ajazz AK820 Pro (Sonix SN32F299) ---
            // 0x8009 — wired + 2.4 GHz (один PID на оба режима); 0xFEFE — BT.
            0x8009 => ConnectionType::Usb,
            0xFEFE => ConnectionType::Bluetooth,
            // --- AULA ---
            0x004B | 0x004E => ConnectionType::Usb,
            0x004C | 0x004F => ConnectionType::Wireless2_4G,
            0x004D | 0x0050 => ConnectionType::Bluetooth,
            // Ревизия «BY Tech Gaming Keyboard» — USB (общий PID F75/F99!)
            0x010C => ConnectionType::Usb,
            // 2.4G-ресивер Compx (3554:FA09) — wireless-канал клавиатуры
            0xFA09 => ConnectionType::Wireless2_4G,
            _ => ConnectionType::Usb,
        };
        let mut bridge = Self {
            device,
            vid,
            pid,
            connection,
            model: None,
        };
        // Проводной F75 И F99 сидят на ОДНОМ PID 0x010C (KB.ini, журнал 10.51),
        // поэтому для этого PID определяем модель feature-запросом (OpenRGB
        // SinowealthControllerDetect.cpp:399). Ошибка детекции не смертельна:
        // model остаётся None, и build_device по умолчанию соберёт F75.
        if vid == 0x258A && pid == 0x010C {
            if let Ok(m) = bridge.detect_model() {
                bridge.model = Some(m);
            }
        }
        Ok(bridge)
    }

    /// Определить идентификатор модели SinoWealth через командный канал 0x06.
    /// Запрос: 06 82 01 00 01 00 06 (+ пад до 520), ответ — Feature 06 520 б,
    /// `model_id` в response[13] (0xCD = AULA F75, 0xA4 = AULA F99).
    /// Возвращает raw-байт; неизвестные значения трактует caller.
    pub fn detect_model(&self) -> hidapi::HidResult<u8> {
        let mut buf = vec![0u8; PACKET_LEN];
        buf[0] = 0x06; // report id
        buf[1] = 0x82;
        buf[2] = 0x01;
        buf[3] = 0x00;
        buf[4] = 0x01;
        buf[5] = 0x00;
        buf[6] = 0x06;
        self.device.send_feature_report(&buf)?;
        let mut resp = vec![0u8; 520 + 1];
        resp[0] = 0x06;
        let n = self.device.get_feature_report(&mut resp)?;
        resp.truncate(n);
        Ok(resp.get(13).copied().unwrap_or(0))
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

// HidBridge передаётся между фоновым потоком (открытие после кика клавиатуры)
// и главным GTK-потоком через mpsc-канал (фикс 2026-09-13: блокирующее
// hidapi-открытие в главном потоке вешало UI на время пересоздания hidraw).
// HidDevice — обёртка над дескриптором hidraw; устройством не пользуются два
// потока ОДНОВРЕМЕННО (в фоне создаётся и отдаётся, в главном — хранится и
// используется в кнопках), поэтому перенос между потоками безопасен.
unsafe impl Send for HidBridge {}

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