// --- Ajazz AK820 Pro ---
// Реализация KeyboardDevice для Ajazz AK820 Pro (USB / 2.4G / BT).
//
// Уникальные особенности:
//   - TFT-экран 0.85" (разрешение ≈ 128×160, RGB565).
//   - Металлический энкодер (rotate / click).
//
// Протокол HID-based: feature-репорты с префиксом
//   REPORT_ID | COMMAND_ID | DATA...
// Основан на open-source фичур-коде klfc / ajazz-hid.

use super::hid::HidBridge;
use super::{Error, KeyboardDevice, Result};
use crate::types::*;

/// Внутреннее состояние: хранит HID-мост и карточку.
pub struct AjazzAk820 {
    bridge: HidBridge,
}

impl AjazzAk820 {
    /// Открыть устройство.
    pub fn open(bridge: HidBridge) -> Self {
        Self { bridge: bridge }
    }

    // --- Протокол Ajazz --------------------------------------------------
    // Report ID: 0x04  (feature-репорт команды)
    // Report ID: 0x05  (данные)
    // Report ID: 0x07  (FTFY)
    //
    // CMD_SET_RGB      = 0x01
    // CMD_SET_MODE     = 0x02
    // CMD_SET_BRIGHT   = 0x03
    // CMD_SET_SPEED    = 0x04
    // CMD_SCREEN_WRITE = 0x10
    // CMD_SCREEN_BRIGHT= 0x11
    // CMD_SYNC_TIME    = 0x12
    // CMD_ENCODER      = 0x20
    // CMD_POLLRATE     = 0x30
    // CMD_DEBOUNCE     = 0x31
    // CMD_SLEEP        = 0x32
    // CMD_KEYMAP       = 0x40
    // CMD_FN_LAYER     = 0x41
    // CMD_MACRO_RECORD = 0x50
    // CMD_MACRO_STOP   = 0x51
    // CMD_MACRO_SET    = 0x52
    // CMD_SAVE_PROFILE = 0xF0
    // CMD_LOAD_PROFILE = 0xF1

    fn cmd(&self, cmd_id: u8, payload: &[u8]) -> Result<()> {
        let mut buf = vec![0x04, cmd_id];
        buf.extend_from_slice(payload);
        self.bridge.send_feature(&buf)?;
        Ok(())
    }

    /// Формат: [R, G, B, keycode_lo, keycode_hi, index]
    fn format_rgb(key: KeyId, color: Color, idx: u8) -> [u8; 6] {
        [
            color.r,
            color.g,
            color.b,
            (key.0 & 0xFF) as u8,
            (key.0 >> 8) as u8,
            idx,
        ]
    }
}

impl KeyboardDevice for AjazzAk820 {
    fn name(&self) -> &str {
        match self.bridge.connection {
            ConnectionType::Usb => "Ajazz AK820 Pro (USB)",
            ConnectionType::Wireless2_4G => "Ajazz AK820 Pro (2.4 GHz)",
            ConnectionType::Bluetooth => "Ajazz AK820 Pro (Bluetooth)",
        }
    }

    fn connection_type(&self) -> ConnectionType {
        self.bridge.connection
    }

    fn is_connected(&self) -> bool {
        // Пробуем прочитать feature-репорт.
        self.bridge.read_feature(0x04, 8).is_ok()
    }

    fn vid_pid(&self) -> (u16, u16) {
        (self.bridge.vid, self.bridge.pid)
    }

    // ----------------------------------------------------------------
    // RGB
    // ----------------------------------------------------------------

    fn set_rgb_mode(&self, mode: RgbMode) -> Result<()> {
        let mode_id = match mode {
            RgbMode::Static => 0x00,
            RgbMode::Breathing => 0x01,
            RgbMode::Wave => 0x02,
            RgbMode::SpectrumCycle => 0x03,
            RgbMode::Ripple => 0x04,
            RgbMode::Rain => 0x05,
            RgbMode::Reactive => 0x06,
            RgbMode::Custom => 0x07,
        };
        self.cmd(0x02, &[mode_id])
    }

