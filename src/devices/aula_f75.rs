// --- AULA F75 ---
// Реализация KeyboardDevice для AULA F75 (USB / 2.4G / BT).
//
// Особенности:
//   - 75% раскладка без NumPad.
//   - Ролик регулировки громкости (без click-кнопки).
//   - Нет TFT-экрана.
//
// ПРОТОКОЛ (разведка 2026-09-11, подтверждено на железе 258a:010c):
//   report ID 0x06 — главный канал команд, тип Feature, пакеты ровно 520 байт
//   (id + 519 payload). Формат payload: [CMD A0 A1 A2 A3 L0 L1 <data...>].
//
//   Чтение конфига (работает):
//     SET feature  06 84 00 00 01 00 80 00 (+ нулевой пад до 520)
//     GET feature 06 520 → 136 байт: id + эхо-заголовок + 128 config + 5A A5.
//     В data-области с offset 58 — эффект-таблица пар (a,b):
//       a = яркость (0x09 полная, 0x01 тусклая),
//       b = (speed << 4) | color_idx.
//
//   Direct-режим (host-rendering, работает — клавиатура сама показывает цвета):
//     SET feature  06 08 00 00 01 00 7A 01 + (R,G,B)×N с offset 0x08, до 520 б.
//     Нужен ПОСТОЯННЫЙ поток кадров (keepalive ~0.5–1с), иначе подсветка гаснет.
//     N = 90 (матрица LED по OpenRGB SinowealthKeyboard10cController, модель 0xCD).
//
//   Цветовая таблица (VERIFIED cloudglides 2026-08-24, журнал 10.59/10.66):
//     read 0x8a / write 0x0a, addr 00 00 01 00, len 0x0200 (+ пад до 520),
//     512 байт = 128 × [RR GG BB 00]. Хранение держится; ОТОБРАЖЕНИЕ требует
//     active self-define (custom) режима прошивки — не ревёрснут (см. ниже).
//   РЕАЛИЗОВАННЫЕ МЕТОДЫ (по состоянию на 2026-09-17, «добить»):
//     - render_frame()            — direct-канал (host-rendering) ✓
//     - read_config/write_config  — конфиг-регион ✓
//     - save_hardware_effect/light/effect_with_style — запись в память ✓
//     - remap_key/set_fn_layer    — keymap (cloudglides+openaula) ✓
//     - set_rgb_mode/brightness/speed — ОБЁРТКИ над записью в память
//       (убраны выдуманные 0x02/0x04/0x05; применяются после кика/перезапуска)
//     - set_rgb_color             — RMW цветовой таблицы 0x8a/0x0a (USB)
//     - set_polling_rate/debounce/sleep_timer — честный Unsupported
//       (реальных команд в открытых источниках НЕТ, журнал 10.59/10.66)

use super::hid::HidBridge;
use super::{Error, KeyboardDevice, Result};
use crate::types::*;
use std::cell::Cell;

/// Количество клавиш в 75% раскладке F75 (без NumPad).
pub const KEY_COUNT: u16 = 84;
/// Количество светодиодов в матрице подсветки (см. OpenRGB, модель 0xCD).
pub const RGB_LED_COUNT: usize = 90;

/// Аппаратные эффекты прошивки — селектор `data[10]` в конфиг-регионе.
/// Коды собраны эмпирически на железе: переключение Fn+`\` на клавиатуре
/// меняет `data[10]` и сохраняет его в память (сессия 2026-09-12).
/// Порядок в цикле клавиатуры: 08 → 0B → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 0A → 0B.
pub const HW_EFFECTS: &[(u8, &str)] = &[
    (0x02, "Дыхание"),
    (0x03, "Мерцание / смена цветов"),
    (0x04, "Реактивная (вспышка при наборе)"),
    (0x05, "Падающие блоки"),
    (0x06, "Радуга с пульсацией"),
    (0x07, "Диско (волна вправо-влево)"),
    (0x08, "Радуга (пульсирующая)"),
    (0x0A, "Змейка по рядам"),
    (0x0B, "Перелив-волна"),
    // 0x0C — встречен в дефолтном конфиге на железе (2026-09-13, идёт после
    // Fn+` перещёлкивания 0B→0C→0A). Название не выяснено.
    (0x0C, "Эффект 0C (имя не выяснено)"),
];

/// Аппаратные эффекты РЕСИВЕРА 2.4G — селектор `led[10]` в LED config (0x13).
/// Эмпирическая карта, снята на железе 2026-09-15 (прогон всех кодов серией,
/// владелец описал визуально). Коды НЕ совпадают с dnviti-PROTOCOL.md (тот — для
/// другого чипа) и НЕ синонимичны USB-кодам `data[10]` из `HW_EFFECTS`.
/// `led[1]` селектором НЕ является (запись туда визуально ничего не меняла).
/// Пишутся через 0x04 сразу во флеш — держатся без приложения.
pub const WL_EFFECTS: &[(u8, &str)] = &[
    (0x01, "Радужная статика"),
    (0x03, "Смена цветов (статично)"),
    (0x04, "Линия"),
    (0x05, "Дождь"),
    (0x06, "Дыхание"),
    (0x07, "Мерцание разных букв"),
    (0x08, "Змейка"),
    (0x0A, "Пульсация из центра по сторонам"),
    (0x0D, "Ободок по периметру"),
    (0x0F, "Волны между рядами (сверху вниз)"),
    (0x10, "Волны вправо-влево от центра"),
    (0x11, "Эффект 0x11 (не выяснен)"),
    (0x12, "Эффект 0x12 (не выяснен)"),
    (0x13, "Выкл (нет подсветки)"),
];

/// Найти имя аппаратного эффекта по коду (для отладки/GUI).
pub fn hw_effect_name(code: u8) -> Option<&'static str> {
    HW_EFFECTS.iter().find(|(c, _)| *c == code).map(|(_, n)| *n)
}

/// Найти имя эффекта ресивера по коду (для отладки/GUI).
pub fn wl_effect_name(code: u8) -> Option<&'static str> {
    WL_EFFECTS.iter().find(|(c, _)| *c == code).map(|(_, n)| *n)
}

/// Номер Feature-репорта главного канала.
const REPORT_FEATURE: u8 = 0x06;
/// Длина пакета (без report id).
const PACKET_LEN: usize = 520;

// --- Ремап (keymap) F75: реальный протокол ---
// Первоисточники (проверены на железе авторами):
//   cloudglides/aula-f75-linux-drivers  (docs/PROTOCOL.md, 2026-08-24)
//   not-ayan/openaula                   (README.md, реверс таблицы)
// Журнал AGENTS.md 10.59–10.61. Формат:
//   - Вся таблица слоя — 512 байт = 128 слотов × 4 [page, 0x00, 0x00, usage]
//     (page 0x00 = HID usage page 0x07 клавиатура/кейпад, 0x02 = 0x0C consumer;
//     unbind = page 0x00 usage 0x00 = [0,0,0,0]).
//   - Валидные слоты 0..126 (слот 127 перекрыл бы трейлер 5A A5 на [518..519]).
//   - Залочены прошивкой F1–F12: [12,18,24,30,36,42,48,54,60,66,72,78].
//   - Физическая Fn — слот #53 (байт 212), родное [13,0,0,0]; «тупая»
//     перезапись слота ломает Fn (openaula: нужна точечная запись) — защищаем.
// Чтение (openaula): запрос 06 83 00 <layer> 01 00 00 02 (+пад до 520) →
// GET 06 520: payload 512 б с offset 8 (report id + эхо-заголовок).
// Запись (cloudglides blob, 520 б): 06 03 <layer> 00 01 00 00 FF + слоты +
// трейлер 5A A5. Пишем ТОЛЬКО изменённый слот (read-modify-write); подтверждается
// read-back. Формат чтения слоя FN1 — по аналогии с blob (байт [2] = слой),
// живая сверка отложена (журнал 10.62).
const KEYMAP_SLOTS: usize = 128;
const KEYMAP_SLOT_BYTES: usize = 4;
const KEYMAP_BYTES: usize = KEYMAP_SLOTS * KEYMAP_SLOT_BYTES;
/// Залоченные прошивкой F1–F12 (cloudglides PROTOCOL.md) — запись запрещена.
const LOCKED_F_KEYS: [u8; 12] = [12, 18, 24, 30, 36, 42, 48, 54, 60, 66, 72, 78];
/// Слот физической Fn-клавиши (openaula) — перезапись ломает Fn.
const FN_SLOT: u8 = 53;

