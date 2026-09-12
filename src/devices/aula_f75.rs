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
];

/// Найти имя аппаратного эффекта по коду (для отладки/GUI).
pub fn hw_effect_name(code: u8) -> Option<&'static str> {
    HW_EFFECTS.iter().find(|(c, _)| *c == code).map(|(_, n)| *n)
}

/// Номер Feature-репорта главного канала.
const REPORT_FEATURE: u8 = 0x06;
/// Длина пакета (без report id).
const PACKET_LEN: usize = 520;

pub struct AulaF75 {
    bridge: HidBridge,
}

impl AulaF75 {
    pub fn open(bridge: HidBridge) -> Self {
        Self { bridge: bridge }
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
        self.cmd(0x33, &[0x00])?;
        std::thread::sleep(std::time::Duration::from_millis(30));
        let report = self.bridge.read_feature(0x0F, 8)?;
        // Байт [1]..[3] — заряд; если 0xFFFFFF или превышает 100 — недоступно.
        if report.len() > 3 {
            let raw = u32::from(report[1]) | (u32::from(report[2]) << 8) | (u32::from(report[3]) << 16);
            if raw <= 100 {
                return Ok(Some(raw as u8));
            }
        }
        Ok(None)
    }

    // --- EEPROM ---------------------------------------------------------
    // РЕАЛЬНАЯ запись конфига в память клавиатуры (эксперимент 10.24/10.25):
    // снимаем снимок конфига, включаем подсветку (data[76]=09 — свет вкл по
    // умолчанию, байт яркости найден в 10.28), пишем обратно через
    // write_config(). Кик/перезапуск клавы и есть commit: после него подсветка
    // держится БЕЗ запущенного приложения.
    fn save_profile_to_eeprom(&self, _profile_id: u8) -> Result<()> {
        let snap = self.read_config()?;
        if snap.len() < 136 {
            return Err(Error::Protocol("снимок конфига неполный".into()));
        }
        let mut data = vec![0u8; 128];
        data.copy_from_slice(&snap[8..136]);
        // data[76]: яркость активного эффекта (эмпирика 2026-09-12: 00 = свет
        // выключен, 09 = полная яркость; единственный отличающийся байт между
        // снимками «погашено Fn+Вниз» и «яркость макс. Fn+Вверх»).
        data[76] = 0x09; // свет включён по умолчанию
        self.write_config(&data)?;
        Ok(())
    }

    fn load_profile_from_eeprom(&self, _profile_id: u8) -> Result<()> {
        // Отдельного load-протокола нет: клавиатура сама загружает конфиг из
        // памяти при старте (после кика). Отдельная команда — не нужна (10.25).
        Err(Error::Unsupported("load-profile: конфиг читается из памяти при старте"))
    }

    // Аппаратный эффект прошивки (селектор data[10]): пишем код эффекта
    // и включаем свет — эффект держится в памяти и применяется сам после
    // перезапуска/переподключения, без приложения (по Fn+\\ 2026-09-12).
    fn save_hardware_effect(&self, effect: u8) -> Result<()> {
        let snap = self.read_config()?;
        if snap.len() < 136 {
            return Err(Error::Protocol("снимок конфига неполный".into()));
        }
        let mut data = vec![0u8; 128];
        data.copy_from_slice(&snap[8..136]);
        data[10] = effect; // селектор активного эффекта (эмпирика: 02/03/04/05/06/07/08/0A/0B)
        data[76] = 0x09;   // свет включаем полной яркостью (байт яркости, 10.28)
        self.write_config(&data)?;
        Ok(())
    }

    // Состояние подсветки в памяти (data[76]): 0x09 = вкл (полная яркость),
    // 0x00 = выкл при старте. Найден эмпирически (журнал 10.28): «погашено
    // Fn+Вниз» ↔ «яркость макс Fn+Вверх» отличаются ТОЛЬКО этим байтом.
    fn save_hardware_light(&self, on: bool) -> Result<()> {
        let snap = self.read_config()?;
        if snap.len() < 136 {
            return Err(Error::Protocol("снимок конфига неполный".into()));
        }
        let mut data = vec![0u8; 128];
        data.copy_from_slice(&snap[8..136]);
        data[76] = if on { 0x09 } else { 0x00 };
        self.write_config(&data)?;
        Ok(())
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