    fn set_rgb_color(&self, key: KeyId, color: Color) -> Result<()> {
        let data = Self::format_rgb(key, color, 0);
        self.cmd(0x01, &data)
    }

    fn set_rgb_brightness(&self, level: u8) -> Result<()> {
        self.cmd(0x03, &[level])
    }

    fn set_rgb_speed(&self, speed: u8) -> Result<()> {
        self.cmd(0x04, &[speed])
    }

    fn get_rgb_preset(&self) -> Result<Vec<RgbPreset>> {
        // Базовый набор встроенных пресетов.
        Ok(vec![
            RgbPreset {
                name: "Static".into(),
                mode: RgbMode::Static,
                colors: vec![Color::new(255, 255, 255)],
                brightness: 128,
                speed: 0,
            },
            RgbPreset {
                name: "Breathing".into(),
                mode: RgbMode::Breathing,
                colors: vec![Color::new(255, 0, 0)],
                brightness: 128,
                speed: 100,
            },
            RgbPreset {
                name: "Wave".into(),
                mode: RgbMode::Wave,
                colors: vec![
                    Color::new(255, 0, 0),
                    Color::new(0, 255, 0),
                    Color::new(0, 0, 255),
                ],
                brightness: 128,
                speed: 80,
            },
            RgbPreset {
                name: "Spectrum Cycle".into(),
                mode: RgbMode::SpectrumCycle,
                colors: vec![],
                brightness: 128,
                speed: 60,
            },
            RgbPreset {
                name: "Rain".into(),
                mode: RgbMode::Rain,
                colors: vec![Color::new(0, 150, 255)],
                brightness: 128,
                speed: 100,
            },
        ])
    }

    // ----------------------------------------------------------------
    // Keymap
    // ----------------------------------------------------------------

    fn remap_key(&self, from: KeyId, to: KeyAction) -> Result<()> {
        let keycode_out = match &to {
            KeyAction::Press(k) => *k,
            KeyAction::Media(k) => *k | 0x1000,
            KeyAction::Modifier(m) => *m as u16,
            KeyAction::FnLayer(l) => *l as u16 | 0x2000,
            KeyAction::Macro(m) => *m as u16 | 0x3000,
        };
        let data = [
            (from.0 & 0xFF) as u8,
            (from.0 >> 8) as u8,
            (keycode_out & 0xFF) as u8,
            (keycode_out >> 8) as u8,
        ];
        self.cmd(0x40, &data)
    }

    fn set_fn_layer(&self, layer: FnLayer) -> Result<()> {
        for (key, action) in &layer.bindings {
            self.remap_key(*key, action.clone())?;
        }
        // Применяем слой
        self.cmd(0x41, &[0x01])
    }

    fn get_current_keymap(&self) -> Result<Keymap> {
        // Запрашиваем 4 репорта по 64 байта.
        let mut data = Vec::new();
        for offset in 0u8..4 {
            self.cmd(0x40, &[0x00, offset])?;
            std::thread::sleep(std::time::Duration::from_millis(20));
            let report = self.bridge.read_feature(0x05, 64)?;
            data.extend_from_slice(&report[2..]); // пропускаем ID + cmd echo
        }
        Ok(Keymap {
            base: Vec::new(), // Парсинг зависит от железа — оставляем заглушку.
            fn_layer: FnLayer::default(),
        })
    }

    // ----------------------------------------------------------------
    // Screen (уникально для Ajazz)
    // ----------------------------------------------------------------

