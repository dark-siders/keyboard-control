// --- AULA F99 ---
// Реализация KeyboardDevice для AULA F99 (USB / 2.4G / BT).
//
// Особенности:
//   - 96% раскладка с NumPad.
//   - Верхний ролик громкости/подсветки.
//   - Управляемые индикаторы (Caps / Num / WinLock).
//
// Протокол близок к F75, но отличается длиной матрицы (вид драйвера)
// и наличием команд индикаторов.

use super::hid::HidBridge;
use super::{Error, KeyboardDevice, Result};
use crate::types::*;

/// Количество клавиш в 96% раскладке F99.
pub const KEY_COUNT: u16 = 100;

pub struct AulaF99 {
    bridge: HidBridge,
}

impl AulaF99 {
    pub fn open(bridge: HidBridge) -> Self {
        Self { bridge: bridge }
    }

    fn cmd(&self, cmd_id: u8, payload: &[u8]) -> Result<()> {
        let mut buf = vec![cmd_id];
        buf.extend_from_slice(payload);
        self.bridge.send_feature(&buf)?;
        Ok(())
    }
}

/// Индикаторы F99. Значение — какой светодиод менять.
#[derive(Debug, Clone, Copy)]
pub enum Indicator {
    Caps,
    Num,
    WinLock,
}

impl Indicator {
    fn index(&self) -> u8 {
        match self {
            Indicator::Caps => 0x01,
            Indicator::Num => 0x02,
            Indicator::WinLock => 0x03,
        }
    }
}

impl KeyboardDevice for AulaF99 {
    fn name(&self) -> &str {
        match self.bridge.connection {
            ConnectionType::Usb => "AULA F99 (USB)",
            ConnectionType::Wireless2_4G => "AULA F99 (2.4 GHz)",
            ConnectionType::Bluetooth => "AULA F99 (Bluetooth)",
        }
    }

    fn connection_type(&self) -> ConnectionType {
        self.bridge.connection
    }

    fn is_connected(&self) -> bool {
        self.bridge.read_feature(0x00, 8).is_ok()
    }

    fn vid_pid(&self) -> (u16, u16) {
        (self.bridge.vid, self.bridge.pid)
    }

    // --- RGB -----------------------------------------------------------
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

    fn set_rgb_color(&self, key: KeyId, color: Color) -> Result<()> {
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

    fn set_rgb_brightness(&self, level: u8) -> Result<()> {
        self.cmd(0x04, &[level])
    }

    fn set_rgb_speed(&self, speed: u8) -> Result<()> {
        self.cmd(0x05, &[speed])
    }

    fn get_rgb_preset(&self) -> Result<Vec<RgbPreset>> {
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
                colors: vec![Color::new(255, 120, 0)],
                brightness: 128,
                speed: 120,
            },
            RgbPreset {
                name: "Wave".into(),
                mode: RgbMode::Wave,
                colors: vec![
                    Color::new(255, 0, 0),
                    Color::new(255, 165, 0),
                    Color::new(0, 255, 0),
                    Color::new(0, 191, 255),
                    Color::new(148, 0, 211),
                ],
                brightness: 128,
                speed: 90,
            },
            RgbPreset {
                name: "Spectrum Cycle".into(),
                mode: RgbMode::SpectrumCycle,
                colors: vec![],
                brightness: 128,
                speed: 70,
            },
            RgbPreset {
                name: "Rain".into(),
                mode: RgbMode::Rain,
                colors: vec![Color::new(0, 200, 255)],
                brightness: 128,
                speed: 100,
            },
        ])
    }

    // --- Keymap --------------------------------------------------------
    fn remap_key(&self, from: KeyId, to: KeyAction) -> Result<()> {
        let keycode_out = match &to {
            KeyAction::Press(k) => *k,
            KeyAction::Media(k) => *k | 0x40_00,
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

    // --- Screen (не поддерживается) ------------------------------------
    fn set_screen_image(&self, _frame: ScreenFrame) -> Result<()> {
        Err(Error::Unsupported("TFT-экран не поддерживается AULA F99"))
    }

    fn set_screen_brightness(&self, _level: u8) -> Result<()> {
        Err(Error::Unsupported("TFT-экран не поддерживается AULA F99"))
    }

    fn sync_system_time(&self) -> Result<()> {
        Err(Error::Unsupported("синхронизация времени не поддерживается AULA F99"))
    }

    // --- Encoder -------------------------------------------------------
    fn set_encoder_action(&self, action: EncoderAction) -> Result<()> {
        // F99 — верхний ролик громкости/подсветки.
        let action_id = match action {
            EncoderAction::Volume => 0x01,
            EncoderAction::Mute => 0x02,
            EncoderAction::BrightnessUp => 0x03,
            EncoderAction::BrightnessDown => 0x04,
            _ => return Err(Error::Unsupported("ролик F99 поддерживает громкость/подсветку")),
        };
        self.cmd(0x20, &[action_id])
    }

    // --- Macros --------------------------------------------------------
    fn record_macro(&self, _macro_id: u8) -> Result<()> {
        Err(Error::Unsupported("аппаратные макросы не поддерживаются AULA F99"))
    }

    fn stop_macro(&self) -> Result<()> {
        Ok(())
    }

    fn set_macro(&self, _id: u8, _actions: Vec<KeyAction>) -> Result<()> {
        Err(Error::Unsupported("аппаратные макросы не поддерживаются AULA F99"))
    }

    // --- System --------------------------------------------------------
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
        if report.len() > 3 {
            let raw = u32::from(report[1]) | (u32::from(report[2]) << 8) | (u32::from(report[3]) << 16);
            if raw <= 100 {
                return Ok(Some(raw as u8));
            }
        }
        Ok(None)
    }

    // --- EEPROM --------------------------------------------------------
    fn save_profile_to_eeprom(&self, profile_id: u8) -> Result<()> {
        self.cmd(0xF0, &[profile_id])
    }

    fn load_profile_from_eeprom(&self, profile_id: u8) -> Result<()> {
        self.cmd(0xF1, &[profile_id])
    }
}

/// Управление индикаторами (специфично для F99).
/// Реализовано отдельной функцией, чтобы не раздувать trait.
pub fn set_indicator(kbd: &AulaF99, indicator: Indicator, enabled: bool) -> Result<()> {
    let state = if enabled { 0x01 } else { 0x00 };
    kbd.cmd(0x21, &[indicator.index(), state])
}