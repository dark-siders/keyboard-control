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
//   ВАЖНО: методы ниже (set_rgb_mode/set_rgb_color/remap_key/polling rate и т.д.)
//   НАПИСАНЫ С ВЫДУМАННЫМИ cmd-id — на железе не проверены. Рабочие:
//   render_frame() и чтение конфига. Остальное заполняем по мере реверса.

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

    /// Название режима подсветки по индексу из эффект-таблицы (приблизительно).
    fn effect_name(_idx: u8) -> &'static str {
        // Полного маппинга эффектов пока нет; уточнять по дампу прошивки.
        "Эффект"
    }

    // --- Протокол AULA F75 ---
    // НА ЗАМЕТКУ: cmd() и всё через него — выдуманные ID, пока не проверено.
    fn cmd(&self, cmd_id: u8, payload: &[u8]) -> Result<()> {
        let mut buf = vec![cmd_id];
        buf.extend_from_slice(payload);
        self.bridge.send_feature(&buf)?;
        Ok(())
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
        // UI-масштаб (0..=255) → шкала прошивки (0..=4).
        let b = (((brightness as u16) * 4) / 255).min(4) as u8;
        let s = (((speed as u16) * 4) / 255).min(4) as u8;
        led[3] = b;
        led[4] = s;
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

    // --- RGB --------------------------------------------------------
    // Реальные команды: render_frame() (direct, подтверждён) + чтение конфига.
    // set_rgb_mode/color/brightness/speed — ВЫДУМАННЫЕ cmd-id, до реверса.
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

    // ВЫМЫСЕЛ (не проверено): set_rgb_mode
    fn set_rgb_mode(&self, mode: RgbMode) -> Result<()> {
        let mode_id = match mode {
            RgbMode::Static => 0x01,
            RgbMode::Breathing => 0x02,
            RgbMode::Wave => 0x03,
            RgbMode::SpectrumCycle => 0x04,
            RgbMode::Ripple => 0x05,
            RgbMode::Rain => 0x06,
            RgbMode::Reactive => 0x07,
            RgbMode::Custom => 0x08,
        };
        self.cmd(0x02, &[mode_id])
    }

    // ВЫМЫСЕЛ (не проверено): set_rgb_color
    fn set_rgb_color(&self, key: KeyId, color: Color) -> Result<()> {
        // [0x03, key_lo, key_hi, r, g, b]
        self.cmd(
            0x03,
            &[
                (key.0 & 0xFF) as u8,
                (key.0 >> 8) as u8,
                color.r,
                color.g,
                color.b,
            ],
        )
    }

    // ВЫМЫСЕЛ (не проверено): set_rgb_brightness
    fn set_rgb_brightness(&self, level: u8) -> Result<()> {
        self.cmd(0x04, &[level])
    }

    // ВЫМЫСЕЛ (не проверено): set_rgb_speed
    fn set_rgb_speed(&self, speed: u8) -> Result<()> {
        self.cmd(0x05, &[speed])
    }

    /// Прочитать текущий эффект из конфига (реально, через read_config).
    fn get_rgb_preset(&self) -> Result<Vec<RgbPreset>> {
        let data = self.read_config()?;
        // data[0] = report id, [1..8] — эхо-заголовок, [8..] — 128 байт config + 5A A5
        let cfg = &data[8..data.len().min(8 + 128)];
        // Эффект-таблица: пары (a,b) с offset 58; берём первую пару как текущую.
        let a = cfg.get(58).copied().unwrap_or(0x09); // яркость
        let b = cfg.get(59).copied().unwrap_or(0); // (speed << 4) | color
        Ok(vec![RgbPreset {
            name: format!("{} (из прошивки)", Self::effect_name(b & 0x0F)),
            mode: RgbMode::Custom,
            colors: vec![],
            brightness: a,
            speed: b >> 4,
        }])
    }

    // --- Keymap ------------------------------------------------------
    fn remap_key(&self, from: KeyId, to: KeyAction) -> Result<()> {
        let keycode_out = match &to {
            KeyAction::Press(k) => *k,
            KeyAction::Media(k) => *k | 0x40_00, // старший байт маркирует consumer
            KeyAction::Modifier(m) => *m as u16,
            KeyAction::FnLayer(l) => *l as u16 | 0x80_00,
            KeyAction::Macro(m) => *m as u16 | 0xC0_00,
        };
        self.cmd(
            0x10,
            &[
                (from.0 & 0xFF) as u8,
                (from.0 >> 8) as u8,
                (keycode_out & 0xFF) as u8,
                (keycode_out >> 8) as u8,
            ],
        )
    }

    fn set_fn_layer(&self, layer: FnLayer) -> Result<()> {
        for (key, action) in &layer.bindings {
            self.remap_key(*key, action.clone())?;
        }
        self.cmd(0x11, &[0x01])
    }

    fn get_current_keymap(&self) -> Result<Keymap> {
        Ok(Keymap {
            base: Vec::new(),
            fn_layer: FnLayer::default(),
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
        // F75 — ролик только громкость.
        let action_id = match action {
            EncoderAction::Volume | EncoderAction::Mute => 0x01,
            _ => return Err(Error::Unsupported("ролик F75 поддерживает только громкость")),
        };
        self.cmd(0x20, &[action_id])
    }

    // --- Macros ---------------------------------------------------------
    fn record_macro(&self, _macro_id: u8) -> Result<()> {
        Err(Error::Unsupported("аппаратные макросы не поддерживаются AULA F75"))
    }

    fn stop_macro(&self) -> Result<()> {
        Ok(())
    }

    fn set_macro(&self, _id: u8, _actions: Vec<KeyAction>) -> Result<()> {
        Err(Error::Unsupported("аппаратные макросы не поддерживаются AULA F75"))
    }

    // --- System ---------------------------------------------------------
    fn set_polling_rate(&self, rate: u16) -> Result<()> {
        let rate_id = match rate {
            125 => 0x01,
            250 => 0x02,
            500 => 0x03,
            1000 => 0x04,
            _ => return Err(Error::Protocol("неподдерживаемый polling rate".into())),
        };
        self.cmd(0x30, &[rate_id])
    }

    fn set_debounce(&self, ms: u16) -> Result<()> {
        self.cmd(0x31, &[(ms & 0xFF) as u8, (ms >> 8) as u8])
    }

    fn set_sleep_timer(&self, seconds: u16) -> Result<()> {
        self.cmd(0x32, &[(seconds & 0xFF) as u8, (seconds >> 8) as u8])
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
///
/// Эффекты реализованы как host-аналоги Windows-пресетов:
///   Static       — все LED = base (с учётом brightness).
///   Breathing    — синусоидальная модуляция яркости по времени.
///   Wave         — бегущая волна: hue смещается по матрице по X.
///   SpectrumCycle— «радуга»: hue бежит циклически по всем LED.
///   Ripple       — расширяющееся кольцо от центра раскладки.
///   Rain         — «дождь»: случайные LED вспыхивают и гаснут.
///   Reactive     — «отклик»: капля, растекающаяся по матрице.
pub fn host_frame(p: &HostEffectParams, t: u32) -> Vec<Color> {
    let led = RGB_LED_COUNT;
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
                // Волна hue по X-координате клавиши (i % COLS).
                let col = (i % RGB_COLS) as f32;
                let h = ((col / RGB_COLS as f32) * 360.0 + phase.to_degrees()) % 360.0;
                hsl(h, 1.0, 0.5, bright_k)
            }
            RgbMode::SpectrumCycle => {
                // Спектр-цикл: hue одинаково движется для всех.
                let h = (phase.to_degrees() + (i as f32) * 2.0) % 360.0;
                hsl(h, 1.0, 0.5, bright_k)
            }
            RgbMode::Ripple => {
                // Кольцевая волна от центра 75% матрицы.
                let cx = RGB_COLS as f32 * 0.5;
                let cy = (RGB_LED_COUNT / RGB_COLS) as f32 * 0.5;
                let x = (i % RGB_COLS) as f32;
                let y = (i / RGB_COLS) as f32;
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
                let cx = (phase * RGB_LED_COUNT as f32) as usize % RGB_LED_COUNT;
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
