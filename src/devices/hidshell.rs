// --- HID-разведка и raw-доступ ---
// Низкоуровневый инструмент для изучения протокола клавиатур.
// Позволяет открыть конкретный интерфейс, отправлять feature/output-репорты,
// читать input-репорты и сканировать номера репортов (probe).
//
// Зачем это нужно: у AULA/Ajazz на USB несколько HID-интерфейсов
// (клавиатура + вендорский интерфейс настроек). Фичур-репорты живут
// на вендорском. Открывать нужно по конкретному пути /dev/hidrawN.

use hidapi::{HidApi, HidDevice, HidResult};
use std::ffi::CString;

/// Один HID-интерфейс из device_list.
#[derive(Debug, Clone)]
pub struct Iface {
    pub path: String,
    pub usage_page: u16,
    pub usage: u16,
    pub interface_number: i32,
}

/// Собрать все интерфейсы устройства по VID/PID.
/// Возвращает каждый hidraw-путь, подходящий под пару VID:PID.
pub fn list_interfaces(api: &HidApi, vid: u16, pid: u16) -> Vec<Iface> {
    api.device_list()
        .filter(|d| d.vendor_id() == vid && d.product_id() == pid)
        .map(|d| Iface {
            path: d.path().to_string_lossy().to_string(),
            usage_page: d.usage_page(),
            usage: d.usage(),
            interface_number: d.interface_number(),
        })
        .collect()
}

/// Открыть интерфейс по пути /dev/hidrawN.
pub fn open_path(api: &HidApi, path: &str) -> HidResult<HidDevice> {
    let cstr = CString::new(path).expect("путь не должен содержать NUL");
    api.open_path(&cstr)
}

/// Отправить feature-репорт. `data[0]` — номер репорта.
pub fn send_feature(dev: &HidDevice, data: &[u8]) -> HidResult<()> {
    dev.send_feature_report(data)
}

/// Прочитать feature-репорт `id` (буфер данных длиной `len`).
/// Возвращает полный репорт, первый байт = report id.
pub fn get_feature(dev: &HidDevice, id: u8, len: usize) -> HidResult<Vec<u8>> {
    let mut buf = vec![0u8; len + 1];
    buf[0] = id;
    let n = dev.get_feature_report(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}

/// Отправить output-репорт (некоторые AULA используют его вместо feature).
pub fn send_output(dev: &HidDevice, data: &[u8]) -> HidResult<()> {
    dev.send_output_report(data)
}

/// Прочитать input-репорт. `len` — ожидаемая длина данных, `timeout_ms` — таймаут
/// (0 = бесконечно, отрицательное = без ожидания).
pub fn read_input(dev: &HidDevice, len: usize, timeout_ms: i32) -> HidResult<Vec<u8>> {
    let mut buf = vec![0u8; len + 1];
    let n = dev.read_timeout(&mut buf, timeout_ms)?;
    buf.truncate(n);
    Ok(buf)
}

/// Скан всех номеров feature-репортов (0x00..=0xFF).
/// Возвращает только те, что вернули данные (>= 2 байта).
pub fn probe_features(dev: &HidDevice, len: usize) -> Vec<(u8, Vec<u8>)> {
    let mut hit = Vec::new();
    for id in 0u16..=255 {
        let id = id as u8;
        if let Ok(v) = get_feature(dev, id, len) {
            if v.len() >= 2 {
                hit.push((id, v));
            }
        }
    }
    hit
}

/// Текстовое представление байтов: «02 01 00 FF».
pub fn hex_dump(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}