pub struct AulaF75 {
    bridge: HidBridge,
    /// Последняя установленная яркость LED (0..4, масштаб прошивки) на
    /// ресивере 2.4G — восстанавливается при включении света
    /// (`apply_light_persistent(true)`); по умолчанию полная (4).
    last_wl_brightness: Cell<u8>,
}

impl AulaF75 {
    pub fn open(bridge: HidBridge) -> Self {
        Self {
            bridge,
            last_wl_brightness: Cell::new(4),
        }
    }

    /// Собрать 520-байтовый пакет с одинаковым заголовком.
    fn packet(header: &[u8]) -> Vec<u8> {
        let mut buf = vec![0u8; PACKET_LEN];
        buf[0] = REPORT_FEATURE;
        buf[1..=header.len().min(PACKET_LEN - 1)].copy_from_slice(header);
        buf
    }

    /// Прочитать data-область конфига (128 байт) с эхо-заголовком.
    /// Возвращает готовый снимок: [report_id, эхо (7), config (128), 5A, A5].
    fn read_config(&self) -> Result<Vec<u8>> {
        // Запрос чтения: 06 84 00 00 01 00 80 00 (+ пад до 520)
        let req = Self::packet(&[0x84, 0x00, 0x00, 0x01, 0x00, 0x80, 0x00]);
        self.bridge.send_feature(&req)?;
        let report = self.bridge.read_feature(REPORT_FEATURE, PACKET_LEN)?;
        if report.len() < 8 {
            return Err(Error::Protocol("пустой ответ конфига".into()));
        }
        Ok(report)
    }

    /// Записать конфиг-регион (128 байт с маркером 5A A5 на конце) в память
    /// клавиатуры. Пакет — 06 04 00 00 01 00 80 00 + data + пад до 520.
    /// После записи клавиатура перезапускается (кик) — это и есть commit
    /// (журнал 10.24/10.25: readback подтверждает хранение, свет применяется).
    fn write_config(&self, data: &[u8]) -> Result<()> {
        if data.len() != 128 {
            return Err(Error::Protocol(
                "write_config: нужен ровно 128 байт конфига (с маркером 5A A5)".into(),
            ));
        }
        let mut buf = Self::packet(&[0x04, 0x00, 0x00, 0x01, 0x00, 0x80, 0x00]);
        buf[8..136].copy_from_slice(data);
        self.bridge.send_feature(&buf)?;
        Ok(())
    }

    /// Прочитать цветовую таблицу клавиш — 512 байт = 128 × [RR GG BB 00].
    /// Канал USB: CMD 0x8a, addr 00 00 01 00, len 0x0200 (+ пад до 520).
    /// VERIFIED cloudglides (журнал 10.59/10.66); в 10.5 тот же адрес давал
    /// Protocol error — вероятно из-за неверного пада/порядка len.
    fn read_color_table(&self) -> Result<Vec<u8>> {
        if self.bridge.connection != ConnectionType::Usb {
            return Err(Error::Unsupported(
                "цветовая таблица ресивера 2.4G — отдельный канал (0x02/0x1C), не реализован".into(),
            ));
        }
        let req = Self::packet(&[0x8a, 0x00, 0x00, 0x01, 0x00, 0x00, 0x02]);
        self.bridge.send_feature(&req)?;
        let report = self.bridge.read_feature(REPORT_FEATURE, PACKET_LEN)?;
        if report.len() < 8 + 512 {
            return Err(Error::Protocol(format!(
                "цветовая таблица прочитана неполностью: {} байт (ожидается {})",
                report.len(),
                8 + 512
            )));
        }
        Ok(report[8..8 + 512].to_vec())
    }

    /// Записать цветовую таблицу целиком (512 байт). VERIFIED cloudglides.
    fn write_color_table(&self, table: &[u8]) -> Result<()> {
        if table.len() != 512 {
            return Err(Error::Protocol(
                "write_color_table: нужен ровно 512 байт (128 × 4)".into(),
            ));
        }
        if self.bridge.connection != ConnectionType::Usb {
            return Err(Error::Unsupported(
                "цветовая таблица ресивера 2.4G — отдельный канал (0x02/0x1C), не реализован".into(),
            ));
        }
        let mut buf = Self::packet(&[0x0a, 0x00, 0x00, 0x01, 0x00, 0x00, 0x02]);
        buf[8..8 + 512].copy_from_slice(table);
        self.bridge.send_feature(&buf)?;
        Ok(())
    }

    /// Записать яркость и/или скорость АКТИВНОГО эффекта в его слот
    /// эффект-таблицы. USB — через конфиг-регион (write_config, применяется
    /// после кика/перезапуска); ресивер 2.4G — через LED config (0x04, сразу
    /// во флеш). Слот = 58 + 2*(селектор-1), пара (a=яркость 0..=9,
    /// b=(скорость<<4)|цвет) — эмпирика 10.28/10.49. Младшая тетрада b
    /// (цвет) при записи скорости сохраняется.
    fn set_active_style(&self, brightness: Option<u8>, speed: Option<u8>) -> Result<()> {
        if self.bridge.connection == ConnectionType::Wireless2_4G {
            let mut led = self.wl_led_config()?;
            let sel = led.get(10).copied().unwrap_or(1);
            let slot = 58 + 2 * (sel as usize).saturating_sub(1);
            if slot + 1 >= led.len() {
                return Err(Error::Protocol(format!(
                    "слот активного эффекта {slot} вне LED config (len {})",
                    led.len()
                )));
            }
            if let Some(b) = brightness {
                let bb = ((b as u16 * 9) / 255).min(9) as u8;
                led[slot] = bb;
                self.last_wl_brightness.set(bb);
            }
            if let Some(s) = speed {
                let ss = ((s as u16 * 4) / 255).min(4) as u8;
                led[slot + 1] = (ss << 4) | (led[slot + 1] & 0x0F);
            }
            return self.wl_write_pages(0x04, &led);
        }
        let snap = self.read_config()?;
        if snap.len() < 136 {
            return Err(Error::Protocol("короткий ответ конфига".into()));
        }
        let mut data = vec![0u8; 128];
        data.copy_from_slice(&snap[8..136]);
        let sel = data.get(10).copied().unwrap_or(1);
        let slot = 58 + 2 * (sel as usize).saturating_sub(1);
        if slot + 1 >= 128 {
            return Err(Error::Protocol(format!(
                "слот активного эффекта {slot} вне конфиг-региона (128)"
            )));
        }
        if let Some(b) = brightness {
            let bb = ((b as u16 * 9) / 255).min(9) as u8;
            data[slot] = bb;
        }
        if let Some(s) = speed {
            let ss = ((s as u16 * 4) / 255).min(4) as u8;
            data[slot + 1] = (ss << 4) | (data[slot + 1] & 0x0F);
        }
        self.write_config(&data)?;
        Ok(())
    }

