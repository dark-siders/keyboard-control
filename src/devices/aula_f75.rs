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
    fn save_profile_to_eeprom(&self, profile_id: u8) -> Result<()> {
        self.cmd(0xF0, &[profile_id])
    }

    fn load_profile_from_eeprom(&self, profile_id: u8) -> Result<()> {
        self.cmd(0xF1, &[profile_id])
    }
}