    fn set_screen_image(&self, frame: ScreenFrame) -> Result<()> {
        // Формат: 128×160 пикселей, RGB565, big-endian.
        // Пакуем пакетами по 60 байт в feature-репорт.
        const CHUNK_SIZE: usize = 56; // 60 - 4 заголовочных
        const WIDTH: u16 = 128;
        const HEIGHT: u16 = 160;
        let total_pixels = WIDTH as usize * HEIGHT as usize;
        let expected = total_pixels * 2;
        if frame.data_rgb565.len() != expected {
            return Err(Error::Protocol(format!(
                "ожидается {} байт, получено {}",
                expected,
                frame.data_rgb565.len()
            )));
        }
        // Заголовок: 4 байта [0x00, 0x10, W_hi, W_lo]
        self.cmd(0x10, &[(WIDTH >> 8) as u8, (WIDTH & 0xFF) as u8])?;
        std::thread::sleep(std::time::Duration::from_millis(100));
        let mut offset = 0usize;
        let mut seq: u8 = 0;
        while offset < frame.data_rgb565.len() {
            let end = (offset + CHUNK_SIZE).min(frame.data_rgb565.len());
            let mut packet = vec![0x05, 0x10, seq];
            packet.extend_from_slice(&frame.data_rgb565[offset..end]);
            self.bridge.send_feature(&packet)?;
            seq = seq.wrapping_add(1);
            offset = end;
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        // Финальный маркер
        self.cmd(0x10, &[0xFF])?;
        Ok(())
    }

    fn set_screen_brightness(&self, level: u8) -> Result<()> {
        self.cmd(0x11, &[level])
    }

    fn sync_system_time(&self) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| Error::Protocol(e.to_string()))?;
        let secs = now.as_secs();
        let data = [
            ((secs >> 24) & 0xFF) as u8,
            ((secs >> 16) & 0xFF) as u8,
            ((secs >> 8) & 0xFF) as u8,
            (secs & 0xFF) as u8,
        ];
        self.cmd(0x12, &data)
    }

    // ----------------------------------------------------------------
    // Encoder
    // ----------------------------------------------------------------

    fn set_encoder_action(&self, action: EncoderAction) -> Result<()> {
        let action_id = match action {
            EncoderAction::Volume => 0x00,
            EncoderAction::BrightnessUp => 0x01,
            EncoderAction::BrightnessDown => 0x02,
            EncoderAction::ScrollUp => 0x03,
            EncoderAction::ScrollDown => 0x04,
            EncoderAction::MediaNext => 0x05,
            EncoderAction::MediaPrev => 0x06,
            EncoderAction::PlayPause => 0x07,
            EncoderAction::Mute => 0x08,
            EncoderAction::None => 0xFF,
        };
        self.cmd(0x20, &[action_id])
    }

    // ----------------------------------------------------------------
    // Macros
    // ----------------------------------------------------------------

    fn record_macro(&self, macro_id: u8) -> Result<()> {
        self.cmd(0x50, &[macro_id])
    }

    fn stop_macro(&self) -> Result<()> {
        self.cmd(0x51, &[])
    }

    fn set_macro(&self, id: u8, actions: Vec<KeyAction>) -> Result<()> {
        let mut payload = vec![id, actions.len() as u8];
        for action in &actions {
            let keycode = match action {
                KeyAction::Press(k) => *k,
                KeyAction::Media(k) => *k | 0x1000,
                KeyAction::Modifier(m) => *m as u16,
                KeyAction::FnLayer(l) => *l as u16 | 0x2000,
                KeyAction::Macro(m) => *m as u16 | 0x3000,
            };
            payload.push((keycode & 0xFF) as u8);
            payload.push((keycode >> 8) as u8);
        }
        self.cmd(0x52, &payload)
    }

    // ----------------------------------------------------------------
    // System
    // ----------------------------------------------------------------

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
        self.cmd(0x30, &[0x00])?;
        std::thread::sleep(std::time::Duration::from_millis(30));
        let report = self.bridge.read_feature(0x04, 8)?;
        // Байт [5] — уровень заряда 0..100; 0xFF = не поддерживается.
        if report.len() > 5 && report[5] != 0xFF {
            Ok(Some(report[5]))
        } else {
            Ok(None)
        }
    }

    // ----------------------------------------------------------------
    // EEPROM
    // ----------------------------------------------------------------

    fn save_profile_to_eeprom(&self, profile_id: u8) -> Result<()> {
        self.cmd(0xF0, &[profile_id])
    }

    fn load_profile_from_eeprom(&self, profile_id: u8) -> Result<()> {
        self.cmd(0xF1, &[profile_id])
    }
}