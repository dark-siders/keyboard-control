// --- Абстракция устройств Keyboard-Center ---
// trait KeyboardDevice + реестр поддерживаемых моделей.

use crate::types::{
    Color, EncoderAction, FnLayer, KeyAction, KeyId, Keymap, RgbPreset, ScreenFrame,
};

// Типы, доступные наружу через `crate::devices`.
pub use crate::types::{ConnectionType, RgbMode};

pub mod ajazz_ak820;
pub mod aula_f75;
pub mod aula_f99;
pub mod hid;
pub mod hidshell;
pub mod scanner;

/// Тип ошибки устройств.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("устройство не подключено")]
    NotConnected,
    #[error("устройство не поддерживает эту функцию")]
    Unsupported(&'static str),
    #[error("ошибка HID: {0}")]
    Hid(#[from] hidapi::HidError),
    #[error("ошибка ввода/вывода: {0}")]
    Io(#[from] std::io::Error),
    #[error("некорректный протокол: {0}")]
    Protocol(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Результат обобщённого реестра: сама клавиатура + её «личная карточка».
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Имя модели.
    pub model: &'static str,
    /// VID устройства.
    pub vid: u16,
    /// PID устройства.
    pub pid: u16,
    /// Тип подключения (определяется по PID).
    pub connection: ConnectionType,
    /// Раскладка: 75% или 96%.
    pub layout: Layout,
    /// Поддерживает ли TFT-экран.
    pub has_screen: bool,
    /// Поддерживает ли энкодер/ролик.
    pub has_encoder: bool,
    /// Имеет ли индикаторы Cap/Num/WinLock (управляемые).
    pub has_indicators: bool,
}

/// Физическая раскладка клавиатуры.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Меньше процентов = меньше клавиш.
    L75,
    /// 96% — с NumPad.
    L96,
}

/// Основной trait. Каждая модель (Ajazz/AULA) реализует эти методы.
/// Методы, которые конкретное железо не поддерживает,
/// должны возвращать `Err(Error::Unsupported(...))`.
pub trait KeyboardDevice: Send {
    /// Человекочитаемое имя устройства, например «AULA F99 (USB)».
    fn name(&self) -> &str;
    /// Тип подключения.
    fn connection_type(&self) -> ConnectionType;
    /// Подключено ли устройство в данный момент.
    fn is_connected(&self) -> bool;
    /// VID/PID устройства (для отладки и переподключения).
    fn vid_pid(&self) -> (u16, u16);

    // --- RGB -----------------------------------------------------------
    /// Установить режим подсветки.
    fn set_rgb_mode(&self, mode: RgbMode) -> Result<()>;
    /// Установить цвет конкретной клавиши (для режима Custom).
    fn set_rgb_color(&self, key: KeyId, color: Color) -> Result<()>;
    /// Установить яркость (0..=255).
    fn set_rgb_brightness(&self, level: u8) -> Result<()>;
    /// Установить скорость анимации (0..=255).
    fn set_rgb_speed(&self, speed: u8) -> Result<()>;
    /// Отправить полный кадр подсветки (host-rendering, direct-режим).
    /// `colors` перечисляет все LED в порядке макета; недостающие — выключены.
    fn render_frame(&self, _colors: &[Color]) -> Result<()> {
        Err(Error::Unsupported(
            "хост-рендеринг (render_frame) не поддерживается этим устройством",
        ))
    }
    /// Получить список встроенных пресетов.
    fn get_rgb_preset(&self) -> Result<Vec<RgbPreset>>;

    // --- Keymap / Fn -----------------------------------------------------
    /// Переназначить клавишу.
    fn remap_key(&self, from: KeyId, to: KeyAction) -> Result<()>;
    /// Записать весь Fn-слой.
    fn set_fn_layer(&self, layer: FnLayer) -> Result<()>;
    /// Прочитать текущую карту клавиш с устройства.
    fn get_current_keymap(&self) -> Result<Keymap>;

    // --- Screen (только Ajazz AK820 Pro) ------------------------------
    /// Загрузить кадр на TFT-экран.
    fn set_screen_image(&self, frame: ScreenFrame) -> Result<()>;
    /// Установить яркость экрана (0..=255).
    fn set_screen_brightness(&self, level: u8) -> Result<()>;
    /// Синхронизировать системное время ПК с экраном клавиатуры.
    fn sync_system_time(&self) -> Result<()>;

    // --- Encoder / Knob --------------------------------------------------
    /// Настроить поведение поворота энкодера.
    fn set_encoder_action(&self, action: EncoderAction) -> Result<()>;

    // --- Macros ----------------------------------------------------------
    /// Начать запись макроса (device-side).
    fn record_macro(&self, macro_id: u8) -> Result<()>;
    /// Остановить запись.
    fn stop_macro(&self) -> Result<()>;
    /// Прошивка готовой последовательности действий.
    fn set_macro(&self, id: u8, actions: Vec<KeyAction>) -> Result<()>;

    // --- System ----------------------------------------------------------
    /// Частота опроса: 125 / 250 / 500 / 1000 Гц.
    fn set_polling_rate(&self, rate: u16) -> Result<()>;
    /// Задержка анти-дребезга в миллисекундах.
    fn set_debounce(&self, ms: u16) -> Result<()>;
    /// Таймер автоотключения в секундах (0 = выкл).
    fn set_sleep_timer(&self, seconds: u16) -> Result<()>;
    /// Уровень заряда батареи 0..=100 (None если недоступно).
    fn get_battery_level(&self) -> Result<Option<u8>>;

    // --- EEPROM ----------------------------------------------------------
    /// Сохранить текущий профиль в слот памяти устройства.
    fn save_profile_to_eeprom(&self, profile_id: u8) -> Result<()>;
    /// Загрузить профиль из слота памяти устройства.
    fn load_profile_from_eeprom(&self, profile_id: u8) -> Result<()>;
}

/// Карточка устройства для реестра (VID/PID → модель).
pub const DEVICES: &[DeviceInfo] = &[
    // --- Ajazz AK820 Pro ---
    DeviceInfo {
        model: "Ajazz AK820 Pro",
        vid: 0x0328,
        pid: 0x8207,
        connection: ConnectionType::Usb,
        layout: Layout::L75,
        has_screen: true,
        has_encoder: true,
        has_indicators: false,
    },
    DeviceInfo {
        model: "Ajazz AK820 Pro",
        vid: 0x0328,
        pid: 0x8208,
        connection: ConnectionType::Wireless2_4G,
        layout: Layout::L75,
        has_screen: true,
        has_encoder: true,
        has_indicators: false,
    },
    DeviceInfo {
        model: "Ajazz AK820 Pro",
        vid: 0x0328,
        pid: 0x8209,
        connection: ConnectionType::Bluetooth,
        layout: Layout::L75,
        has_screen: true,
        has_encoder: true,
        has_indicators: false,
    },
    // --- AULA F75 ---
    DeviceInfo {
        model: "AULA F75",
        vid: 0x258A,
        pid: 0x004B,
        connection: ConnectionType::Usb,
        layout: Layout::L75,
        has_screen: false,
        has_encoder: true,
        has_indicators: false,
    },
    DeviceInfo {
        model: "AULA F75",
        vid: 0x258A,
        pid: 0x004C,
        connection: ConnectionType::Wireless2_4G,
        layout: Layout::L75,
        has_screen: false,
        has_encoder: true,
        has_indicators: false,
    },
    DeviceInfo {
        model: "AULA F75",
        vid: 0x258A,
        pid: 0x004D,
        connection: ConnectionType::Bluetooth,
        layout: Layout::L75,
        has_screen: false,
        has_encoder: true,
        has_indicators: false,
    },
    // Ревизия «BY Tech Gaming Keyboard»: PID 0x010C, USB (см. report-descriptor)
    DeviceInfo {
        model: "AULA F75",
        vid: 0x258A,
        pid: 0x010C,
        connection: ConnectionType::Usb,
        layout: Layout::L75,
        has_screen: false,
        has_encoder: true,
        has_indicators: false,
    },
    // --- AULA F99 ---
    DeviceInfo {
        model: "AULA F99",
        vid: 0x258A,
        pid: 0x004E,
        connection: ConnectionType::Usb,
        layout: Layout::L96,
        has_screen: false,
        has_encoder: true,
        has_indicators: true,
    },
    DeviceInfo {
        model: "AULA F99",
        vid: 0x258A,
        pid: 0x004F,
        connection: ConnectionType::Wireless2_4G,
        layout: Layout::L96,
        has_screen: false,
        has_encoder: true,
        has_indicators: true,
    },
    DeviceInfo {
        model: "AULA F99",
        vid: 0x258A,
        pid: 0x0050,
        connection: ConnectionType::Bluetooth,
        layout: Layout::L96,
        has_screen: false,
        has_encoder: true,
        has_indicators: true,
    },
];

/// Найти карточку устройства по (VID, PID).
pub fn find_device_info(vid: u16, pid: u16) -> Option<&'static DeviceInfo> {
    DEVICES.iter().find(|d| d.vid == vid && d.pid == pid)
}