    /// Маппинг host-режима на аппаратный код эффекта конфига (data[10], USB).
    /// Приблизительный (в цикле Fn+` только радужные эффекты, однотонной
    /// статики нет — журнал 10.26); уточнять на живом железе.
    fn usb_effect_for_mode(mode: RgbMode) -> u8 {
        match mode {
            RgbMode::Breathing => 0x02,      // Дыхание
            RgbMode::Reactive => 0x04,       // Реактивная
            RgbMode::Rain => 0x05,           // Падающие блоки (дождь)
            RgbMode::Ripple => 0x0A,         // пульсация (совпадает с ресивером)
            RgbMode::SpectrumCycle => 0x08,  // Радуга
            RgbMode::Wave => 0x0B,           // Перелив-волна
            RgbMode::Static | RgbMode::Custom => 0x03, // Смена цветов (статично)
        }
    }

    // --- Ресивер 2.4G (Compx 3554:FA09) ---
    // Отдельное USB-устройство: клавиатура в режиме 2.4G НЕ видна напрямую,
    // команды идут через ресивер. Протокол — output/input report ID 0x13
    // (20 байт), как у F99 Pro (dnviti/aula-x99-pro, PROTOCOL.md).
    // Версия + заряд АКБ приходят в ответе на команду 0x05 (PSD/identity):
    //   0x05 → [13][05]… PSD … + [13][0A][01][00][04] fw btry type
    //   byte[5]: фирма hi, byte[6]: заряд (0x64 = 100%), byte[7]: 0x01 = wireless.
    // Проверено на железе (2026-09-14): ресивер F75 на 0x0B (как у F99 Pro)
    // НЕ отвечает, работает именно 0x05 — PSD и version приходят вместе.
    // Если клавиатура спит на линке — ответа не будет, пока её не разбудят
    // нажатием клавиши (dnviti: _send_wireless, retries=1). Делаем несколько
    // попыток с паузой, чтобы поймать ответ после пробуждения части клавиш.
    fn wireless_battery(&self) -> Result<Option<u8>> {
        const REPORT_WIRELESS: u8 = 0x13;
        let mut req = [0u8; 20];
        req[0] = REPORT_WIRELESS;
        req[1] = 0x05;
        // 3 попытки по ~1.2 c с паузой 300 мс (данный ресивер спит между
        // таймаутом linкa; ножатия клавиш «будят» канал).
        for _ in 0..3 {
            self.bridge.send_output(&req)?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1200);
            let mut buf = [0u8; 64];
            while std::time::Instant::now() < deadline {
                let n = self.bridge.read_input(&mut buf, 150)?;
                if n >= 8 && buf[0] == REPORT_WIRELESS && buf[1] == 0x0A {
                    let ty = buf[7];
                    let btr = buf[6];
                    if ty == 0x01 && btr <= 100 {
                        return Ok(Some(btr));
                    }
                    return Ok(None);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        // Ресивер молчит — клавиатура, вероятно, спит на линке.
        Ok(None)
    }

    // --- Ресивер 2.4G: страничный протокол RGB (0x13, dnviti PROTOCOL.md) --
    // Чтение/запись конфигурации подсветки через output/input report 0x13:
    //   read  0x44 LED config (128 б), 0x49 RGB table (~490 б)
    //   write 0x04 LED config, 0x09 RGB table  (write = read & 0x0F)
    // Ответы идут страницами: [13][resp][total][idx][len][data…][cs].
    // Ресивер отвечает только когда клавиатура не спит на линке (dnviti:
    // «press any key to wake, then retry»), поэтому делаем несколько попыток.

    /// Прочитать минимально одну «страницу» входящего потока (drop-buffer).
    fn drain_input(&self, timeout_ms: i32) {
        let mut buf = [0u8; 64];
        let _ = self.bridge.read_input(&mut buf, timeout_ms);
    }

    /// Чтение многостраничного блока `cmd` (минимум `need` байт).
    /// Собирает все страницы, сортируя по page index. 2 попытки × ~1.5 с.
    fn wl_read_pages(&self, cmd: u8, need: usize) -> Result<Vec<u8>> {
        const REPORT: u8 = 0x13;
        for _attempt in 0..2 {
            // Дренажим «хвост» предыдущих ответов, чтобы не принять чужую страницу.
            self.drain_input(20);
            let mut req = [0u8; 20];
            req[0] = REPORT;
            req[1] = cmd;
            self.bridge.send_output(&req)?;

            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
            let mut pages: Vec<(usize, Vec<u8>)> = Vec::new();
            let mut total: usize;
            while std::time::Instant::now() < deadline {
                let mut buf = [0u8; 64];
                let n = self.bridge.read_input(&mut buf, 200)?;
                if n < 5 || buf[0] != REPORT {
                    continue;
                }
                let resp_cmd = buf[1];
                // Принимаем собственную команду, её write-вариант (cmd&0x3F)
                // и универсальный ACK 0x0A — как в read_multi_page у dnviti.
                if resp_cmd != cmd && resp_cmd != (cmd & 0x3F) && resp_cmd != 0x0A {
                    continue;
                }
                total = buf[2] as usize;
                let idx = buf[3] as usize;
                let plen = (buf[4] as usize).min(14);
                let mut data = Vec::with_capacity(plen);
                for i in 0..plen {
                    data.push(buf[5 + i]);
                }
                if let Some(slot) = pages.iter_mut().find(|(i, _)| *i == idx) {
                    slot.1 = data; // замена (контрольная страница повторно)
                } else {
                    pages.push((idx, data));
                }
                if total > 0 && pages.len() >= total {
                    break; // все страницы пришли
                }
            }

            let mut out = Vec::new();
            pages.sort_by_key(|(i, _)| *i);
            for (_idx, d) in pages {
                out.extend_from_slice(&d);
            }
            if out.len() >= need {
                return Ok(out);
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        Err(Error::Protocol(
            "клавиатура спит на линке 2.4G — нажмите клавишу и повторите".into(),
        ))
    }

    /// Запись многостраничного блока `data` командой `cmd` (как write_multi_page).
    fn wl_write_pages(&self, cmd: u8, data: &[u8]) -> Result<()> {
        const REPORT: u8 = 0x13;
        let total = (data.len() + 13) / 14;
        for page_idx in 0..total {
            let off = page_idx * 14;
            let chunk = &data[off..(off + 14).min(data.len())];
            let mut buf = [0u8; 20];
            buf[0] = REPORT;
            buf[1] = cmd;
            buf[2] = total as u8;
            buf[3] = page_idx as u8;
            buf[4] = chunk.len() as u8;
            buf[5..5 + chunk.len()].copy_from_slice(chunk);
            self.bridge.send_output(&buf)?;
            // Между страницами — пауза + проглатывание ACK (dnviti).
            std::thread::sleep(std::time::Duration::from_millis(20));
            self.drain_input(40);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        Ok(())
    }

    /// Прочитать LED config ресивера (128 байт).
    fn wl_led_config(&self) -> Result<Vec<u8>> {
        let d = self.wl_read_pages(0x44, 128)?;
        if d.len() < 128 {
            return Err(Error::Protocol(format!(
                "LED config прочитан неполностью: {} байт",
                d.len()
            )));
        }
        Ok(d[..128].to_vec())
    }

    /// Прочитать RGB-таблицу ресивера (~490 байт: 20 эффектов × 7 цветов × 3).
    fn wl_rgb_table(&self) -> Result<Vec<u8>> {
        let d = self.wl_read_pages(0x49, 480)?;
        if d.len() < 480 {
            return Err(Error::Protocol(format!(
                "RGB table прочитана неполностью: {} байт",
                d.len()
            )));
        }
        Ok(d)
    }

    /// Маппинг host-режима на HW-эффект ресивера (селектор `led[10]`,
    /// эмпирическая карта 2026-09-15).
    fn wl_effect_for_mode(mode: RgbMode) -> u8 {
        match mode {
            RgbMode::Static => 0x03,       // Смена цветов (статично)
            RgbMode::Breathing => 0x06,    // Дыхание
            RgbMode::Wave => 0x10,         // Волны вправо-влево от центра
            RgbMode::SpectrumCycle => 0x01, // Радужная статика
            RgbMode::Ripple => 0x0A,       // Пульсация из центра
            RgbMode::Rain => 0x05,         // Дождь
            RgbMode::Reactive => 0x07,     // Мерцание разных букв (отклик)
            RgbMode::Custom => 0x03,       // Ближайший статичный
        }
    }

    /// Применить эффект/цвет/яркость/скорость в память ресивера (0x13).
    /// Запись идёт сразу во флеш — держится без приложения. Порядок по
    /// dnviti (cmd_set_led): LED config → RGB table → LED config ПОВТОРНО
    /// ПОСЛЕДНИМ (иначе «LED changes flash and revert»).
    fn wireless_apply_effect(
        &self,
        mode: RgbMode,
        color: Option<Color>,
        brightness: u8,
        speed: u8,
    ) -> Result<()> {
        let mut led = self.wl_led_config()?;
        // Селектор эффекта на ресивере — led[10] (эмпирика 2026-09-15).
        led[10] = Self::wl_effect_for_mode(mode);
        // Яркость/скорость ЖИВУТ В СЛОТАХ ЭФФЕКТОВ: слот активного эффекта =
        // [58 + 2*(led[10]-1)], пара (a=яркость, b=(speed<<4)|color). Записываем
        // туда, НЕ трогая led[3]/led[4] (они на ресивере не используются,
        // журнал 10.49, эмпирика 2026-09-15).
        let slot = 58 + 2 * ((led[10] as usize).saturating_sub(1));
        // UI-масштаб (0..=255) → шкала прошивки: яркость 0..=9, скорость 0..=4.
        let b = (((brightness as u16) * 9) / 255).min(9) as u8;
        let s = (((speed as u16) * 4) / 255).min(4) as u8;
        led[slot] = b;
        // Сохраняем младшую тетраду b-байта (цвет) при перезаписи скорости.
        led[slot + 1] = (s << 4) | (led[slot + 1] & 0x0F);
        self.last_wl_brightness.set(b);
        let want_color = color.is_some();
        if let Some(c) = color {
            led[6] = 0; // режим «одиночный цвет» (0=single)
            let mut rgb = self.wl_rgb_table()?;
            // Правим первый цветовой слот (offsets 0..2) у каждого эффекта —
            // это и есть «выбранный цвет» для Steady/Breathing/… (dnviti).
            for off in (0..rgb.len().saturating_sub(2)).step_by(21) {
                rgb[off] = c.r;
                rgb[off + 1] = c.g;
                rgb[off + 2] = c.b;
            }
            self.wl_write_pages(0x09, &rgb)?;
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        // LED config пишем ПЕРВЫМ и ПОСЛЕДНИМ (dnviti: must be last to stick).
        self.wl_write_pages(0x04, &led)?;
        if want_color {
            std::thread::sleep(std::time::Duration::from_millis(300));
            self.wl_write_pages(0x04, &led)?;
        }
        Ok(())
    }

    /// Включить/выключить свет на ресивере в память.
    /// НЕ только общая яркость led[3]: ресивер берёт яркость из слота
    /// активного эффекта (пары `a` в RGB-таблице [58..125]), поэтому для
    /// гашения зануляем `a` во ВСЕХ слотах, для включения — поднимаем `a=09`
    /// (аналог журнала 10.36 для USB: «тумблер» — слоты эффектов, а не led[3]).
    fn wireless_apply_light(&self, on: bool) -> Result<()> {
        let mut led = self.wl_led_config()?;
        led[3] = if on {
            self.last_wl_brightness.get().max(1)
        } else {
            0
        };
        // Слоты эффектов: 34 пары (a=яркость слота, b=скорость|цвет) с offset 58.
        for i in (58..=125).step_by(2) {
            led[i] = if on { 0x09 } else { 0x00 };
        }
        self.wl_write_pages(0x04, &led)?;
        Ok(())
    }

    // --- Ремап (keymap) F75: реальный протокол ---
    // Транспорт — тот же feature-канал 0x06/520 б, что и конфиг. Ремап работает
    // ТОЛЬКО по USB (wired): на ресивере 2.4G / BT запись настроек через радио
    // не реализована ни в одном проверенном источнике (cloudglides: «2.4G —
    // только live per-key стрим»).

    /// Прочитать таблицу keymap слоя `layer` (0=Default, 1=FN1) — ровно 512 байт.
    /// Путь чтения — openaula (CMD 0x83): запрос `06 83 00 <layer> 01 00 00 02` +
    /// пад до 520 → GET 06 520. Ответ: report id + эхо-заголовок (8 б) + payload
    /// 512 б (128 слотов × 4). Формат чтения FN1 — по аналогии (байт [2] = слой),
    /// живая сверка отложена (журнал 10.62).
    fn read_keymap_raw(&self, layer: u8) -> Result<Vec<u8>> {
        if self.bridge.connection != ConnectionType::Usb {
            return Err(Error::Unsupported(
                "ремап F75 работает только по USB (wired); на 2.4G/BT запись настроек не реализована".into(),
            ));
        }
        let req = Self::packet(&[0x83, 0x00, layer, 0x01, 0x00, 0x00, 0x02]);
        self.bridge.send_feature(&req)?;
        let report = self.bridge.read_feature(REPORT_FEATURE, PACKET_LEN)?;
        if report.len() < 8 + KEYMAP_BYTES {
            return Err(Error::Protocol(format!(
                "keymap слоя {layer}: ответ неполный ({} байт, ожидается {})",
                report.len(),
                8 + KEYMAP_BYTES
            )));
        }
        Ok(report[8..8 + KEYMAP_BYTES].to_vec())
    }

    /// Записать таблицу keymap слоя `layer` (cloudglides blob, 520 б):
    /// `06 03 <layer> 00 01 00 00 FF` + 128 слотов + трейлер `5A A5` на [518..519].
    /// `table` — ровно 512 байт слотов; пишем переданную таблицу целиком.
    fn write_keymap_raw(&self, layer: u8, table: &[u8]) -> Result<()> {
        if table.len() != KEYMAP_BYTES {
            return Err(Error::Protocol(
                "write_keymap: нужна таблица ровно 512 байт".into(),
            ));
        }
        if self.bridge.connection != ConnectionType::Usb {
            return Err(Error::Unsupported(
                "ремап F75 работает только по USB (wired); на 2.4G/BT запись настроек не реализована".into(),
            ));
        }
        let mut buf = Vec::with_capacity(PACKET_LEN);
        buf.push(REPORT_FEATURE);
        buf.extend_from_slice(&[0x03, layer, 0x00, 0x01, 0x00, 0x00, 0xFF]);
        buf.extend_from_slice(table);
        // Трейлер перекрывает последние 2 байта невалидного слота 127 — намеренно.
        buf[PACKET_LEN - 2] = 0x5A;
        buf[PACKET_LEN - 1] = 0xA5;
        debug_assert_eq!(buf.len(), PACKET_LEN);
        self.bridge.send_feature(&buf)?;
        Ok(())
    }

    /// Кодирование действия в 4 байта слота [page, 0x00, 0x00, usage].
    /// Пока поддерживаются только «клавиша→клавиша» (page 0x00) и «медиа»
    /// (page 0x02); модификаторы/слои/макросы — ждут живого теста (журнал 10.62).
    fn slot_encode(action: &KeyAction) -> Result<[u8; 4]> {
        match action {
            KeyAction::Press(usage) => Ok([0x00, 0x00, 0x00, (*usage & 0xFF) as u8]),
            KeyAction::Media(usage) => Ok([0x02, 0x00, 0x00, (*usage & 0xFF) as u8]),
            _ => Err(Error::Unsupported(
                "F75: поддержан ремап «клавиша→клавиша» и «медиа»; модификаторы/слои/макросы — ждут живого теста".into(),
            )),
        }
    }

    /// Декодирование 4 байт слота в действие. Слоты со служебными page
    /// (13 = Fn, и т.п.) возвращают None — их НЕ трогаем при записи (RMW).
    fn slot_decode(bytes: [u8; 4]) -> Option<KeyAction> {
        match bytes[0] {
            0x00 => {
                let usage = bytes[3] as u16;
                if usage == 0 {
                    None // unbind
                } else {
                    Some(KeyAction::Press(usage))
                }
            }
            0x02 => Some(KeyAction::Media(bytes[3] as u16)),
            _ => None, // page 13 (Fn) и прочие служебные — не разбираем
        }
    }

    /// Проверка защищённых/невалидных слотов перед записью (RMW).
    /// Залоченные прошивкой F1–F12 (cloudglides) и физическая Fn (#53, openaula)
    /// возвращают критическую ошибку — запись в них запрещена.
    fn check_writable_slot(slot: u8) -> Result<()> {
        if slot == FN_SLOT {
            return Err(Error::Protocol(
                "слот #53 — физическая Fn-клавиша, перезапись ломает Fn (openaula); защищено".into(),
            ));
        }
        if LOCKED_F_KEYS.contains(&slot) {
            return Err(Error::Protocol(
                format!("слот #{slot} — F1–F12 залочены прошивкой (cloudglides); запись запрещена"),
            ));
        }
        if slot as usize >= KEYMAP_SLOTS {
            return Err(Error::Protocol(
                format!("слот #{slot} вне диапазона keymap (0..{})", KEYMAP_SLOTS - 1),
            ));
        }
        Ok(())
    }
}

impl KeyboardDevice for AulaF75 {
    fn name(&self) -> &str {
        match self.bridge.connection {
            ConnectionType::Usb => "AULA F75 (USB)",
            ConnectionType::Wireless2_4G => "AULA F75 (2.4 GHz)",
            ConnectionType::Bluetooth => "AULA F75 (Bluetooth)",
        }
    }

    fn connection_type(&self) -> ConnectionType {
        self.bridge.connection
    }

    fn is_connected(&self) -> bool {
        self.read_config().is_ok()
    }

fn vid_pid(&self) -> (u16, u16) {
        (self.bridge.vid, self.bridge.pid)
    }

    fn led_count(&self) -> usize {
        RGB_LED_COUNT
    }

    fn led_cols(&self) -> usize {
        RGB_COLS
    }

    // --- RGB ----------------------------------------------------------
    // Реальные команды: render_frame() (direct host-rendering, подтверждён),
    // чтение конфига, запись эффекта/света в память, цветовая таблица.
    // set_rgb_* — честные обёртки (см. ниже), не вымысел.
    fn render_frame(&self, colors: &[Color]) -> Result<()> {
        // direct-режим: 06 08 00 00 01 00 7A 01 + (R,G,B)×N, пад до 520
        let mut buf = Self::packet(&[0x08, 0x00, 0x00, 0x01, 0x00, 0x7A, 0x01]);
        for (i, c) in colors.iter().take(RGB_LED_COUNT).enumerate() {
            let o = 8 + i * 3;
            buf[o] = c.r;
            buf[o + 1] = c.g;
            buf[o + 2] = c.b;
        }
        self.bridge.send_feature(&buf)?;
        Ok(())
    }

    // --- RGB (аппаратные методы) -------------------------------------
    // Выдуманные cmd-id (0x02–0x05) УБРАНЫ (журнал 10.66): отдельной live-
    // команды «режим/яркость/скорость» у проводного F75 НЕТ — эффекты живут
    // только в конфиг-регионе (селектор + слоты) либо в host-rendering.
    // Методы ниже — ЧЕСТНЫЕ ОБЁРТКИ над записью в память (вариант владельца
    // «б»): применяются после кика/перезапуска клавиатуры. Live-эффекты —
    // только через render_frame() (host-движок), см. cli.rs/rgb_studio.rs.
    fn set_rgb_mode(&self, mode: RgbMode) -> Result<()> {
        let code = match self.bridge.connection {
            ConnectionType::Wireless2_4G => Self::wl_effect_for_mode(mode),
            _ => Self::usb_effect_for_mode(mode),
        };
        self.save_hardware_effect(code)
    }

    // Пери-кей цвет по USB: read-modify-write цветовой таблицы (0x8a/0x0a).
    // Запись держится и читается (VERIFIED cloudglides, журнал 10.59/10.66),
    // но ОТОБРАЖЕНИЕ требует активного self-define (custom) режима прошивки,
    // который на F75 не ревёрснут — живой цвет рисует render_frame().
    // Ресивер 2.4G: отдельный канал (0x02/0x1C) — не реализован.
    fn set_rgb_color(&self, key: KeyId, color: Color) -> Result<()> {
        if self.bridge.connection != ConnectionType::Usb {
            return Err(Error::Unsupported(
                "per-key цвет на ресивере 2.4G — канал 0x02/0x1C, не реализован".into(),
            ));
        }
        let idx = key.0 as usize;
        if idx >= 128 {
            return Err(Error::Protocol(format!(
                "KeyId {idx} вне диапазона цветовой таблицы (0..127)"
            )));
        }
        let mut tbl = self.read_color_table()?;
        let off = idx * 4;
        tbl[off] = color.r;
        tbl[off + 1] = color.g;
        tbl[off + 2] = color.b;
        tbl[off + 3] = 0x00;
        self.write_color_table(&tbl)
    }

    // Яркость пишется в слот АКТИВНОГО эффекта (data[10] / led[10]):
    // 58 + 2*(селектор-1), байт a в 0..=9. Применяется после кика (USB)
    // или сразу во флеш (ресивер).
    fn set_rgb_brightness(&self, level: u8) -> Result<()> {
        self.set_active_style(Some(level), None)
    }

    // Скорость пишется в старшую тетраду байта b слота активного эффекта
    // (цвет в младшей тетраде сохраняется).
    fn set_rgb_speed(&self, speed: u8) -> Result<()> {
        self.set_active_style(None, Some(speed))
    }

    /// Прочитать текущий аппаратный эффект из конфига: селектор (data[10]),
    /// яркость/скорость — из СЛОТА активного эффекта (не первой пары).
    /// Только по USB: ресивер отвечает на канал 0x06 константным блоком
    /// (журнал 10.15), для 2.4G — LED config через 0x13.
    fn get_rgb_preset(&self) -> Result<Vec<RgbPreset>> {
        if self.bridge.connection != ConnectionType::Usb {
            return Err(Error::Unsupported(
                "get_rgb_preset на 2.4G: читайте LED config каналом 0x13".into(),
            ));
        }
        let data = self.read_config()?;
        if data.len() < 8 + 128 {
            return Err(Error::Protocol("короткий ответ конфига".into()));
        }
        let cfg = &data[8..8 + 128];
        let sel = cfg.get(10).copied().unwrap_or(0);
        let slot = 58 + 2 * (sel as usize).saturating_sub(1);
        let a = cfg.get(slot).copied().unwrap_or(0x09);
        let b = cfg.get(slot + 1).copied().unwrap_or(0);
        Ok(vec![RgbPreset {
            name: format!("{} (из прошивки)", hw_effect_name(sel).unwrap_or("Эффект")),
            mode: RgbMode::Custom,
            colors: vec![],
            brightness: a,
            speed: b >> 4,
        }])
    }

    // --- Keymap ------------------------------------------------------
    // Реальный протокол ремапа (cloudglides + openaula, журнал 10.59–10.61):
    // read-modify-write — полная таблица слоя читается, меняется ТОЛЬКО целевой
    // слот, таблица пишется обратно blob'ом cloudglides. До реверса метода
    // remap здесь были ВЫДУМАННЫЕ cmd-id (0x10/0x11) — заменены.
    fn remap_key(&self, from: KeyId, to: KeyAction) -> Result<()> {
        let slot = from.0 as u8;
        Self::check_writable_slot(slot)?;
        let encoded = Self::slot_encode(&to)?;
        let mut table = self.read_keymap_raw(0)?; // слой 0 = Default
        let off = (slot as usize) * KEYMAP_SLOT_BYTES;
        table[off..off + KEYMAP_SLOT_BYTES].copy_from_slice(&encoded);
        self.write_keymap_raw(0, &table)
    }

    fn set_fn_layer(&self, layer: FnLayer) -> Result<()> {
        if layer.bindings.is_empty() {
            return Ok(()); // нечего менять: не затираем FN1 пустой записью
        }
        let mut table = self.read_keymap_raw(1)?; // слой 1 = FN1
        for (key, action) in &layer.bindings {
            let slot = key.0 as u8;
            Self::check_writable_slot(slot)?;
            let encoded = Self::slot_encode(action)?;
            let off = (slot as usize) * KEYMAP_SLOT_BYTES;
            table[off..off + KEYMAP_SLOT_BYTES].copy_from_slice(&encoded);
        }
        self.write_keymap_raw(1, &table)
    }

    fn get_current_keymap(&self) -> Result<Keymap> {
        let base_tab = self.read_keymap_raw(0)?;
        // FN1 читаем пробной попыткой: формат чтения слоя 1 пока не сверен на
        // железе (журнал 10.62), при неудаче не роняем карту целиком.
        let fn_tab = self.read_keymap_raw(1).unwrap_or_default();
        let decode = |table: &[u8]| {
            let mut out = Vec::new();
            for i in 0..KEYMAP_SLOTS.saturating_sub(1) {
                let off = i * KEYMAP_SLOT_BYTES;
                let bytes = [
                    table[off],
                    table[off + 1],
                    table[off + 2],
                    table[off + 3],
                ];
                if let Some(act) = Self::slot_decode(bytes) {
                    out.push((KeyId(i as u16), act));
                }
            }
            out
        };
        Ok(Keymap {
            base: decode(&base_tab),
            fn_layer: FnLayer {
                bindings: decode(&fn_tab),
            },
        })
    }

    // --- Screen (не поддерживается) -----------------------------------
    fn set_screen_image(&self, _frame: ScreenFrame) -> Result<()> {
        Err(Error::Unsupported("TFT-экран не поддерживается AULA F75"))
    }

    fn set_screen_brightness(&self, _level: u8) -> Result<()> {
        Err(Error::Unsupported("TFT-экран не поддерживается AULA F75"))
    }

    fn sync_system_time(&self) -> Result<()> {
        Err(Error::Unsupported("синхронизация времени не поддерживается AULA F75"))
    }

    // --- Encoder ------------------------------------------------------
    fn set_encoder_action(&self, action: EncoderAction) -> Result<()> {
        // Ролик F75 аппаратно жёстко = громкость: отдельной команды настройки
        // энкодера в протоколе НЕТ (не документирован ни одним источником,
        // журнал 10.59/10.66). Volume/Mute — штатное поведение, коммандить
        // нечем — возвращаем Ok (выдуманный cmd 0x20 убран).
        match action {
            EncoderAction::Volume | EncoderAction::Mute => Ok(()),
            _ => Err(Error::Unsupported(
                "ролик F75 поддерживает только громкость (аппаратно), переназначение не ревёрснуто",
            )),
        }
    }

    // --- Macros ---------------------------------------------------------
    fn record_macro(&self, _macro_id: u8) -> Result<()> {
        Err(Error::Unsupported("аппаратные макросы не поддерживаются AULA F75 (cloudglides: MACRO_SUPPORTED=false)"))
    }

    fn stop_macro(&self) -> Result<()> {
        Ok(())
    }

    fn set_macro(&self, _id: u8, _actions: Vec<KeyAction>) -> Result<()> {
        Err(Error::Unsupported("аппаратные макросы не поддерживаются AULA F75 (cloudglides: MACRO_SUPPORTED=false)"))
    }

    // --- System ---------------------------------------------------------
    // Выдуманные cmd 0x30/0x31/0x32 УБРАНЫ: реальных USB-команд для
    // polling rate / debounce / sleep в открытых PROTOCOL.md НЕТ (журнал
    // 10.59/10.66). На ресивере 2.4G эти параметры живут в LED config
    // (cloudglides: debounce — frame0 byte8, sleep — frame1 byte15 =
    // minutes*2) — реализация ожидает живого теста, см. журнал 10.66.
    fn set_polling_rate(&self, _rate: u16) -> Result<()> {
        Err(Error::Unsupported(
            "polling rate F75: команда не ревёрснута (нет в открытых PROTOCOL.md)",
        ))
    }

    fn set_debounce(&self, _ms: u16) -> Result<()> {
        Err(Error::Unsupported(
            "debounce F75: команда не ревёрснута; на ресивере 2.4G живёт в LED config (frame0 byte8, cloudglides)",
        ))
    }

    fn set_sleep_timer(&self, _seconds: u16) -> Result<()> {
        Err(Error::Unsupported(
            "sleep timer F75: команда не ревёрснута; на ресивере 2.4G — LED config (frame1 byte15 = minutes*2, cloudglides)",
        ))
    }

    fn get_battery_level(&self) -> Result<Option<u8>> {
        // Ресивер 2.4G (3554:FA09): отдельный канал output/input 0x13.
        // Проводной канал (0x06) заряда не отдаёт — только ресивер.
        if self.bridge.connection == ConnectionType::Wireless2_4G {
            return self.wireless_battery();
        }
        // USB/BT: канал заряда пока не найден (журнал 10.4), честно None.
        Ok(None)
    }

    // --- EEPROM ---------------------------------------------------------
    // РЕАЛЬНАЯ запись конфига в память клавиатуры (эксперимент 10.24/10.25).
    // Свет вкл/выкл управляется СЛОТАМИ эффект-таблицы: у каждого эффекта в
    // data[58..96] (шаг 2, пары (a,b)) лежит яркость a — 09 = полная, 00 =
    // выкл. Эмпирика 2026-09-13: занулять надо ВСЕ слоты, иначе при
    // переключении Fn+` свет возвращается (слот другого эффекта = 09).
    // Маркер 5A A5 на data[126..127] и служебные data[98..125] не трогаем
    // (затирание маркера = откат клавиатуры к заводскому дефолту).
    fn save_profile_to_eeprom(&self, _profile_id: u8) -> Result<()> {
        let snap = self.read_config()?;
        if snap.len() < 136 {
            return Err(Error::Protocol("снимок конфига неполный".into()));
        }
        let mut data = vec![0u8; 128];
        data.copy_from_slice(&snap[8..136]);
        for i in (58..98).step_by(2) {
            data[i] = 0x09; // свет включён полной яркостью для всех эффектов
        }
        self.write_config(&data)?;
        Ok(())
    }

    fn load_profile_from_eeprom(&self, _profile_id: u8) -> Result<()> {
        // Отдельного load-протокола нет: клавиатура сама загружает конфиг из
        // памяти при старте (после кика). Отдельная команда — не нужна (10.25).
        Err(Error::Unsupported("load-profile: конфиг читается из памяти при старте"))
    }

    // Аппаратный эффект прошивки (селектор data[10]): пишем код эффекта
    // и включаем свет полной яркостью во всех слотах эффект-таблицы — эффект
    // держится в памяти и применяется сам после перезапуска/переподключения,
    // без приложения (по Fn+\\ 2026-09-12; слои яркости — 2026-09-13).
    //
    // Для РЕСИВЕРА 2.4G селектор другой — led[10] (WL_EFFECTS): пишем код
    // эффекта через 0x13 сразу во флеш, свет включаем слотами [58..125].
    fn save_hardware_effect(&self, effect: u8) -> Result<()> {
        if self.bridge.connection == ConnectionType::Wireless2_4G {
            let mut led = self.wl_led_config()?;
            // Селектор эффекта ресивера — led[10]; остальные байты полем-в-поле.
            // Эмпирика 2026-09-15: запись ТОЛЬКО led[10] меняет эффект без кика;
            // затирание led[1]/led[3]/слотов [58..125] вызывало перезапуск клавиатуры.
            led[10] = effect;
            self.wl_write_pages(0x04, &led)?;
            return Ok(());
        }
        let snap = self.read_config()?;
        if snap.len() < 136 {
            return Err(Error::Protocol("снимок конфига неполный".into()));
        }
        let mut data = vec![0u8; 128];
        data.copy_from_slice(&snap[8..136]);
        data[10] = effect; // селектор активного эффекта (эмпирика: 02/03/04/05/06/07/08/0A/0B)
        for i in (58..98).step_by(2) {
            data[i] = 0x09; // свет включаем полной яркостью для всех эффектов
        }
        self.write_config(&data)?;
        Ok(())
    }

    // Сохранить эффект ВМЕСТЕ С яркостью и скоростью в память (GUI-кнопка
    // «Записать в память»): ресивер 2.4G — пишем led[10] + слот активного
    // эффекта [58+2*(led[10]-1)] (a=яркость, старшая тетрада b=скорость, цвет
    // в младшей тетраде сохраняем). USB — тот же селектор data[10] + слот
    // эффект-таблицы (журнал 10.28: слот яркости у активного эффекта).
    fn save_effect_with_style(&self, effect: u8, brightness: u8, speed: u8) -> Result<()> {
        if self.bridge.connection == ConnectionType::Wireless2_4G {
            let mut led = self.wl_led_config()?;
            led[10] = effect;
            let slot = 58 + 2 * ((led[10] as usize).saturating_sub(1));
            let b = (((brightness as u16) * 9) / 255).min(9) as u8;
            let s = (((speed as u16) * 4) / 255).min(4) as u8;
            led[slot] = b;
            led[slot + 1] = (s << 4) | (led[slot + 1] & 0x0F);
            self.last_wl_brightness.set(b);
            self.wl_write_pages(0x04, &led)?;
            return Ok(());
        }
        // USB: эффект + яркость/скорость активного слота эффект-таблицы.
        let snap = self.read_config()?;
        if snap.len() < 136 {
            return Err(Error::Protocol("снимок конфига неполный".into()));
        }
        let mut data = vec![0u8; 128];
        data.copy_from_slice(&snap[8..136]);
        data[10] = effect;
        let slot = 58 + 2 * ((effect as usize).saturating_sub(1));
        if slot + 1 < 128 {
            let b = (((brightness as u16) * 9) / 255).min(9) as u8;
            let s = (((speed as u16) * 4) / 255).min(4) as u8;
            data[slot] = b;
            data[slot + 1] = (s << 4) | (data[slot + 1] & 0x0F);
        }
        self.write_config(&data)?;
        Ok(())
    }

    // Состояние подсветки в памяти для ВСЕХ эффектов: яркость a в слотах
    // эффект-таблицы data[58..96] (шаг 2). 0x09 = вкл (полная яркость),
    // 0x00 = выкл при старте. Найдено эмпирически (журнал 2026-09-13):
    // «погашено Fn+Вниз» ↔ «яркость макс Fn+Вверх» у активного эффекта
    // меняет его слот; чтобы свет не возвращался при переключении эффектов
    // Fn+`, яркость зануляется/восстанавливается у ВСЕХ слотов.
    //
    // Для РЕСИВЕРА 2.4G — те же слота [58..125] в LED config (0x13),
    // запись во флеш через 0x04 (аналог wireless_apply_light, но по имени
    // кнопки «Записать в память»/автосейва).
    fn save_hardware_light(&self, on: bool) -> Result<()> {
        if self.bridge.connection == ConnectionType::Wireless2_4G {
            return self.wireless_apply_light(on);
        }
        let snap = self.read_config()?;
        if snap.len() < 136 {
            return Err(Error::Protocol("снимок конфига неполный".into()));
        }
        let mut data = vec![0u8; 128];
        data.copy_from_slice(&snap[8..136]);
        let level = if on { 0x09 } else { 0x00 };
        for i in (58..98).step_by(2) {
            data[i] = level;
        }
        self.write_config(&data)?;
        Ok(())
    }

    // --- Персистентные хуки (ресивер 2.4G) ----------------------------
    // Возвращают Ok(true), когда эффект/свет записан в память клавиатуры
    // (host-стрим тогда не нужен) и Ok(false) — для проводного F75
    // (там latch нет, работает host-рендеринг, см. 10.14/10.16).

    // Применить эффект/цвет/яркость/скорость НАВСЕГДА (в память ресивера).
    // Ресивер сразу пишет во флеш (dnviti) — эффект держится без приложения.
    fn apply_rgb_persistent(
        &self,
        mode: RgbMode,
        color: Option<Color>,
        brightness: u8,
        speed: u8,
    ) -> Result<bool> {
        match self.bridge.connection {
            ConnectionType::Wireless2_4G => {
                self.wireless_apply_effect(mode, color, brightness, speed)?;
                Ok(true)
            }
            // Проводной F75 аппаратных эффектов без latch не умеет — пусть
            // GUI/CLI гоняют host-рендеринг поверх движка.
            _ => Ok(false),
        }
    }

    // Выключить/включить свет НАВСЕГДА (яркость LED config ресивера).
    fn apply_light_persistent(&self, on: bool) -> Result<bool> {
        match self.bridge.connection {
            ConnectionType::Wireless2_4G => {
                self.wireless_apply_light(on)?;
                Ok(true)
            }
            // Проводной F75 host-гашение (см. 10.30) — возвращаем Ok(false).
            _ => Ok(false),
        }
    }
}

// ============================================================================
//  HOST-ДВИЖОК ЭФФЕКТОВ (host-rendering)
// ============================================================================
//  Аппаратные эффекты на F75 latch'а не имеют (журнал 10.14/10.16), поэтому
//  эффекты рендерим НА ХОСТЕ и шлём через подтверждённый render_frame()
//  direct-канал (06 08 … 7A 01, keepalive ~0.5–1с). Так эффекты/яркость/
//  скорость работают как в Windows-софте — на любом подключении (USB/2.4G/BT),
//  потому что host-рендеринг не зависит от аппаратной таблицы эффектов.
//
//  Это чистые функции: `mode` описано в types::RgbMode, кадр — Vec<Color>.
//  Никаких выдуманных cmd-id — только render_frame().

/// Параметры host-эффекта.
#[derive(Debug, Clone, Copy)]
pub struct HostEffectParams {
    /// Режим (из types::RgbMode).
    pub mode: RgbMode,
    /// Базовый цвет (для Static/Breathing/Custom).
    pub base: Color,
    /// Яркость 0..=255.
    pub brightness: u8,
    /// Скорость 1..=255.
    pub speed: u8,
}

impl Default for HostEffectParams {
    fn default() -> Self {
        Self {
            mode: RgbMode::Static,
            base: Color::new(255, 90, 0),
            brightness: 200,
            speed: 60,
        }
    }
}

/// Один кадр host-эффекта: `Vec<Color>` длиной RGB_LED_COUNT.
/// `t` — время в условных тиках (растёт со скоростью keepalive).
pub fn host_frame(p: &HostEffectParams, t: u32) -> Vec<Color> {
    host_frame_with(p, t, RGB_LED_COUNT, RGB_COLS)
}

/// То же, что `host_frame`, но с явными размерами матрицы (LED/колонки).
/// F99 (96%, 113 LED) использует эту версию с собственными константами;
/// host-движок одинаковый (тот же SinoWealth-чип, журнал 10.51).
pub fn host_frame_with(p: &HostEffectParams, t: u32, led_count: usize, cols: usize) -> Vec<Color> {
    let led = led_count;
    let mut out = Vec::with_capacity(led);
    let bright_k = p.brightness.max(1) as f32 / 255.0;

    // Фаза с нормализацией скорости (фикс 10.22): при speed=255 полный
    // цикл эффекта занимает ~1500 мс. Раньше формула t*speed/360*TAU при
    // 500-мс тике и speed>=57 давала прыжок фазы больше полного оборота —
    // эффект «телепортировался» и скорость визуально не воспринималась.
    let phase = p.speed as f32 / 255.0 * std::f32::consts::TAU * t as f32 / 1500.0;

    for i in 0..led {
        let c = match p.mode {
            RgbMode::Static => scale_c(p.base, bright_k),
            RgbMode::Breathing => {
                // Мягкое «дыхание»: синус яркости по времени.
                let k = 0.3 + 0.7 * (0.5 + 0.5 * phase.sin());
                scale_c(p.base, k * bright_k)
            }
            RgbMode::Wave => {
                // Волна hue по X-координате клавиши (i % cols).
                let col = (i % cols) as f32;
                let h = ((col / cols as f32) * 360.0 + phase.to_degrees()) % 360.0;
                hsl(h, 1.0, 0.5, bright_k)
            }
            RgbMode::SpectrumCycle => {
                // Спектр-цикл: hue одинаково движется для всех.
                let h = (phase.to_degrees() + (i as f32) * 2.0) % 360.0;
                hsl(h, 1.0, 0.5, bright_k)
            }
            RgbMode::Ripple => {
                // Кольцевая волна от центра матрицы.
                let cx = cols as f32 * 0.5;
                let cy = (led_count / cols) as f32 * 0.5;
                let x = (i % cols) as f32;
                let y = (i / cols) as f32;
                let ring = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
                let k = ((ring - phase).sin() * 0.5 + 0.5).powi(2);
                scale_c(p.base, k * bright_k)
            }
            RgbMode::Rain => {
                // «Дождь»: детерминированный псевдослучайный мерцания по фазе.
                let h = (i as u32).wrapping_mul(0x9E37).wrapping_add(t.wrapping_mul(p.speed as u32));
                let on = (h & 0x0FF) < 0x20;
                if on {
                    scale_c(p.base, (1.0 - (h as f32 * 0.001).fract() * 0.5) * bright_k)
                } else {
                    scale_c(p.base, 0.02)
                }
            }
            RgbMode::Reactive => {
                // «Отклик»: капля у индекса фазы, растекается вокруг.
                let cx = (phase * led_count as f32) as usize % led_count;
                let dist = (i as i32 - cx as i32).unsigned_abs() as f32;
                let k = (1.0 - dist / 12.0).max(0.0).powi(2);
                scale_c(p.base, k * bright_k)
            }
            RgbMode::Custom => scale_c(p.base, bright_k),
        };
        out.push(c);
    }
    out
}

// (конец host-движка)

// ============================================================================
//  HOST-ДВИЖОК ЭФФЕКТОВ (host-rendering, работает на железе).
// ============================================================================
//  Latch/commit на проводном F75 НЕ найдены (журнал 10.14/10.16), поэтому
//  живые эффекты делаем на хосте: считаем кадр по (mode, brightness, speed,
//  phase) и шлём через render_frame() — тот самый keepalive-канал direct.
//  Всё ниже — чистая математика кадров, HID не трогает.

/// Количество колонок в 75% раскладке (для host-эффектов волны).
pub const RGB_COLS: usize = 22; // примерное число колонок в 75% (без NumPad).

/// Масштабировать цвет с коэффициентом (для host-эффектов).
#[inline]
fn scale_c(c: Color, k: f32) -> Color {
    Color::new(
        ((c.r as f32) * k).clamp(0.0, 255.0) as u8,
        ((c.g as f32) * k).clamp(0.0, 255.0) as u8,
        ((c.b as f32) * k).clamp(0.0, 255.0) as u8,
    )
}

/// HSL → RGB (h в градусах, s,l в [0,1]).
pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let h = h.rem_euclid(360.0) / 360.0;
    let a = s * l.min(1.0 - l);
    let f = |n: f32| {
        let k = (n + h * 12.0) % 12.0;
        l - a * (k - 3.0).max(-1.0).min(9.0 - 3.0).clamp(-1.0, 1.0)
    };
    let r = (f(0.0) * 255.0).round().clamp(0.0, 255.0) as u8;
    let g = (f(8.0) * 255.0).round().clamp(0.0, 255.0) as u8;
    let b = (f(4.0) * 255.0).round().clamp(0.0, 255.0) as u8;
    (r, g, b)
}

/// HSL → RGB c учётом яркости.
#[inline]
fn hsl(h: f32, s: f32, l: f32, bright_k: f32) -> Color {
    let (r, g, b) = hsl_to_rgb(h, s, l);
    scale_c(Color::new(r, g, b), bright_k)
}
