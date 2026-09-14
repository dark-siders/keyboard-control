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

// Мост над hidapi доступен наружу (rgb_studio получает его из фонового
// потока переподключения и строит драйвер без HID-вызовов).
pub use hid::HidBridge;

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
    /// Сохранить аппаратный эффект из встроенной таблицы прошивки в память
    /// (AULA F75: код селектора data[10], см. `aula_f75::HW_EFFECTS`).
    /// Эффект держится в памяти и применяется после перезапуска клавиатуры,
    /// без запущенного приложения. Прочие устройства — Unsupported.
    fn save_hardware_effect(&self, _effect: u8) -> Result<()> {
        Err(Error::Unsupported(
            "аппаратный эффект (save_hardware_effect) не поддерживается этим устройством",
        ))
    }
    /// Сохранить состояние подсветки «включена/выключена» в память устройства
    /// (AULA F75: data[18], 1=свет вкл при старте, 0=выкл). После перезапуска
    /// держится. Прочие устройства — Unsupported.
    fn save_hardware_light(&self, _on: bool) -> Result<()> {
        Err(Error::Unsupported(
            "сохранение света в память (save_hardware_light) не поддерживается этим устройством",
        ))
    }

    // --- Персистентное применение (2.4G-ресивер) -------------------------
    // Ресивер 3554:FA09 НЕ имеет direct-канала host-rendering (0x06-у ровно,
    // как у проводной F75): подстройка света на нём возможна ТОЛЬКО записью
    // в память через протокол 0x13 (LED config 0x04 / RGB table 0x09 / …) —
    // это сразу флеш, держится без приложения. Вызывающий (GUI/CLI) сначала
    // пробует эти методы: `Ok(true)` = применено в память (стриминг не нужен),
    // `Ok(false)` = устройство не умеет — использовать host-рендеринг.
    /// Попробовать применить настройки подсветки АППАРАТНО (в память).
    /// Возвращает `true`, если применено в память, и `false`, если устройство
    /// не поддерживает аппаратное применение (тогда — host-rendering).
    fn apply_rgb_persistent(
        &self,
        _mode: RgbMode,
        _color: Option<Color>,
        _brightness: u8,
        _speed: u8,
    ) -> Result<bool> {
        Ok(false)
    }
    /// Попробовать включить/выключить свет АППАРАТНО (в память).
    fn apply_light_persistent(&self, _on: bool) -> Result<bool> {
        Ok(false)
    }
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
    // 2.4G-ресивер «Compx» (VID 0x3554 PID 0xFA09). Отдельное USB-устройство:
    // клавиатура при подключении по 2.4G живёт за ресивером и НЕ видна в HID,
    // пока она не подключена по USB. Командный канал — hidraw1 (интерфейс 1,
    // usage_page 0xFF02), report ID 0x13 (output/input, 20 байт); батарея —
    // команда 0x05 (PSD, возвращает version в ответе 0x0A), см. aula_f75::wireless_battery.
    DeviceInfo {
        model: "AULA F75 (2.4 GHz)",
        vid: 0x3554,
        pid: 0xFA09,
        connection: ConnectionType::Wireless2_4G,
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

/// Открыть HID-мост (hidapi enumerate+open) по (VID/PID).
/// БЛОКИРУЮЩИЙ вызов — используем только в фоновых потоках (в главном
/// GTK-потоке hidapi-открытие на время пересоздания hidraw после кика
/// вешало UI; фикс 2026-09-13).
pub fn open_bridge(vid: u16, pid: u16) -> Result<HidBridge> {
    let api = hidapi::HidApi::new()?;
    Ok(hid::HidBridge::open(&api, vid, pid)?)
}

/// Собрать драйвер устройства из готового HID-моста.
/// НЕ делает HID-вызовов (только конструкторы) — безопасно вызывать в
/// главном GTK-потоке после того, как мост доставил фоновый поток.
pub fn build_device(bridge: HidBridge) -> Result<Box<dyn KeyboardDevice>> {
    let (vid, pid) = (bridge.vid, bridge.pid);
    let dev: Box<dyn KeyboardDevice> = match (vid, pid) {
        (0x0328, _) => Box::new(ajazz_ak820::AjazzAk820::open(bridge)),
        (0x258A, 0x004B..=0x004D | 0x010C) => Box::new(aula_f75::AulaF75::open(bridge)),
        (0x258A, 0x004E..=0x0050) => Box::new(aula_f99::AulaF99::open(bridge)),
        // 2.4G-ресивер Compx (3554:FA09) — обслуживает F75/F99: командный канал
        // output/input 0x13; батарея через CMD 0x05/PSD (см. aula_f75::wireless_battery).
        (0x3554, 0xFA09) => Box::new(aula_f75::AulaF75::open(bridge)),
        _ => return Err(Error::Unsupported("неподдерживаемое устройство")),
    };
    Ok(dev)
}

/// Открыть устройство по (VID/PID) с выбором командного интерфейса.
/// Используется как GUI (app.rs), так и фоновыми потоками записи в память
/// (rgb_studio, фикс 10.34) — там блокирующие hidapi-вызовы нельзя делать
/// в главном потоке GTK.
pub fn open_vidpid(vid: u16, pid: u16) -> Result<Box<dyn KeyboardDevice>> {
    build_device(open_bridge(vid, pid)?)
}