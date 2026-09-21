// --- Общие типы данных Keyboard-Center ---
// Цвета, клавиши, макросы, режимы подсветки — всё, что
// используется сразу несколькими подсистемами.

use serde::{Deserialize, Serialize};

/// Стандартный RGB-цвет (по 8 бит на канал).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Цвет в виде 32-битного ARGB (для GTK / Image).
    pub fn to_argb(&self) -> u32 {
        (0xFFu32 << 24) | ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    /// Парсинг из hex-строки вида `#RRGGBB`.
    pub fn from_hex_checked(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Self::new(r, g, b))
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::new(255, 255, 255)
    }
}

/// Идентификатор физической клавиши в макете.
/// Обычно совпадает с «скэнкодом» (keycode) клавиатуры.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct KeyId(pub u16);

/// Действие, на которое можно переназначить клавишу.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum KeyAction {
    /// Обычная клавиша (скэнкод из USB HID usage table).
    Press(u16),
    /// Медиа-клавиша (HID consumer usage).
    Media(u16),
    /// Модификатор (Ctrl/Shift/Alt/Win).
    Modifier(u8),
    /// Слой Fn (переключение слоя).
    FnLayer(u8),
    /// Макрос по идентификатору.
    Macro(u8),
}

/// Режим RGB-подсветки (пресет).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RgbMode {
    Static,
    Breathing,
    Wave,
    SpectrumCycle,
    Ripple,
    Rain,
    Reactive,
    Custom,
}

/// Одно событие макроса (wsclx macros::action, формат флеша).
/// Три поля: `delay_ms` — задержка ПЕРЕД действием, `key_code` — HID-код,
/// `family` — тип события: 3 = клавиатура (флаги 0xB0/0x30), 1 = мышь
/// (0x90/0x10), 2 = consumer/text (те же wire-флаги, что у мыши).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroEvent {
    /// Задержка в мс перед следующим действием (LE u16 в флеше).
    pub delay_ms: u16,
    /// HID-код клавиши (интерпретация зависит от `family`).
    pub key_code: u8,
    /// Тип события (actionType): 3=keyboard, 1=mouse, 2=consumer/text.
    pub family: u8,
    /// true = нажатие, false = отпускание (bit 7 wire-флагов).
    pub is_press: bool,
}

/// Прочитанный макрос целиком (id + все события).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Macro {
    pub id: u8,
    pub events: Vec<MacroEvent>,
}

/// Готовый пресет RGB (набор цветов + параметры).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RgbPreset {
    pub name: String,
    pub mode: RgbMode,
    pub colors: Vec<Color>,
    pub brightness: u8,
    pub speed: u8,
}

/// Тип физического подключения устройства.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionType {
    Usb,
    Wireless2_4G,
    Bluetooth,
}

impl ConnectionType {
    pub fn label(&self) -> &'static str {
        match self {
            ConnectionType::Usb => "USB",
            ConnectionType::Wireless2_4G => "2.4 GHz",
            ConnectionType::Bluetooth => "Bluetooth",
        }
    }
}

/// Кадр для TFT-экрана Ajazz (RGB565).
/// `width`/`height` — реальное разрешение экрана.
#[derive(Debug, Clone)]
pub struct ScreenFrame {
    pub width: u16,
    pub height: u16,
    /// Пиксели в RGB565 (big-endian), длина = width * height * 2.
    pub data_rgb565: Vec<u8>,
}

/// Слой Fn — набор переназначений для верхнего слоя.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FnLayer {
    pub bindings: Vec<(KeyId, KeyAction)>,
}

/// Текущая карта клавиш (основной слой + Fn-слой).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Keymap {
    pub base: Vec<(KeyId, KeyAction)>,
    pub fn_layer: FnLayer,
}

/// Действие энкодера/ролика (поворот и клик).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncoderAction {
    Volume,
    BrightnessUp,
    BrightnessDown,
    ScrollUp,
    ScrollDown,
    MediaNext,
    MediaPrev,
    PlayPause,
    Mute,
    None,
}

/// Строка макрос-записи: задержка + действие.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroStep {
    pub delay_ms: u16,
    pub action: KeyAction,
}