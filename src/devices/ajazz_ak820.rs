// --- Ajazz AK820 Pro ---
// Реальная реализация протокола по wsclx/ak820pro-modder (fw 1.07),
// gohv/EPOMAKER-Ajazz-AK820-Pro, TaxMachine/ajazz-keyboard-software-linux,
// barneygaumer-dot/ajazz820p_utils, fpb/ajazz-ak820-pro.
// Подсветка и время/сон — через рабочий control-канал 0xFF13 (feature reports,
// journal 10.106: подтверждён на нашем железе 0C45:8009).
// TFT-экран — через 0xFF67 (bulk/interrupt, journal 10.60/10.61).
// На нашем железе 0xFF68 НЕ РАБОТАЕТ (таймаут, Output Report Count=4096 в
// дескрипторе против 64-байтных пакетов firmware). Per-key и батарея переведены
// на рабочий 0xFF13 (journal 10.115); заводская base-раскладка захардкожена из
// ассета `data/ajazz/rgb-keyboard.xml`. Макросы перенесены на 0xFF13 по рамке
// официального DeviceDriver.exe V1.0.0.5 (`04 19` → `04 15` → `04 02`),
// keymap-запись и TFT по 0xFF68 остаются заделом (journal 10.116).
//
// 2.4G-донгл (PID 0xFDFD, usage page 0xFF60, MI_03/input3, journal 10.118):
// протокол РЕСИВЕРА, а НЕ wsclx AA-кадры 0xFF68 (они лишь эхо-подтверждались).
// Кадр 32 б `[order, len, chunk#, data(≤28), checksum = sum[0..30] & 0xFF]`;
// ресивер подтверждает кадр эхом первых 3 байт, BEGIN/COMMIT/SAVE НЕТ.
// Orders: 0x05 подсветка, 0x07 сон, 0x0C часы, 0x20 батарея. Донгл НЕ релеит
// payload ответов, поэтому запись без чтения (keymap/макросы/per-key) —
// честный Unsupported. Источники: deddroid/ak820-rs (PROTOCOL.md, работает на
// железе) + SignalRGB-плагин kshitizwagle (захват трафика офиц. ПО).

use super::hid::HidBridge;
use super::hidshell;
use super::{Error, KeyboardDevice, Result};
use crate::types::*;
use std::time::{Duration, Instant};

#[allow(unused_imports)] // используются только в linker-компиляции с chrono
use chrono::{Datelike, Timelike};

const PACKET_LEN: usize = 64;
const HEADER_LEN: usize = 8;
const PAYLOAD_PER_PACKET: usize = PACKET_LEN - HEADER_LEN; // 56
const REPORT_ID: u8 = 0;
const MAGIC_OUTGOING: u8 = 0xAA;
const MAGIC_INCOMING: u8 = 0x55;
const DEFAULT_TIMEOUT_MS: i32 = 500;

/// Команды протокола (wsclx protocol.rs, cmd-модуль).
mod cmd {
    pub const GET_DEVICE_INFO: u8 = 16;
    pub const GET_GAME_MODE: u8 = 17; // задел: get_game_mode (см. выше)
    pub const GET_KEY: u8 = 18;
    /// Чтение текущих глобальных настроек подсветки (16-байтный payload,
    /// формат как у SET_LED_EFFECT; cmd 19, журнал 10.60).
    pub const GET_LED_EFFECT: u8 = 19;
    pub const GET_FN_KEY: u8 = 22;
    pub const GET_CUSTOM_LED_DATA: u8 = 20;
    /// Заводская раскладка Fn-слоя, read-only (форма как у GET_FN_KEY).
    pub const GET_DEFAULT_FN_KEY_MATRIX: u8 = 28;
    /// Заводская раскладка базового слоя, read-only (форма как у GET_KEY).
    /// Используется приложением для кнопки «Factory default» — ставится в
    /// черновик для просмотра перед записью.
    pub const GET_DEFAULT_KEY_MATRIX: u8 = 31;
    /// задел: game-mode-блок (см. get_game_mode)
    #[allow(dead_code)]
    pub const SET_GAME_MODE: u8 = 33;
    pub const SET_KEY: u8 = 34;
    /// wsclx-канал 0xFF68: SET_LED_EFFECT. На нашем железе канал 0xFF68
    /// таймаутит (журнал 10.106), подсветка идёт через 0xFF13 — оставлено
    /// как справочная константа протокола.
    #[allow(dead_code)]
    pub const SET_LED_EFFECT: u8 = 35;
    /// wsclx-канал 0xFF68: custom-LED буфер. На нашей ревизии не отвечает,
    /// per-key идёт live-кадрами `0x20`/таблицей `0x23` через 0xFF13.
    #[allow(dead_code)]
    pub const SET_CUSTOM_LED_DATA: u8 = 36;
    pub const SET_FN_KEY: u8 = 38;
    /// Сброс к заводским настройкам. Опциональный аргумент `0xFF` = полный сброс.
    pub const SET_FACTORY_RESET: u8 = 15;
    /// Boot-анимация подсветки. Формат payload НЕ расшифрован (только значение
    /// cmd известно из wsclx PROTOCOL.md) — задел.
    #[allow(dead_code)]
    pub const SET_LED_BOOT_ANIMATION: u8 = 64;
    pub const SET_TFT_USER_ANIMATION: u8 = 80;
    pub const SET_TFT_BUILT_IN_INDEX: u8 = 81;
    /// TFT-ack: прошивка подтверждает каждый загруженный чанк командой 65,
    /// а НЕ 80 (wsclx PROTOCOL.md; при синхронном чтении ответов фильтровать
    /// по 65). У нас загрузка fire-and-forget, но константа — для будущего.
    #[allow(dead_code)]
    pub const SET_LED_USER_ANIMATION: u8 = 65;
}

/// Control-интерфейс Ajazz (usage page 0xFF13, iface 3) — время, сон и
/// загрузка картинок через 64-байтные feature-репорты (gohv/TaxMachine/
/// bernalirio, журнал 10.60). Отличается от основного командного канала
/// 0xFF68 (что использует wsclx-транспорт) — открывается отдельным дескриптором.
mod ctl {
    /// Длина feature-репорта.
    pub const PACKET_LEN: usize = 64;
    /// Командные пакеты начинаются с report id 0x04, data-пакеты — с 0x00.
    pub const REPORT_ID_CMD: u8 = 0x04;
    /// Команды (gohv protocol.rs):
    pub const START: u8 = 0x18;
    /// FINISH — закрывает транзакцию настройки (gohv usb.rs::transaction).
    pub const FINISH: u8 = 0xF0;
    pub const MODE: u8 = 0x13;
    pub const TIME: u8 = 0x28;
    pub const SLEEP: u8 = 0x17;
    /// COMMIT — фиксация принятой таблицы (Twyx22 `CMD_COMMIT`, а не 0xF0!).
    /// Прежнее имя `SAVE` сохранено ниже как алиас — им пользуется синк часов.
    pub const COMMIT: u8 = 0x02;
    pub const SAVE: u8 = 0x02;
    /// Таблица per-key цветов: 576 б = 144 слота × 4 `[index, R, G, B]`.
    pub const KEY_COLORS: u8 = 0x23;
    /// Live-кадр per-key цветов (512 б подряд `[index, R, G, B]`, без сохранения).
    pub const LIVE_COLORS: u8 = 0x20;
    /// Чтение сохранённой таблицы цветов (Twyx22 `CMD_RGB_READ`).
    pub const RGB_READ: u8 = 0xF5;
    /// Макросы: открыть сессию записи (official DeviceDriver.exe: `04 19`).
    pub const MACRO_SESSION: u8 = 0x19;
    /// Макросы: таблица index+data (official DeviceDriver.exe: `04 15`).
    pub const MACRO_DATA: u8 = 0x15;
    /// Трейлер data-пакета (в последних двух байтах).
    pub const DELIM: (u8, u8) = (0xAA, 0x55);
    /// Usage page control-интерфейса, на котором живут эти команды.
    pub const USAGE_PAGE: u16 = 0xFF13;
    /// Интерфейс control-канала (fallback к usage page).
    pub const INTERFACE: i32 = 3;
    /// Пауза ПЕРЕД отправкой командного пакета (config.xml официального ПО:
    /// `cmd_delaytime` = 35 мс; Twyx22 `CMD_DELAY_MS`).
    pub const PAUSE_MS: u64 = 35;
    /// Пауза ПЕРЕД чтением feature-ответа (Twyx22 `READ_DELAY_MS`).
    pub const READ_DELAY_MS: u64 = 50;
    /// Число повторов «грязной» транзакции записи таблицы.
    pub const RETRIES: usize = 3;
    /// Длина таблицы per-key цветов (576 б).
    pub const TABLE_LEN: usize = 576;
    /// Слотов в таблице per-key цветов (144).
    pub const TABLE_SLOTS: usize = TABLE_LEN / 4;
    /// Длина live-кадра per-key (512 б).
    pub const LIVE_LEN: usize = 512;
    /// Сколько блоков по 64 б занимает таблица (576/64 = 9).
    pub const TABLE_BLOCKS: usize = TABLE_LEN / PACKET_LEN;
}

/// 2.4G-донгл (PID 0xFDFD, usage page 0xFF60, MI_03) — протокол ресивера.
/// Кадр 32 байта: `[0]=order, [1]=len, [2]=chunk#, [3..31)=data (до 28 б),
/// [31]=checksum` (сумма байтов [0..30] & 0xFF). Ресивер подтверждает кадр
/// ЭХОМ ПЕРВЫХ ТРЁХ БАЙТОВ (order, len, chunk#); BEGIN/COMMIT/SAVE НЕТ.
/// Это НЕ wsclx AA/55-кадры 0xFF68! Источники: deddroid/ak820-rs (PROTOCOL.md,
/// работает на железе), SignalRGB-плагин kshitizwagle (захват трафика
/// официального ПО), gha-projects/ak820-mac (дизасм DeviceDriver.exe).
mod rf {
    /// Длина кадра донгла (32 байта + report id 0x00 = 33 в hidapi).
    pub const FRAME: usize = 32;
    /// Полезная нагрузка одного кадра: байты [3..31).
    pub const CHUNK: usize = 28;
    /// Подсветка: 16 б `[mode, RGB, rainbow, brightness, speed, direction, AA 55]`.
    pub const LIGHTING: u8 = 0x05;
    /// Система (сон/блокировки/response): 16 б + `AA 55`.
    /// На живой клавиатуре владельца order 0x07 НЕ применяет таймер сна
    /// (проверено 2026-11-16) — константа оставлена как справочная.
    #[allow(dead_code)]
    pub const SYSTEM: u8 = 0x07;
    /// Макросы: блок, дополненный до кратного 28; длина последнего = len % 28.
    #[allow(dead_code)]
    pub const MACROS: u8 = 0x09;
    /// Часы: 16 б `[0, 1, 5A, YY, MM, DD, hh, mm, ss, 0, weekday, 0, 0, 0, AA 55]`.
    pub const CLOCK: u8 = 0x0C;
    /// Keymap base: 21 чанк × 28 б (576 б таблицы).
    #[allow(dead_code)]
    pub const KEYMAP: u8 = 0x10;
    /// Keymap Fn: тот же формат, что KEYMAP.
    #[allow(dead_code)]
    pub const KEYMAP_FN: u8 = 0x12;
    /// Per-key цвета (SignalRGB: пишет прямо во флеш, износ ~20k стираний).
    #[allow(dead_code)]
    pub const PER_KEY: u8 = 0x14;
    /// Батарея: len 1, data [0]; ответ — эхо 3 б + проценты в байте [3].
    pub const BATTERY: u8 = 0x20;
}

/// 128-LED матрица (один слот на клавишу keymap'а).
const LED_COUNT: usize = 128;
/// Keymap: 128 слотов × 4 байта.
const TOTAL_SLOTS: usize = 128;
const SLOT_BYTES: usize = 4;
const KEYMAP_BYTES: usize = TOTAL_SLOTS * SLOT_BYTES;

/// HID usage (Keyboard/Keypad page 0x07) → индекс LED/слота подсветки AK820 Pro.
///
/// Источник: официальный `layouts/rgb-keyboard.xml` из установщика
/// `AJAZZ_AK820_Pro…V1.0.0.5.exe` (копия ассета — `data/ajazz/rgb-keyboard.xml`,
/// провенанс — `data/ajazz/README.md`), сверен с `keys.rs` проекта
/// `deddroid/ak820-rs`. Порядок индексов НЕ совпадает ни с HID usage, ни с
/// keymap-слотами — поэтому для per-key нужен именно этот пересчёт.
///
/// Таблица содержит все 81 клавишу официальной раскладки, включая Fn
/// (вендорский код `0xAF`, light index 96), и служит единым источником как для
/// `light_index_for_usage`, так и для `factory_keymap_ak820`.
const HID_USAGE_TO_LED: &[(u16, u8)] = &[
    (0x29, 1),   // Esc
    (0x3A, 2),   // F1
    (0x3B, 3),   // F2
    (0x3C, 4),   // F3
    (0x3D, 5),   // F4
    (0x3E, 6),   // F5
    (0x3F, 7),   // F6
    (0x40, 8),   // F7
    (0x41, 9),   // F8
    (0x42, 10),  // F9
    (0x43, 11),  // F10
    (0x44, 12),  // F11
    (0x45, 13),  // F12
    (0x4C, 119), // Delete
    (0x35, 19),  // Grave/~
    (0x1E, 20),  // 1
    (0x1F, 21),  // 2
    (0x20, 22),  // 3
    (0x21, 23),  // 4
    (0x22, 24),  // 5
    (0x23, 25),  // 6
    (0x24, 26),  // 7
    (0x25, 27),  // 8
    (0x26, 28),  // 9
    (0x27, 29),  // 0
    (0x2D, 30),  // Minus
    (0x2E, 31),  // Equal
    (0x2A, 103), // Backspace
    (0x4A, 117), // Home
    (0x2B, 37),  // Tab
    (0x14, 38),  // Q
    (0x1A, 39),  // W
    (0x08, 40),  // E
    (0x15, 41),  // R
    (0x17, 42),  // T
    (0x1C, 43),  // Y
    (0x18, 44),  // U
    (0x0C, 45),  // I
    (0x12, 46),  // O
    (0x13, 47),  // P
    (0x2F, 48),  // [
    (0x30, 49),  // ]
    (0x31, 67),  // Backslash
    (0x4B, 118), // PageUp
    (0x39, 55),  // CapsLock
    (0x04, 56),  // A
    (0x16, 57),  // S
    (0x07, 58),  // D
    (0x09, 59),  // F
    (0x0A, 60),  // G
    (0x0B, 61),  // H
    (0x0D, 62),  // J
    (0x0E, 63),  // K
    (0x0F, 64),  // L
    (0x33, 65),  // ;
    (0x34, 66),  // '
    (0x28, 85),  // Enter
    (0x4E, 121), // PageDown
    (0xE1, 73),  // Left Shift
    (0x1D, 74),  // Z
    (0x1B, 75),  // X
    (0x06, 76),  // C
    (0x19, 77),  // V
    (0x05, 78),  // B
    (0x11, 79),  // N
    (0x10, 80),  // M
    (0x36, 81),  // ,
    (0x37, 82),  // .
    (0x38, 83),  // /
    (0xE5, 84),  // Right Shift
    (0x52, 101), // Up
    (0xE0, 91),  // Left Ctrl
    (0xE3, 92),  // Left Win
    (0xE2, 93),  // Left Alt
    (0x2C, 94),  // Space
    (0xE6, 95),  // Right Alt
    (0xAF, 96),  // Fn (вендорский код из официального лейаута)
    (0xE4, 98),  // Right Ctrl
    (0x50, 99),  // Left
    (0x51, 100), // Down
    (0x4F, 102), // Right
];

/// Заводская base-раскладка AK820 Pro (81 клавиша) из официального установщика
/// (`layouts/rgb-keyboard.xml`, ассет `data/ajazz/rgb-keyboard.xml`).
///
/// `KeyId` = light index (в XML он же `key_index`), действие — `Press` с HID
/// usage клавиши; Fn идёт вендорским кодом `0xAF`. Заводской Fn-слой в
/// установщике отсутствует (только base), поэтому `fn_layer` пустой.
///
/// Захардкожена, потому что канал `0xFF68` на ревизии `0C45:8009` мёртв
/// (журнал 10.106) — прежний путь `GET_DEFAULT_KEY_MATRIX` не отвечает.
pub fn factory_keymap_ak820() -> Keymap {
    let base = HID_USAGE_TO_LED
        .iter()
        .map(|&(code, led)| (KeyId(led as u16), KeyAction::Press(code)))
        .collect();
    Keymap {
        base,
        fn_layer: FnLayer::default(),
    }
}

/// Режимы подсветки Ajazz (wsclx lighting.rs Mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum AjazzMode {
    Off = 0x00,
    Static = 0x01,
    SingleOn = 0x02,
    SingleOff = 0x03,
    Glittering = 0x04,
    Falling = 0x05,
    Colourful = 0x06,
    Breath = 0x07,
    Spectrum = 0x08,
    Outward = 0x09,
    Scrolling = 0x0A,
    Rolling = 0x0B,
    Rotating = 0x0C,
    Explode = 0x0D,
    Launch = 0x0E,
    Ripples = 0x0F,
    Flowing = 0x10,
    Pulsating = 0x11,
    Tilt = 0x12,
    Shuttle = 0x13,
    /// Per-key custom RGB (читает буфер из SET_CUSTOM_LED_DATA).
    Custom = 0x80,
}

impl AjazzMode {
    /// Приближённый маппинг наших RgbMode на режимы Ajazz.
    fn from_our_mode(mode: RgbMode) -> Self {
        match mode {
            RgbMode::Static => Self::Static,
            RgbMode::Breathing => Self::Breath,
            RgbMode::Wave => Self::Rolling,
            RgbMode::SpectrumCycle => Self::Spectrum,
            RgbMode::Ripple => Self::Ripples,
            RgbMode::Rain => Self::Falling,
            RgbMode::Reactive => Self::Explode,
            RgbMode::Custom => Self::Custom,
            // Аудио-реактив — host-фича (спектр звука стримится с ПК), у Ajazz
            // аппаратного «послушать звук» нет; ближайший статичный режим.
            RgbMode::Audio => Self::Custom,
        }
    }

    /// Код аппаратного эффекта (0x00..0x13, тот же enum, что у wsclx) → режим.
    /// Используется для `--hw-effect` на 2.4G-донгле, где коды эффектов
    /// совпадают с таблицей wsclx (0x14+ не существуют).
    fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            0x00 => Self::Off,
            0x01 => Self::Static,
            0x02 => Self::SingleOn,
            0x03 => Self::SingleOff,
            0x04 => Self::Glittering,
            0x05 => Self::Falling,
            0x06 => Self::Colourful,
            0x07 => Self::Breath,
            0x08 => Self::Spectrum,
            0x09 => Self::Outward,
            0x0A => Self::Scrolling,
            0x0B => Self::Rolling,
            0x0C => Self::Rotating,
            0x0D => Self::Explode,
            0x0E => Self::Launch,
            0x0F => Self::Ripples,
            0x10 => Self::Flowing,
            0x11 => Self::Pulsating,
            0x12 => Self::Tilt,
            0x13 => Self::Shuttle,
            _ => return None,
        })
    }
}

/// Человекочитаемое имя аппаратного кода эффекта (таблица wsclx 0x00..0x13)
/// — для вывода `kbc rgb --hw-effect` на 2.4G-донгле Ajazz.
pub fn wl_mode_name(code: u8) -> Option<&'static str> {
    Some(match code {
        0x00 => "Off",
        0x01 => "Static",
        0x02 => "Single On",
        0x03 => "Single Off",
        0x04 => "Glittering",
        0x05 => "Falling",
        0x06 => "Colourful",
        0x07 => "Breath",
        0x08 => "Spectrum",
        0x09 => "Outward",
        0x0A => "Scrolling",
        0x0B => "Rolling",
        0x0C => "Rotating",
        0x0D => "Explode",
        0x0E => "Launch",
        0x0F => "Ripples",
        0x10 => "Flowing",
        0x11 => "Pulsating",
        0x12 => "Tilt",
        0x13 => "Shuttle",
        _ => return None,
    })
}

/// Тип события макроса (wsclx macros.rs MacroActionKind).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MacroFamily {
    Keyboard,
    // Задел: мышь-кнопки в макросах; сейчас не конструируется.
    #[allow(dead_code)]
    Mouse,
}

impl MacroFamily {
    /// Wire-флаги: Keyboard press/release = 0xB0/0x30, Mouse = 0x90/0x10.
    fn wire_flags(self, is_press: bool) -> u8 {
        match (self, is_press) {
            (Self::Keyboard, true) => 0xB0,
            (Self::Keyboard, false) => 0x30,
            (Self::Mouse, true) => 0x90,
            (Self::Mouse, false) => 0x10,
        }
    }
}

/// Внутреннее состояние Ajazz.
pub struct AjazzAk820 {
    bridge: HidBridge,
    // Последние применённые значения (для «дозаписи» при нажатии CLI/GUI).
    last_mode: std::cell::Cell<u8>,
    last_brightness: std::cell::Cell<u8>,
    last_speed: std::cell::Cell<u8>,
    last_color: std::cell::Cell<[u8; 3]>,
    /// Кэш записанных макросов (100 слотов). Чтения макросов из прошивки нет,
    /// поэтому при записи одного слота остальные сохраняются отсюда. Кэш живёт
    /// до пересоздания объекта (перезапуска процесса).
    macro_cache: std::sync::Mutex<Vec<Option<Vec<KeyAction>>>>,
}

impl AjazzAk820 {
    /// Открыть устройство (мост уже на командном канале, usage page 0xFF68).
    pub fn open(bridge: HidBridge) -> Self {
        Self {
            bridge,
            last_mode: std::cell::Cell::new(AjazzMode::Static as u8),
            last_brightness: std::cell::Cell::new(0),
            last_speed: std::cell::Cell::new(0),
            last_color: std::cell::Cell::new([255, 255, 255]),
            macro_cache: std::sync::Mutex::new(vec![None; 100]),
        }
    }

    // --- Транспорт ----------------------------------------------------

    /// Собрать исходящий кадр (wsclx protocol::build_frame).
    fn build_frame(
        cmd: u8,
        len_or_type: u8,
        addr: u16,
        payload: &[u8],
        last_packet: bool,
    ) -> [u8; PACKET_LEN] {
        let mut pkt = [0u8; PACKET_LEN];
        pkt[0] = MAGIC_OUTGOING;
        pkt[1] = cmd;
        pkt[2] = len_or_type;
        pkt[3] = (addr & 0xFF) as u8;
        pkt[4] = ((addr >> 8) & 0xFF) as u8;
        pkt[6] = if last_packet { 1 } else { 0 };
        let n = payload.len().min(PAYLOAD_PER_PACKET);
        pkt[HEADER_LEN..HEADER_LEN + n].copy_from_slice(&payload[..n]);
        pkt
    }

    fn send_frame(&self, frame: &[u8; PACKET_LEN]) -> Result<()> {
        let mut buf = [0u8; PACKET_LEN + 1];
        buf[0] = REPORT_ID;
        buf[1..].copy_from_slice(frame);
        self.bridge.send_output(&buf)?;
        Ok(())
    }

    /// Прочитать input-репорт до таймаута, дождаться ответа на `want_cmd`.
    fn read_response(&self, want_cmd: u8, timeout_ms: i32) -> Result<Vec<u8>> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
        let mut buf = [0u8; PACKET_LEN];
        loop {
            let remaining_ms = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(i32::MAX as u128) as i32;
            if remaining_ms <= 0 {
                return Err(Error::Protocol(format!(
                    "таймаут ожидания ответа на команду 0x{:02x}",
                    want_cmd
                )));
            }
            let n = self.bridge.read_input(&mut buf, remaining_ms)?;
            if n == 0 {
                continue;
            }
            if buf[0] != MAGIC_INCOMING {
                continue;
            }
            if buf[1] != want_cmd {
                continue;
            }
            return Ok(buf[HEADER_LEN..n.min(PACKET_LEN)].to_vec());
        }
    }

    /// Одиночный GET (content_size <= 56). Возвращает payload.
    fn get(&self, cmd_byte: u8, content_size: usize) -> Result<Vec<u8>> {
        if content_size > PAYLOAD_PER_PACKET {
            return Err(Error::Protocol("get: размер больше одного чанка".into()));
        }
        let frame = Self::build_frame(cmd_byte, content_size as u8, 0, &[], true);
        self.send_frame(&frame)?;
        let payload = self.read_response(cmd_byte, DEFAULT_TIMEOUT_MS)?;
        Ok(payload.into_iter().take(content_size).collect())
    }

    /// Одиночный SET (payload <= 56). Дренирует ответ-эхо.
    fn set(&self, cmd_byte: u8, payload: &[u8]) -> Result<()> {
        if payload.len() > PAYLOAD_PER_PACKET {
            return Err(Error::Protocol("set: размер больше одного чанка".into()));
        }
        let frame = Self::build_frame(cmd_byte, payload.len() as u8, 0, payload, true);
        self.send_frame(&frame)?;
        let _ = self.read_response(cmd_byte, DEFAULT_TIMEOUT_MS);
        Ok(())
    }

    /// Многочанковый GET (например keymap 512 байт).
    fn get_many(&self, cmd_byte: u8, content_size: usize) -> Result<Vec<u8>> {
        self.get_many_at(cmd_byte, 0, content_size)
    }

    /// Многочанковый GET с произвольным базовым адресом (макросы: index 0,
    /// отдельные data-блоки по offset из index).
    fn get_many_at(
        &self,
        cmd_byte: u8,
        addr_base: u16,
        content_size: usize,
    ) -> Result<Vec<u8>> {
        let chunk = PAYLOAD_PER_PACKET;
        let num_chunks = content_size.div_ceil(chunk).max(1);
        let mut out = Vec::with_capacity(content_size);
        for i in 0..num_chunks {
            let addr = addr_base.wrapping_add((i * chunk) as u16);
            let remaining = content_size - i * chunk;
            let this_size = remaining.min(chunk);
            let is_last = i == num_chunks - 1;
            let frame = Self::build_frame(cmd_byte, this_size as u8, addr, &[], is_last);
            self.send_frame(&frame)?;
            let payload = self.read_response(cmd_byte, DEFAULT_TIMEOUT_MS)?;
            let take = payload.len().min(this_size);
            out.extend_from_slice(&payload[..take]);
        }
        out.truncate(content_size);
        Ok(out)
    }

    /// Многочанковый SET с двухфазной записью (index без commit → данные с commit).
    fn set_many_at(
        &self,
        cmd_byte: u8,
        addr_base: u16,
        payload: &[u8],
        commit_on_last: bool,
    ) -> Result<()> {
        let chunk = PAYLOAD_PER_PACKET;
        let num_chunks = payload.len().div_ceil(chunk).max(1);
        for i in 0..num_chunks {
            let addr = addr_base.wrapping_add((i * chunk) as u16);
            let start = i * chunk;
            let this_size = (payload.len() - start).min(chunk);
            let is_final_chunk = i == num_chunks - 1;
            let last_flag = commit_on_last && is_final_chunk;
            let frame = Self::build_frame(
                cmd_byte,
                this_size as u8,
                addr,
                &payload[start..start + this_size],
                last_flag,
            );
            self.send_frame(&frame)?;
            let _ = self.read_response(cmd_byte, DEFAULT_TIMEOUT_MS);
        }
        Ok(())
    }

    // --- Транспорт 2.4G-донгла (usage page 0xFF60, MI_03) ------------------
    // Протокол РЕСИВЕРА, не wsclx-кадры 0xFF68 (журнал 10.114/A1 + deddroid/
    // ak820-rs): кадр 32 б `[order, len, chunk#, data(≤28), checksum]`,
    // ресивер эхо-подтверждает первые 3 байта. См. комментарий модуля `rf`.

    /// Донгл 2.4G? (PID 0xFDFD → ConnectionType::Wireless2_4G).
    fn is_wireless(&self) -> bool {
        self.bridge.connection == ConnectionType::Wireless2_4G
    }

    /// Собрать 32-байтный кадр ресивера: order, len, chunk#, data, checksum
    /// (сумма байтов [0..30] & 0xFF на [31]).
    fn build_rf_frame(order: u8, len: u8, chunk: u8, data: &[u8]) -> [u8; rf::FRAME] {
        let mut f = [0u8; rf::FRAME];
        f[0] = order;
        f[1] = len;
        f[2] = chunk;
        let n = data.len().min(rf::CHUNK);
        f[3..3 + n].copy_from_slice(&data[..n]);
        let sum: u32 = f[..rf::FRAME - 1].iter().map(|&b| u32::from(b)).sum();
        f[rf::FRAME - 1] = (sum & 0xFF) as u8;
        f
    }

    /// Отправить кадр ресиверу (hidapi-буфер 33 б: report id 0x00 + 32-б кадр).
    fn send_rf_frame(&self, frame: &[u8; rf::FRAME]) -> Result<()> {
        let mut buf = [0u8; rf::FRAME + 1];
        buf[0] = REPORT_ID;
        buf[1..].copy_from_slice(frame);
        self.bridge.send_output(&buf)?;
        Ok(())
    }

    /// Прочитать эхо ресивера: первые 3 байта кадра (order, len, chunk#).
    /// Возвращает полный 32-байтный эхо-кадр (для батареи — проценты в [3]).
    fn read_rf_echo(&self, frame: &[u8; rf::FRAME], timeout_ms: i32) -> Result<Option<[u8; rf::FRAME]>> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
        let mut buf = [0u8; rf::FRAME + 1];
        loop {
            let remaining_ms = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(i32::MAX as u128) as i32;
            if remaining_ms <= 0 {
                return Ok(None);
            }
            let n = self.bridge.read_input(&mut buf, remaining_ms)?;
            if n == 0 {
                continue;
            }
            // hidapi кладёт report id первым байтом (0x00 для unnumbered).
            let echo: [u8; rf::FRAME] = if n >= rf::FRAME + 1 && buf[0] == REPORT_ID {
                buf[1..=rf::FRAME].try_into().unwrap()
            } else if n >= rf::FRAME {
                buf[..rf::FRAME].try_into().unwrap()
            } else {
                continue;
            };
            if echo[..3] != frame[..3] {
                continue;
            }
            return Ok(Some(echo));
        }
    }

    /// Отправить кадр и дождаться эха. Пробуем до 3 раз: первый ответ после
    /// открытия канала может прийти со СЛЕДУЮЩИМ запросом (aklink DONGLE.md),
    /// а т.к. кадр идентичен, «чужой» отложенный ack тоже засчитывается.
    /// `Ok(None)` на третьей попытке — клавиатура спит на линке.
    fn rf_send(
        &self,
        order: u8,
        len: u8,
        chunk: u8,
        data: &[u8],
    ) -> Result<Option<[u8; rf::FRAME]>> {
        let frame = Self::build_rf_frame(order, len, chunk, data);
        for attempt in 0..3 {
            self.send_rf_frame(&frame)?;
            std::thread::sleep(Duration::from_millis(5));
            match self.read_rf_echo(&frame, DEFAULT_TIMEOUT_MS)? {
                Some(echo) => return Ok(Some(echo)),
                None if attempt < 2 => std::thread::sleep(Duration::from_millis(50)),
                None => return Ok(None),
            }
        }
        unreachable!()
    }

    // --- Подсветка ----------------------------------------------------

    /// 64-байтный mode-data пакет (gohv protocol::mode_data_packet).
    /// ВАЖНО: `p[0]` = код эффекта, и именно он становится report id при
    /// отправке (gohv usb.rs: «byte 0 is the mode value itself, which hidapi
    /// sends as the report ID»). Дельмитер — `p[14]=0x55, p[15]=0xAA`.
    fn mode_data_packet(
        mode: AjazzMode,
        color: [u8; 3],
        rainbow: bool,
        brightness: u8,
        speed: u8,
        direction: u8,
    ) -> [u8; ctl::PACKET_LEN] {
        let mut p = [0u8; ctl::PACKET_LEN];
        p[0] = mode as u8; // report id = код эффекта
        p[1] = color[0];
        p[2] = color[1];
        p[3] = color[2];
        p[8] = rainbow as u8;
        p[9] = brightness.min(5);
        p[10] = speed.min(5);
        p[11] = direction;
        p[14] = ctl::DELIM.1; // 0x55
        p[15] = ctl::DELIM.0; // 0xAA
        p
    }

    /// Применить подсветку через рабочий control-канал 0xFF13 (feature reports).
    /// Полная транзакция (gohv usb.rs::transaction):
    /// START (0x18) → MODE (0x13) → mode-data → FINISH (0xF0).
    /// На 2.4G-донгле 0xFF13 отсутствует — маршрутизируем в apply_lighting_wl.
    fn apply_lighting(&self, mode: AjazzMode) -> Result<()> {
        if self.is_wireless() {
            return self.apply_lighting_wl(mode);
        }
        let dev = self.open_control()?;

        // 1) START
        self.send_ctl(&dev, &Self::control_packet(ctl::START, 0x00, 0x01))?;
        // 2) MODE preamble
        self.send_ctl(&dev, &Self::control_packet(ctl::MODE, 0x00, 0x01))?;
        // 3) mode-data (report id = код эффекта)
        let data = Self::mode_data_packet(
            mode,
            self.last_color.get(),
            false,
            self.last_brightness.get(),
            self.last_speed.get(),
            0,
        );
        self.send_ctl(&dev, &data)?;
        // 4) FINISH — завершение транзакции
        self.send_ctl(&dev, &Self::control_packet(ctl::FINISH, 0x00, 0x01))?;
        std::thread::sleep(Duration::from_millis(100));

        self.last_mode.set(mode as u8);
        Ok(())
    }

    /// 16-байтный payload подсветки ресивера (deddroid/ak820-rs `rf_short` от
    /// проводного lighting-пакета): первые 14 байт проводного пакета
    /// `[mode, RGB, 0,0,0,0, rainbow, brightness, speed, direction, 0,0]` +
    /// `AA 55` на [14..15]. На донгле ложится в кадр order 0x05.
    fn rf_lighting_data(
        mode: AjazzMode,
        rgb: [u8; 3],
        rainbow: bool,
        brightness: u8,
        speed: u8,
        direction: u8,
    ) -> [u8; 16] {
        let mut d = [0u8; 16];
        d[0] = mode as u8;
        if mode != AjazzMode::Off {
            d[1] = rgb[0];
            d[2] = rgb[1];
            d[3] = rgb[2];
            d[8] = rainbow as u8;
            d[9] = brightness.min(5);
            d[10] = speed.min(5);
            d[11] = direction;
        }
        d[14] = 0xAA;
        d[15] = 0x55;
        d
    }

    /// Применить подсветку через 2.4G-донгл: кадр ресивера order 0x05
    /// (LIGHTING) с 16-байтным payload. Протокол — deddroid/ak820-rs
    /// (работает на железе) + SignalRGB-захват официального ПО; БЕЗ
    /// BEGIN/COMMIT/SAVE — ресивер подтверждает кадр эхом первых 3 байт.
    fn apply_lighting_wl(&self, mode: AjazzMode) -> Result<()> {
        let data = Self::rf_lighting_data(
            mode,
            self.last_color.get(),
            false,
            self.last_brightness.get(),
            self.last_speed.get(),
            0,
        );
        match self.rf_send(rf::LIGHTING, 16, 0, &data)? {
            Some(_) => {}
            None => {
                return Err(Error::Protocol(
                    "ресивер 2.4G не ответил: клавиатура спит на линке — \
                     нажмите клавишу и повторите"
                        .into(),
                ))
            }
        }
        self.last_mode.set(mode as u8);
        Ok(())
    }

    /// Чтение текущего custom-LED буфера (512 байт, 128×4).
    /// Задел: на нашей ревизии канал 0xFF68 не отвечает (журнал 10.114),
    /// per-key идёт через `0x20`/`0x23` на 0xFF13.
    #[allow(dead_code)]
    fn get_custom_led(&self) -> Result<Vec<u8>> {
        self.get_many(cmd::GET_CUSTOM_LED_DATA, LED_COUNT * 4)
    }

    /// Запись custom-LED буфера (512 байт). См. `get_custom_led` — задел.
    #[allow(dead_code)]
    fn set_custom_led(&self, buffer: &[u8]) -> Result<()> {
        if buffer.len() != LED_COUNT * 4 {
            return Err(Error::Protocol("custom-LED буфер не 512 байт".into()));
        }
        self.set_many_at(cmd::SET_CUSTOM_LED_DATA, 0, buffer, true)
    }

    /// Прочитать info-структуру (wsclx system.rs: firmware, батарея, профиль).
    fn get_device_info(&self) -> Result<[u8; 48]> {
        let payload = self.get(cmd::GET_DEVICE_INFO, 48)?;
        let mut b = [0u8; 48];
        b[..payload.len().min(48)].copy_from_slice(&payload);
        Ok(b)
    }

    /// Прочитать game-mode (wsclx system.rs, 56 байт).
    /// С 10.63 таймер сна ушёл на control-канал; этот метод и константы
    /// GET/SET_GAME_MODE остались как задел для чтения/записи других полей
    /// game-mode-блока (раскладка/свет/поведение) — держим помеченным.
    #[allow(dead_code)]
    fn get_game_mode(&self) -> Result<[u8; 56]> {
        let payload = self.get(cmd::GET_GAME_MODE, 56)?;
        let mut b = [0u8; 56];
        b[..payload.len().min(56)].copy_from_slice(&payload);
        Ok(b)
    }

    // --- Keymap --------------------------------------------------------

    /// Кодировка одного слота keymap из наших KeyAction.
    fn slot_encode(action: &KeyAction) -> [u8; SLOT_BYTES] {
        match action {
            KeyAction::Press(u) => [2, 0, (*u & 0xFF) as u8, 0],
            KeyAction::Media(m) => [3, (*m & 0xFF) as u8, (*m >> 8) as u8, 0],
            KeyAction::Modifier(m) => [2, 0, 0xE0 + (*m & 0x07), 0],
            KeyAction::FnLayer(l) => [10, *l, 0, 0],
            KeyAction::Macro(id) => [6, *id, 0, 0],
        }
    }

    /// Декодирование слота keymap: вернёт None для Default/незнакомых.
    fn slot_decode(bytes: [u8; SLOT_BYTES]) -> Option<KeyAction> {
        let p = bytes[0];
        match p {
            0 => None,
            2 => Some(KeyAction::Press(bytes[2] as u16)),
            3 => Some(KeyAction::Media((bytes[1] as u16) | ((bytes[2] as u16) << 8))),
            6 => Some(KeyAction::Macro(bytes[1])),
            10 => Some(KeyAction::FnLayer(bytes[1])),
            _ => None,
        }
    }

    /// Прочитать BASE keymap (128 слотов).
    fn get_base_keymap(&self) -> Result<Vec<(u8, [u8; SLOT_BYTES])>> {
        let payload = self.get_many(cmd::GET_KEY, KEYMAP_BYTES)?;
        Ok((0..TOTAL_SLOTS)
            .filter_map(|i| {
                let base = i * SLOT_BYTES;
                if base + SLOT_BYTES > payload.len() {
                    return None;
                }
                Some((i as u8, payload[base..base + SLOT_BYTES].try_into().unwrap()))
            })
            .collect())
    }

    /// Записать BASE keymap целиком.
    fn set_base_keymap(&self, slots: &[(u8, [u8; SLOT_BYTES])]) -> Result<()> {
        let mut out = vec![0u8; KEYMAP_BYTES];
        for (slot, bytes) in slots {
            let idx = (*slot as usize) * SLOT_BYTES;
            out[idx..idx + SLOT_BYTES].copy_from_slice(bytes);
        }
        self.set_many_at(cmd::SET_KEY, 0, &out, true)
    }

    /// Прочитать FN-слой.
    fn get_fn_keymap(&self) -> Result<Vec<(u8, [u8; SLOT_BYTES])>> {
        let payload = self.get_many(cmd::GET_FN_KEY, KEYMAP_BYTES)?;
        Ok((0..TOTAL_SLOTS)
            .filter_map(|i| {
                let base = i * SLOT_BYTES;
                if base + SLOT_BYTES > payload.len() {
                    return None;
                }
                Some((i as u8, payload[base..base + SLOT_BYTES].try_into().unwrap()))
            })
            .collect())
    }

    /// Записать FN-слой целиком.
    fn set_fn_keymap(&self, slots: &[(u8, [u8; SLOT_BYTES])]) -> Result<()> {
        let mut out = vec![0u8; KEYMAP_BYTES];
        for (slot, bytes) in slots {
            let idx = (*slot as usize) * SLOT_BYTES;
            out[idx..idx + SLOT_BYTES].copy_from_slice(bytes);
        }
        self.set_many_at(cmd::SET_FN_KEY, 0, &out, true)
    }

    // --- Макросы --------------------------------------------------------

    /// Наши KeyAction → набор макро-событий (press + release, 0 задержки).
    fn actions_from_keys(actions: &[KeyAction]) -> Vec<(u16, u8, MacroFamily, bool)> {
        let mut out = Vec::new();
        for a in actions {
            let (code, family) = match a {
                KeyAction::Press(u) => ((*u & 0xFF) as u8, MacroFamily::Keyboard),
                KeyAction::Media(m) => ((*m & 0xFF) as u8, MacroFamily::Keyboard),
                KeyAction::Modifier(m) => (0xE0 + (*m & 0x07), MacroFamily::Keyboard),
                KeyAction::FnLayer(l) => (*l, MacroFamily::Keyboard),
                KeyAction::Macro(id) => (*id & 0x7F, MacroFamily::Keyboard),
            };
            out.push((0, code, family, true));
            out.push((0, code, family, false));
        }
        out
    }
}

// --- Control-интерфейс (время, сон) и анимация TFT ------------------

impl AjazzAk820 {
    /// Открыть control-интерфейс (usage page 0xFF13, fallback — iface 3)
    /// СВЕЖИМ HID-дескриптором. В отличие от основного канала 0xFF68 (wsclx-
    /// транспорт), эти команды живут на втором интерфейсе и требуют
    /// feature-репортов (gohv/TaxMachine/bernalirio; журнал 10.60).
    fn open_control(&self) -> Result<hidapi::HidDevice> {
        let api = hidapi::HidApi::new().map_err(Error::from)?;
        let mut best_rank = 0u8;
        let mut best_path: Option<Vec<u8>> = None;
        for d in api.device_list() {
            if d.vendor_id() != self.bridge.vid || d.product_id() != self.bridge.pid {
                continue;
            }
            let p = d.path();
            if p.to_bytes().is_empty() {
                continue;
            }
            let rank = if d.usage_page() == ctl::USAGE_PAGE {
                2
            } else if d.interface_number() == ctl::INTERFACE {
                1
            } else {
                0
            };
            if rank > best_rank {
                best_rank = rank;
                best_path = Some(p.to_bytes().to_vec());
            }
        }
        let path = best_path.ok_or_else(|| {
            Error::Protocol("control-интерфейс Ajazz (usage 0xFF13 / iface 3) не найден".into())
        })?;
        let cstr = std::ffi::CString::new(path)
            .map_err(|_| Error::Protocol("control путь содержит NUL".into()))?;
        api.open_path(&cstr).map_err(Error::from)
    }

    /// 64-байтный командный пакет control-канала:
    /// `[0]=0x04 (report id), [1]=cmd, [2]=byte2, [8]=byte8`, остальное 0.
    fn control_packet(cmd: u8, byte2: u8, byte8: u8) -> [u8; ctl::PACKET_LEN] {
        let mut p = [0u8; ctl::PACKET_LEN];
        p[0] = ctl::REPORT_ID_CMD;
        p[1] = cmd;
        p[2] = byte2;
        p[8] = byte8;
        p
    }

    /// Отправить feature-репорт по control-каналу 0xFF13. `pkt[0]` — ЛОГИЧЕСКИЙ
    /// первый байт пакета (0x04 для команд, код эффекта для mode-data).
    ///
    /// КРИТИЧНО (живое железо 0C45:8009, журнал 10.112): 0xFF13 — НЕнумерованный
    /// 64-байтный feature-report (дескриптор `0613FF...B102` без `85 xx`). hidapi
    /// на hidraw трактует `buf[0]` как НОМЕР report-а, поэтому 64-байтный пакет с
    /// `buf[0]=0x04` уходил бы как «report id 4» (его у устройства нет) и прошивка
    /// его молча игнорировала. Шлём 65 байт: ведущий `0x00` (report id) + те же 64
    /// байта пакета (framing B). Подтверждено на железе: так оживают подсветка и
    /// часы. GET_REPORT-хендшейк — только для команд (pkt[0]==0x04, gohv usb.rs);
    /// на нашем железе он отдаёт Broken pipe, но это безвредно (ошибку игнорируем).
    fn send_ctl(&self, dev: &hidapi::HidDevice, pkt: &[u8; ctl::PACKET_LEN]) -> Result<()> {
        let buf = Self::ctl_frame(pkt);
        dev.send_feature_report(&buf).map_err(Error::from)?;
        if pkt[0] == ctl::REPORT_ID_CMD {
            let mut resp = [0u8; ctl::PACKET_LEN + 1];
            let _ = dev.get_feature_report(&mut resp);
        }
        std::thread::sleep(Duration::from_millis(10));
        Ok(())
    }

    /// Транспортная обёртка control-пакета: ведущий report id `0x00` (framing B)
    /// + 64 байта логического пакета. Причина — см. `send_ctl` (журнал 10.112).
    fn ctl_frame(pkt: &[u8; ctl::PACKET_LEN]) -> [u8; ctl::PACKET_LEN + 1] {
        let mut buf = [0u8; ctl::PACKET_LEN + 1];
        buf[1..].copy_from_slice(pkt);
        buf
    }

    // --- Транспорт официального протокола (этапы 0-2) -------------------
    //
    // Сверено с реверсом официального ПО: deddroid/ak820-rs
    // (`device.rs` feature/command/data/commit_checked/upload_table) и
    // Twyx22/ak820-center (`device.cpp` exchange/startSession/commit),
    // плюс gohv/EPOMAKER-Ajazz-AK820-Pro. Отличие от подтверждённого на
    // железе `send_ctl` (журнал 10.112) — паузы и чтение ответа, поэтому
    // живут ОТДЕЛЬНЫМИ методами: проверенный путь подсветки не тронут.

    /// 16-битная сумма всех байтов (wrapping). Прошивка возвращает её в
    /// COMMIT-ответе, чтобы приложение убедилось в целостности таблицы.
    fn sum16(data: &[u8]) -> u16 {
        data.iter()
            .fold(0u16, |acc, b| acc.wrapping_add(*b as u16))
    }

    /// Командный пакет с 16-битным счётчиком в `[8..10]`: число следующих
    /// data-пакетов / блоков (deddroid `protocol::control`).
    fn control_count(cmd: u8, byte2: u8, count: u16) -> [u8; ctl::PACKET_LEN] {
        let mut p = Self::control_packet(cmd, byte2, 0);
        p[8] = (count & 0xFF) as u8;
        p[9] = (count >> 8) as u8;
        p
    }

    /// Отправить feature-репорт и НЕ ждать ответа (data-пакеты таблиц).
    /// `pause` — пауза ПЕРЕД отправкой (как в Twyx22 `exchange`).
    fn ctl_feature(
        dev: &hidapi::HidDevice,
        pkt: &[u8; ctl::PACKET_LEN],
        pause: Duration,
    ) -> Result<()> {
        std::thread::sleep(pause);
        let buf = Self::ctl_frame(pkt);
        dev.send_feature_report(&buf).map_err(Error::from)?;
        Ok(())
    }

    /// Отправить пакет и прочитать ответ. `None` = ответа нет: на нашем железе
    /// `get_feature_report` может отдавать Broken pipe — это НЕ ошибка
    /// транспорта, вызывающая сторона решает, критично ли отсутствие ответа.
    fn ctl_exchange(
        dev: &hidapi::HidDevice,
        pkt: &[u8; ctl::PACKET_LEN],
    ) -> Result<Option<[u8; ctl::PACKET_LEN]>> {
        Self::ctl_feature(dev, pkt, Duration::from_millis(ctl::PAUSE_MS))?;
        std::thread::sleep(Duration::from_millis(ctl::READ_DELAY_MS));
        let mut resp = [0u8; ctl::PACKET_LEN + 1];
        match dev.get_feature_report(&mut resp) {
            Ok(_) => {
                let mut out = [0u8; ctl::PACKET_LEN];
                out.copy_from_slice(&resp[1..]);
                Ok(Some(out))
            }
            Err(_) => Ok(None),
        }
    }

    /// Отправить блок данных чанками по 64 б без чтения ответа
    /// (deddroid `data`/`packets`; нулевое дополнение последнего чанка).
    fn ctl_data(dev: &hidapi::HidDevice, data: &[u8], pause: Duration) -> Result<()> {
        for chunk in data.chunks(ctl::PACKET_LEN) {
            let mut pkt = [0u8; ctl::PACKET_LEN];
            pkt[..chunk.len()].copy_from_slice(chunk);
            Self::ctl_feature(dev, &pkt, pause)?;
        }
        Ok(())
    }

    /// COMMIT + сверка 16-битной суммы, которую прошивка вернула в ответе
    /// (deddroid `commit_checked`). `Some(true)` — сумма совпала, `Some(false)` —
    /// не совпала, `None` — ответа не было. `None` НЕ считается провалом:
    /// иначе на нашем железе (Broken pipe на GET_REPORT) запись таблиц падала
    /// бы всегда.
    fn ctl_commit_checked(dev: &hidapi::HidDevice, data: &[u8]) -> Result<Option<bool>> {
        let reply = Self::ctl_exchange(dev, &Self::control_count(ctl::COMMIT, 0, 0))?;
        Ok(reply.map(|r| u16::from_le_bytes([r[5], r[6]]) == Self::sum16(data)))
    }

    /// Записать таблицу на control-канал:
    /// `START(0x18)` → `<команда>` (count = число блоков) → данные →
    /// `COMMIT` со сверкой суммы → `FINISH(0xF0)`.
    /// Одна и та же последовательность у Twyx22 (`writeCustomRgb`) и deddroid
    /// (`upload_table`). Повторяем до `RETRIES` раз, если COMMIT вернул
    /// несовпадение суммы (200 мс между попытками).
    fn upload_table(
        &self,
        dev: &hidapi::HidDevice,
        command: u8,
        table: &[u8],
        what: &str,
    ) -> Result<()> {
        let blocks = table.len().div_ceil(ctl::PACKET_LEN) as u16;
        let pause = Duration::from_millis(ctl::PAUSE_MS);
        for attempt in 1..=ctl::RETRIES {
            Self::ctl_exchange(dev, &Self::control_count(ctl::START, 0, 0))?;
            Self::ctl_exchange(dev, &Self::control_count(command, 0, blocks))?;
            Self::ctl_data(dev, table, pause)?;
            let ok = Self::ctl_commit_checked(dev, table)?;
            if ok != Some(false) {
                // Some(true) — сумма сошлась; None — ответа нет, но запись
                // отправлена (на нашем железе GET_REPORT недоступен).
                Self::ctl_exchange(dev, &Self::control_count(ctl::SAVE, 0, 0))?;
                return Ok(());
            }
            if attempt < ctl::RETRIES {
                std::thread::sleep(Duration::from_millis(200));
            }
        }
        Err(Error::Protocol(format!(
            "клавиатура не подтвердила приём таблицы ({what}) после {} попыток",
            ctl::RETRIES
        )))
    }

    const MACRO_SLOTS: usize = 100;
    const MACRO_INDEX_LEN: usize = Self::MACRO_SLOTS * 4; // 400 б

    /// Собрать таблицу макросов в формате официального драйвера
    /// (`DeviceDriver.exe`, команда `04 15`; сверено с дизассемблером V1.0.0.5):
    /// 100 слотов × 4 байта index (u16 LE смещения записи, пустой = FF FF FF FF),
    /// затем записи: u16 LE `events*2`, 6 байт reserved, события по 4 б
    /// `[delay_lo, delay_hi, key_code, wire_flags]`. Задержка минимум 10 мс.
    fn build_macro_table(macros: &[Option<Vec<KeyAction>>]) -> Vec<u8> {
        let mut table = vec![0xFFu8; Self::MACRO_INDEX_LEN];
        let mut offset = Self::MACRO_INDEX_LEN;
        for (id, actions) in macros.iter().enumerate().take(Self::MACRO_SLOTS) {
            let Some(actions) = actions else { continue };
            let events = Self::actions_from_keys(actions);
            if events.is_empty() {
                continue;
            }
            table[id * 4] = (offset & 0xFF) as u8;
            table[id * 4 + 1] = ((offset >> 8) & 0xFF) as u8;
            // Официальный драйвер пишет u16 LE = events*2 и 6 зарезервированных
            // байт (заголовок записи занимает 8 байт).
            let n = events.len();
            let count = (n as u16) * 2;
            table.extend_from_slice(&count.to_le_bytes());
            table.extend_from_slice(&[0u8; 6]);
            for (delay, code, family, press) in events {
                let delay = delay.max(10);
                table.extend_from_slice(&delay.to_le_bytes());
                table.push(code);
                table.push(family.wire_flags(press));
            }
            offset += 8 + n * 4;
        }
        // Официальный драйвер выравнивает длину до 64-байтных блоков, добавляя
        // один блок, если длина кратна 64, иначе два; AA 55 — в конце.
        let blocks = offset.div_ceil(ctl::PACKET_LEN)
            + if offset % ctl::PACKET_LEN == 0 { 1 } else { 2 };
        table.resize(blocks * ctl::PACKET_LEN, 0);
        let len = table.len();
        table[len - 2] = ctl::DELIM.0;
        table[len - 1] = ctl::DELIM.1;
        table
    }

    /// Записать таблицу макросов на рабочий control-канал 0xFF13:
    /// `04 19` (сессия) → `04 15` (count = число 64-байтных блоков) → данные →
    /// `04 02` (COMMIT). FINISH не отправляется: в официальном драйвере после
    /// COMMIT сессия закрывается сама.
    fn upload_macro_table(&self, table: &[u8]) -> Result<()> {
        let dev = self.open_control()?;
        let blocks = (table.len() / ctl::PACKET_LEN) as u16;
        Self::ctl_exchange(&dev, &Self::control_packet(ctl::MACRO_SESSION, 0, 0))?;
        Self::ctl_exchange(
            &dev,
            &Self::control_count(ctl::MACRO_DATA, 0, blocks),
        )?;
        std::thread::sleep(Duration::from_millis(30));
        Self::ctl_data(&dev, table, Duration::from_millis(5))?;
        std::thread::sleep(Duration::from_millis(30));
        Self::ctl_exchange(&dev, &Self::control_packet(ctl::COMMIT, 0, 0))?;
        Ok(())
    }

    /// Индекс LED/слота подсветки по HID usage клавиши (страница 0x07).
    /// `None` — usage, которого нет в официальной раскладке.
    fn light_index_for_usage(usage: u16) -> Option<u8> {
        HID_USAGE_TO_LED
            .iter()
            .find(|(u, _)| *u == usage)
            .map(|(_, idx)| *idx)
    }

    /// Собрать 576-байтную таблицу `[index, R, G, B]` из упакованного
    /// readback-а `RGBRGB…` (432 б = 144 слота × 3). Трейлер `AA 55`
    /// записывается в последние два байта, слот пишется только если
    /// `slot*4+4 <= 574` (как в deddroid `protocol::table`).
    fn color_table_from_rgb(rgb: &[u8]) -> Result<Vec<u8>> {
        let need = ctl::TABLE_SLOTS * 3;
        if rgb.len() < need {
            return Err(Error::Protocol(format!(
                "readback таблицы цветов короче ожидаемого: {} < {need} б",
                rgb.len()
            )));
        }
        let mut t = vec![0u8; ctl::TABLE_LEN];
        for slot in 0..ctl::TABLE_SLOTS {
            let off = slot * 4;
            if off + 4 <= ctl::TABLE_LEN - 2 {
                t[off] = slot as u8;
                t[off + 1] = rgb[slot * 3];
                t[off + 2] = rgb[slot * 3 + 1];
                t[off + 3] = rgb[slot * 3 + 2];
            }
        }
        t[ctl::TABLE_LEN - 2] = ctl::DELIM.0;
        t[ctl::TABLE_LEN - 1] = ctl::DELIM.1;
        Ok(t)
    }

    /// Прочитать сохранённую таблицу per-key цветов: команда `0xF5` с count=9,
    /// первый блок приходит в ответе (63 полезных байта), далее 8 блоков по
    /// 64 б; завершается COMMIT (Twyx22 `readCustomRgb`).
    fn read_key_colors(&self, dev: &hidapi::HidDevice) -> Result<Vec<u8>> {
        let need = ctl::TABLE_SLOTS * 3;
        let cmd = Self::control_count(ctl::RGB_READ, 0, ctl::TABLE_BLOCKS as u16);
        let first = Self::ctl_exchange(dev, &cmd)?.ok_or_else(|| {
            Error::Protocol(
                "клавиатура не ответила на чтение таблицы цветов (0xF5); \
                 per-key RMW невозможен"
                    .into(),
            )
        })?;
        let mut raw = Vec::with_capacity(ctl::TABLE_LEN);
        raw.extend_from_slice(&first[1..]);
        for _ in 1..ctl::TABLE_BLOCKS {
            let zero = [0u8; ctl::PACKET_LEN];
            let next = Self::ctl_exchange(dev, &zero)?.ok_or_else(|| {
                Error::Protocol("чтение таблицы цветов оборвалось: нет ответа".into())
            })?;
            raw.extend_from_slice(&next);
        }
        let _ = Self::ctl_exchange(dev, &Self::control_count(ctl::COMMIT, 0, 0));
        if raw.len() < need {
            return Err(Error::Protocol(format!(
                "readback таблицы цветов короткий: {} < {need} б",
                raw.len()
            )));
        }
        raw.truncate(need);
        Ok(raw)
    }

    /// Записать таблицу per-key цветов через `0x23` (144 слота × 4 б).
    fn write_key_colors(&self, dev: &hidapi::HidDevice, table: &[u8]) -> Result<()> {
        self.upload_table(dev, ctl::KEY_COLORS, table, "per-key цвета")
    }

    /// Живой (несохраняемый) per-key кадр: `0x20` count=8 → 512 б данных →
    /// `COMMIT` → чтение статуса без проверки (deddroid `live_frame`).
    /// FINISH здесь НЕ отправляется — прошивка сама возвращается к
    /// сохранённому эффекту, когда кадры перестают приходить.
    fn apply_live_frame(&self, dev: &hidapi::HidDevice, data: &[u8]) -> Result<()> {
        Self::ctl_feature(
            dev,
            &Self::control_count(ctl::LIVE_COLORS, 0, 8),
            Duration::from_millis(5),
        )?;
        Self::ctl_data(dev, data, Duration::from_millis(2))?;
        Self::ctl_feature(
            dev,
            &Self::control_count(ctl::COMMIT, 0, 0),
            Duration::from_millis(5),
        )?;
        std::thread::sleep(Duration::from_millis(5));
        let mut resp = [0u8; ctl::PACKET_LEN + 1];
        let _ = dev.get_feature_report(&mut resp);
        Ok(())
    }

    /// 512-байтный live-кадр из пар `(light index, цвет)`: подряд
    /// `[index, R, G, B]`, нулевое дополнение до 512 (deddroid `live_frame`).
    fn live_frame_from_pairs(pairs: &[(u8, [u8; 3])]) -> Vec<u8> {
        let mut d = Vec::with_capacity(ctl::LIVE_LEN);
        for (i, [r, g, b]) in pairs.iter().take(ctl::LIVE_LEN / 4) {
            d.extend_from_slice(&[*i, *r, *g, *b]);
        }
        d.resize(ctl::LIVE_LEN, 0);
        d
    }

    /// Загрузить payload (256-байтный заголовок + пиксельные кадры LE) на
    /// TFT-интерфейс 0xFF67 чанками по 4096, затем переключить дисплей на
    /// user-содержимое. Общий хвост для `set_screen_image`/`set_screen_animation`.
    fn upload_tft_payload(&self, payload: &[u8]) -> Result<()> {
        if payload.len() < 256 || payload.len() % 2 != 0 {
            return Err(Error::Protocol(
                "TFT payload: ожидается заголовок 256 б + чётные пиксельные данные".into(),
            ));
        }
        // Открываем интерфейс TFT-загрузки (usage page 0xFF67, большой output report).
        let api = hidapi::HidApi::new().map_err(Error::from)?;
        let tft = api.device_list().find(|d| {
            d.vendor_id() == self.bridge.vid
                && d.product_id() == self.bridge.pid
                && d.usage_page() == 0xFF67
        });
        let dev = match tft {
            Some(info) => {
                let path = std::ffi::CString::new(info.path().to_bytes().to_vec())
                    .map_err(|_| Error::Protocol("TFT путь содержит NUL".into()))?;
                api.open_path(&path).map_err(Error::from)?
            }
            None => return Err(Error::Protocol("интерфейс 0xFF67 (TFT) не найден".into())),
        };

        const TFT_REPORT_LEN: usize = 4104;
        const TFT_PAYLOAD_LEN: usize = TFT_REPORT_LEN - HEADER_LEN; // 4096
        let total_chunks = payload.len().div_ceil(TFT_PAYLOAD_LEN).max(1);
        let mut report = vec![0u8; 1 + TFT_REPORT_LEN];
        for i in 0..total_chunks {
            let start = i * TFT_PAYLOAD_LEN;
            let end = (start + TFT_PAYLOAD_LEN).min(payload.len());
            // Специфический per-chunk заголовок TFT: [0xAA, 80, idx_lo, idx_hi, total_lo, total_hi, 0x50, 0x06].
            report[0] = REPORT_ID;
            report[1] = MAGIC_OUTGOING;
            report[2] = cmd::SET_TFT_USER_ANIMATION;
            report[3] = (i & 0xFF) as u8;
            report[4] = ((i >> 8) & 0xFF) as u8;
            report[5] = (total_chunks & 0xFF) as u8;
            report[6] = ((total_chunks >> 8) & 0xFF) as u8;
            report[7] = 0x50;
            report[8] = 0x06;
            report[9..9 + (end - start)].copy_from_slice(&payload[start..end]);
            dev.write(&report).map_err(Error::from)?;
            // Ack-фильтр (wsclx 10.60): прошивка подтверждает каждый загруженный
            // чанк командой 65 (SET_LED_USER_ANIMATION), а НЕ 80. Читаем ответ
            // неблокирующе (короткий таймаут, 2 попытки): если 65 получен —
            // чанк принят; если ответа нет — продолжаем (fire-and-forget),
            // чтобы не вешать синхронную загрузку на живом железе без ack.
            for _ in 0..2 {
                let mut ack = vec![0u8; 1 + TFT_REPORT_LEN];
                let n = match dev.read_timeout(&mut ack, 150) {
                    Ok(n) => n,
                    Err(_) => break,
                };
                // Смещение отчёт id: [0]=report id(0), [1]=0xAA… либо [0]=0x55.
                let cmd_byte = if ack.first() == Some(&MAGIC_INCOMING) {
                    ack[1]
                } else {
                    ack[2]
                };
                if ack.contains(&MAGIC_INCOMING) && cmd_byte == cmd::SET_LED_USER_ANIMATION {
                    break; // чанк подтверждён
                }
                if n == 0 {
                    break;
                }
            }
        }

        // Переключение дисплея на user-содержимое. wsclx: «try index 0, если не
        // показывает — перебрать до ~10». На железе подберём.
        self.set(cmd::SET_TFT_BUILT_IN_INDEX, &[0])?;
        Ok(())
    }
}

/// Стратегия «применить в память» для Ajazz: настройки пишутся во флеш СРАЗУ
/// (save-команда есть и работает, журнал 10.53), поэтому persistent-хуки
/// просто применяют свет и возвращают `true` — host-стрим не нужен.

impl KeyboardDevice for AjazzAk820 {
    fn name(&self) -> &str {
        match self.bridge.connection {
            ConnectionType::Usb => "Ajazz AK820 Pro (USB/2.4G)",
            ConnectionType::Bluetooth => "Ajazz AK820 Pro (Bluetooth)",
            ConnectionType::Wireless2_4G => "Ajazz AK820 Pro (2.4 GHz)",
        }
    }

    fn connection_type(&self) -> ConnectionType {
        self.bridge.connection
    }

    fn is_connected(&self) -> bool {
        // 2.4G-донгл: пинг батареи (order 0x20) — эхо первых 3 байт
        // подтверждает, что транспорт жив (deddroid/ak820-rs `rf_echo`).
        if self.is_wireless() {
            return self.rf_send(rf::BATTERY, 1, 0, &[0]).is_ok();
        }
        // Раньше проверялось через get_device_info() → канал 0xFF68, который на
        // живом железе таймаутит (журнал 10.106/10.112), поэтому всегда было
        // «нет». Проверяем через рабочий control-канал 0xFF13: если он открылся
        // по VID/PID — устройство живо и подключено.
        self.open_control().is_ok()
    }

    fn vid_pid(&self) -> (u16, u16) {
        (self.bridge.vid, self.bridge.pid)
    }

    // --- RGB -----------------------------------------------------------

    fn set_rgb_mode(&self, mode: RgbMode) -> Result<()> {
        self.apply_lighting(AjazzMode::from_our_mode(mode))
    }

    fn set_rgb_color(&self, key: KeyId, color: Color) -> Result<()> {
        if self.is_wireless() {
            // Донгл не релеит payload ответов (эхо пустое) — прочитать текущую
            // per-key таблицу нельзя, а запись поверх чужого содержимого
            // небезопасна (затёрли бы всю карту). Честный отказ.
            return Err(Error::Unsupported(
                "per-key подсветка по 2.4G: таблицу нельзя прочитать (донгл не релеит данные ответов); \
                 используйте одноцветный режим (kbc rgb --mode static --color RRGGBB)",
            ));
        }
        // `KeyId` для Ajazz — HID usage (страница 0x07), а таблица цветов
        // адресуется собственными light-индексами (порядок другой!). Для CLI
        // `--color` этот путь недостижим (ранний выход в apply_rgb_persistent),
        // поэтому RMW цветовой таблицы используется только точечно.
        let idx = Self::light_index_for_usage(key.0).ok_or_else(|| {
            Error::Unsupported(
                "у клавиши нет отдельного LED в таблице цветов Ajazz (HID usage не найден)",
            )
        })?;
        let dev = self.open_control()?;
        // read-modify-write: читаем сохранённую таблицу (432 б упакованного
        // RGB), правим один слот, пишем обратно (576 б `[index, R, G, B]`).
        let rgb = self.read_key_colors(&dev)?;
        let mut table = Self::color_table_from_rgb(&rgb)?;
        let off = idx as usize * 4;
        table[off + 1] = color.r;
        table[off + 2] = color.g;
        table[off + 3] = color.b;
        self.write_key_colors(&dev, &table)?;
        // Сохранённая таблица видна только в Custom-режиме — переключаемся.
        drop(dev);
        self.last_color.set([color.r, color.g, color.b]);
        self.apply_lighting(AjazzMode::Custom)
    }

    fn set_rgb_brightness(&self, level: u8) -> Result<()> {
        let b = (level as u16 * 5 / 255).min(5) as u8;
        self.last_brightness.set(b);
        self.apply_lighting(AjazzMode::from_our_mode(
            crate::types::RgbMode::Custom, // не используется: apply_lighting держит свой режим
        ))?;
        Ok(())
    }

    fn set_rgb_speed(&self, speed: u8) -> Result<()> {
        let s = (speed as u16 * 5 / 255).min(5) as u8;
        self.last_speed.set(s);
        // Скорость — свойство режима; переприменяем последний режим.
        self.apply_current_mode()?;
        Ok(())
    }

    /// Хост-рендеринг: одноцветный кадр → Static (0xFF13), разноцветный →
    /// live-кадр `0x20` (per-key, без сохранения во флеш).
    ///
    /// Раньше per-key был честным `Unsupported`: буфер 0xFF68 (SET_CUSTOM_LED_DATA)
    /// на нашей ревизии не отвечает. По реверсу официального ПО per-key кадры
    /// живут на control-канале 0xFF13 (`live_frame`, deddroid `device.rs`), так
    /// что их можно слать и здесь. НЕ проверено на живом железе (журнал 10.114).
    fn render_frame(&self, colors: &[Color]) -> Result<()> {
        if let Some(uniform) = Self::uniform_frame_color(colors) {
            self.last_color.set([uniform.r, uniform.g, uniform.b]);
            return self.apply_lighting(AjazzMode::Static);
        }
        // Разноцветный кадр: индекс элемента = light-индекс LED (у Ajazz нет
        // отдельной геометрии, led_count не переопределён — порядок задаётся
        // вызывающей стороной).
        let pairs: Vec<(u8, [u8; 3])> = colors
            .iter()
            .take(ctl::LIVE_LEN / 4)
            .enumerate()
            .map(|(i, c)| (i as u8, [c.r, c.g, c.b]))
            .collect();
        let data = Self::live_frame_from_pairs(&pairs);
        let dev = self.open_control()?;
        self.apply_live_frame(&dev, &data)
    }

    fn get_rgb_preset(&self) -> Result<Vec<RgbPreset>> {
        let presets = [
            ("Static", RgbMode::Static),
            ("Breath", RgbMode::Breathing),
            ("Spectrum", RgbMode::SpectrumCycle),
            ("Rolling", RgbMode::Wave),
            ("Ripples", RgbMode::Ripple),
            ("Falling", RgbMode::Rain),
            ("Explode", RgbMode::Reactive),
            ("Custom", RgbMode::Custom),
        ];
        Ok(presets
            .iter()
            .map(|(name, our_mode)| RgbPreset {
                name: (*name).to_string(),
                mode: *our_mode,
                colors: vec![Color::new(255, 255, 255)],
                brightness: 128,
                speed: 60,
            })
            .collect())
    }

    // --- Keymap / Fn ----------------------------------------------------

    fn remap_key(&self, from: KeyId, to: KeyAction) -> Result<()> {
        if self.is_wireless() {
            // Ремап — запись всей таблицы слоя: без чтения текущей раскладки
            // (донгл не релеит payload) запись затёрла бы все слоты дефолтами.
            // Честный отказ до захвата официального ПО.
            return Err(Error::Unsupported(
                "keymap по 2.4G: запись требует чтения текущей раскладки, а донгл не передаёт \
                 данные ответов (эхо только заголовок)",
            ));
        }
        if from.0 as usize >= TOTAL_SLOTS {
            return Err(Error::Protocol("слот keymap вне диапазона 0..128".into()));
        }
        let mut slots = self.get_base_keymap()?;
        if let Some(pos) = slots.iter_mut().find(|(s, _)| *s == from.0 as u8) {
            pos.1 = Self::slot_encode(&to);
        }
        self.set_base_keymap(&slots)
    }

    fn set_fn_layer(&self, layer: FnLayer) -> Result<()> {
        if self.is_wireless() {
            return Err(Error::Unsupported(
                "Fn-слой по 2.4G: запись требует чтения текущего слоя, а донгл не передаёт \
                 данные ответов (эхо только заголовок)",
            ));
        }
        let mut slots = self.get_fn_keymap()?;
        for (key, action) in &layer.bindings {
            if let Some(pos) = slots.iter_mut().find(|(s, _)| *s == key.0 as u8) {
                pos.1 = Self::slot_encode(action);
            }
        }
        self.set_fn_keymap(&slots)
    }

    fn get_current_keymap(&self) -> Result<Keymap> {
        if self.is_wireless() {
            // Эхо донгла не содержит payload → карта клавиш недоступна.
            return Err(Error::Unsupported(
                "чтение keymap по 2.4G недоступно (донгл не релеит данные ответов)",
            ));
        }
        let mut base = Vec::new();
        for (slot, bytes) in self.get_base_keymap()? {
            if let Some(action) = Self::slot_decode(bytes) {
                base.push((KeyId(slot as u16), action));
            }
        }
        let mut fn_layer = FnLayer::default();
        for (slot, bytes) in self.get_fn_keymap()? {
            if let Some(action) = Self::slot_decode(bytes) {
                fn_layer.bindings.push((KeyId(slot as u16), action));
            }
        }
        Ok(Keymap { base, fn_layer })
    }

    /// Текущий аппаратный эффект подсветки (GET_LED_EFFECT, 16-байтный
    /// payload в формате SET_LED_EFFECT; режим — первый байт). wsclx 10.60.
    fn get_current_effect(&self) -> Result<u8> {
        if self.is_wireless() {
            return Err(Error::Unsupported(
                "чтение текущего эффекта по 2.4G недоступно (эхо донгла без payload)",
            ));
        }
        let payload = self.get(cmd::GET_LED_EFFECT, 16)?;
        Ok(payload.first().copied().unwrap_or(0))
    }

    /// Полный заводской сброс (SET_FACTORY_RESET с аргументом 0xFF).
    /// wsclx 10.60: опциональный аргумент 0xFF = полный сброс.
    fn factory_reset(&self) -> Result<()> {
        if self.is_wireless() {
            // По deddroid/ak820-rs команды сброса у ресивера НЕТ (драйвер
            // перезаписывает дефолтный профиль); на клавиатуре — зажать
            // Fn+Space на 3–5 с. Честный отказ.
            return Err(Error::Unsupported(
                "сброс к заводским по 2.4G: команды нет (зажмите Fn+Space на 3–5 с на клавиатуре)",
            ));
        }
        self.set(cmd::SET_FACTORY_RESET, &[0xFF])
    }

    /// Заводская base-раскладка из официального установщика (ассет
    /// `data/ajazz/rgb-keyboard.xml`). Заводской Fn-слой в установщике
    /// отсутствует. Захардкожено: канал `0xFF68` на ревизии 8009 не отвечает
    /// (журнал 10.106), а команд чтения keymap на рабочем `0xFF13` нет
    /// (журнал 10.116).
    fn get_factory_keymap(&self) -> Result<Keymap> {
        Ok(factory_keymap_ak820())
    }

    // --- Screen (Ajazz-only) -------------------------------------------

    /// Мультикадровая анимация на TFT. Кадры от нашего конвертера — RGB565
    /// big-endian; TFT ждёт little-endian — при загрузке пиксели переставляются.
    /// Задержки — в единицах 2 мс (журнал 10.61: kabelklaus/Beattrey/aks075), в
    /// отличие от wsclx (5 мс). На железе подобрать точную единицу.
    fn set_screen_animation(&self, frames: &[ScreenFrame], delays_ms: &[u16]) -> Result<()> {
        const FRAME_BYTES: usize = 128 * 128 * 2;
        const MAX_FRAMES: usize = 30; // wsclx: tftMaxFrames (~30) на одну загрузку
        if frames.is_empty() || frames.len() != delays_ms.len() {
            return Err(Error::Protocol("TFT: кадры и задержки не совпадают по длине".into()));
        }
        if frames.len() > MAX_FRAMES {
            return Err(Error::Protocol(format!(
                "TFT Ajazz: анимация из {} кадров больше лимита {MAX_FRAMES} (децимируйте GIF)",
                frames.len()
            )));
        }
        for f in frames {
            if f.width != 128 || f.height != 128 {
                return Err(Error::Protocol(format!(
                    "TFT Ajazz: ожидается 128×128, получено {}×{}",
                    f.width, f.height
                )));
            }
            if f.data_rgb565.len() != FRAME_BYTES {
                return Err(Error::Protocol(format!(
                    "TFT Ajazz: ожидается {FRAME_BYTES} байт кадра, получено {}",
                    f.data_rgb565.len()
                )));
            }
        }

        // 256-байтный заголовок анимации:
        //   [0] = N (число кадров)
        //   [1..1+N) = задержки каждого кадра, единица 2 мс (clamp 0..=255)
        //   [1+N] = 0x00 terminator
        //   [2+N..256] = 0xFF pad
        let n = frames.len();
        let mut payload = vec![0u8; 256 + n * FRAME_BYTES];
        payload[0] = n as u8;
        for (i, d) in delays_ms.iter().enumerate() {
            payload[1 + i] = (d / 2).min(255) as u8;
        }
        payload[1 + n] = 0x00;
        for i in (2 + n)..256 {
            payload[i] = 0xFF;
        }
        // Кадры: big-endian → little-endian.
        for (fi, f) in frames.iter().enumerate() {
            let base = 256 + fi * FRAME_BYTES;
            for (dst, src) in payload[base..base + FRAME_BYTES]
                .chunks_exact_mut(2)
                .zip(f.data_rgb565.chunks_exact(2))
            {
                dst[0] = src[1];
                dst[1] = src[0];
            }
        }

        self.upload_tft_payload(&payload)
    }

    /// Один кадр на TFT — частный случай анимации из одного кадра.
    fn set_screen_image(&self, frame: ScreenFrame) -> Result<()> {
        if self.is_wireless() {
            // У донгла нет интерфейсов 0xFF67/0xFF68 (TFT) — экран по 2.4G
            // недоступен (aklink docs/DONGLE.md: iface 0..4 = kb/mouse/consumer/
            // 0xFF60/0xFF59).
            return Err(Error::Unsupported(
                "TFT-экран по 2.4G недоступен (у донгла нет канала 0xFF67/0xFF68)",
            ));
        }
        self.set_screen_animation(&[frame], &[0])
    }

    fn set_screen_brightness(&self, _level: u8) -> Result<()> {
        Err(Error::Unsupported(
            "яркость TFT-экрана Ajazz: отдельная команда не декодирована в wsclx",
        ))
    }

    /// Синхронизация времени: локальное время ПК на экран клавиатуры.
    /// Control-протокол (gohv protocol.rs + TaxMachine ak820pro.cpp, журнал 10.60):
    ///   START (0x18, b8=1) → TIME-config (0x28, b8=1) → data(!report id 0x00!,
    ///   [0]=0x00,[1]=0x01,[2]=0x5A,[3]=YY,[4]=MM,[5]=DD,[6]=hh,[7]=mm,[8]=ss,
    ///   [9]=0x00,[10]=0x04, trailer AA 55) → SAVE (0x02, b8=0).
    fn sync_system_time(&self) -> Result<()> {
        use chrono::Local;
        let now = Local::now().naive_local();
        let (y, mo, d, h, mi, s) = (
            now.year() % 100,
            now.month() as u8,
            now.day() as u8,
            now.hour() as u8,
            now.minute() as u8,
            now.second() as u8,
        );

        // 2.4G-донгл: кадр ресивера order 0x0C (CLOCK), 16 б данных —
        // как проводной пакет часов, но weekday в [10] (Sunday = 0)
        // (deddroid/ak820-rs `rf_clock_data`).
        if self.is_wireless() {
            let weekday = now.weekday().num_days_from_sunday() as u8;
            let mut data = [0u8; 16];
            data[1] = 0x01;
            data[2] = 0x5A;
            data[3] = y as u8;
            data[4] = mo;
            data[5] = d;
            data[6] = h;
            data[7] = mi;
            data[8] = s;
            data[10] = weekday;
            data[14] = 0xAA;
            data[15] = 0x55;
            return match self.rf_send(rf::CLOCK, 16, 0, &data)? {
                Some(_) => Ok(()),
                None => Err(Error::Protocol(
                    "ресивер 2.4G не ответил: клавиатура спит на линке — \
                     нажмите клавишу и повторите"
                        .into(),
                )),
            };
        }

        let dev = self.open_control()?;
        self.send_ctl(&dev, &Self::control_packet(ctl::START, 0x00, 0x01))?;
        self.send_ctl(&dev, &Self::control_packet(ctl::TIME, 0x00, 0x01))?;

        // data-пакет: логический первый байт 0x00 (НЕ 0x04).
        let mut data = [0u8; ctl::PACKET_LEN];
        data[1] = 0x01;
        data[2] = 0x5A;
        data[3] = y as u8;
        data[4] = mo;
        data[5] = d;
        data[6] = h;
        data[7] = mi;
        data[8] = s;
        data[9] = 0x00;
        data[10] = 0x04;
        data[62] = ctl::DELIM.0;
        data[63] = ctl::DELIM.1;
        // send_ctl навешивает ведущий report id 0x00 (framing B, журнал 10.112);
        // pkt[0]=0x00 → хендшейк не делается.
        self.send_ctl(&dev, &data)?;

        self.send_ctl(&dev, &Self::control_packet(ctl::SAVE, 0x00, 0x00))?;
        Ok(())
    }

    // --- Encoder ---------------------------------------------------------

    fn set_encoder_action(&self, _action: EncoderAction) -> Result<()> {
        Err(Error::Unsupported(
            "энкодер AK820 Pro firmware-fixed (Volume/Mute), ремап невозможен (журнал 10.53)",
        ))
    }

    // --- Macros ----------------------------------------------------------

    fn record_macro(&self, _macro_id: u8) -> Result<()> {
        Err(Error::Unsupported(
            "запись макроса на устройстве (device-side) у AK820 Pro не предусмотрена протоколом",
        ))
    }

    fn stop_macro(&self) -> Result<()> {
        Err(Error::Unsupported(
            "запись макроса на устройстве у AK820 Pro не предусмотрена",
        ))
    }

    fn set_macro(&self, id: u8, actions: Vec<KeyAction>) -> Result<()> {
        if self.is_wireless() {
            return Err(Error::Unsupported(
                "макросы по 2.4G: донгл не релеит данные ответов и не поддерживает \
                 загрузку таблицы макросов",
            ));
        }
        if id as usize >= Self::MACRO_SLOTS {
            return Err(Error::Protocol("макрос-слот вне диапазона 0..99".into()));
        }
        // Официальный драйвер ограничивает запись одного макроса 320 байтами
        // (8-байтный заголовок + события по 4 байта с учётом press/release).
        if Self::actions_from_keys(&actions).len() * 4 + 8 > 320 {
            return Err(Error::Protocol("макрос превышает лимит 320 байт".into()));
        }
        // Прошивка не отдаёт таблицу макросов на чтение, поэтому храним уже
        // записанные слоты в кэше и пересобираем ВСЮ таблицу при каждой записи —
        // иначе запись одного макроса стёрла бы остальные 99.
        let mut cache = self.macro_cache.lock().map_err(|_| {
            Error::Protocol("кэш макросов повреждён (mutex poisoned)".into())
        })?;
        cache[id as usize] = if actions.is_empty() {
            None
        } else {
            Some(actions)
        };
        let table = Self::build_macro_table(&cache);
        self.upload_macro_table(&table)
    }

    fn get_macros(&self) -> Result<Vec<Macro>> {
        // Прошивка НЕ отдаёт макросы на чтение (wsclx-путь 0xFF68 у нас мёртв,
        // а по 0xFF13 команды чтения макросов нет). Возвращаем кэш записанных
        // этим процессом слотов; пустые слоты не включаем. Важно: это НЕ дамп
        // содержимого клавиатуры — после перезапуска кэш пуст.
        let cache = self.macro_cache.lock().map_err(|_| {
            Error::Protocol("кэш макросов повреждён (mutex poisoned)".into())
        })?;
        let mut macros = Vec::new();
        for (id, slot) in cache.iter().enumerate() {
            let Some(actions) = slot else { continue };
            let events = Self::actions_from_keys(actions);
            if events.is_empty() {
                continue;
            }
            macros.push(Macro {
                id: id as u8,
                events: events
                    .into_iter()
                    .map(|(delay, code, family, press)| MacroEvent {
                        delay_ms: delay.max(10),
                        key_code: code,
                        family: match family {
                            MacroFamily::Keyboard => 3,
                            MacroFamily::Mouse => 1,
                        },
                        is_press: press,
                    })
                    .collect(),
            });
        }
        Ok(macros)
    }

    // --- Диагностика (read-only) -----------------------------------------

    /// Read-only дамп всех GET-команд протокола. Формат: `ключ=hex` по строке
    /// на секцию; ошибки отдельных команд — `ключ_error=…` (дамп продолжается).
    /// Ничего в устройство не пишется.
    fn diagnostic_dump(&self) -> Result<String> {
        let mut out = String::new();
        out.push_str("# Ajazz AK820 Pro — read-only dump (все команды GET)\n");
        out.push_str(&format!("name={}\n", self.name()));
        out.push_str(&format!(
            "vid_pid={:04X}:{:04X}\n",
            self.bridge.vid, self.bridge.pid
        ));
        out.push_str(&format!("connection={}\n", self.bridge.connection.label()));

        // 2.4G-донгл: read-only пинги протокола ресивера (order/len/chunk# +
        // checksum). Команд чтения настроек нет; батарея (order 0x20) — живой
        // read-only канал, возвращающий проценты в байте [3] эха.
        if self.is_wireless() {
            out.push_str(
                "transport=0xFF60 (2.4G донгл, кадр [order,len,chunk#,data≤28,checksum])\n",
            );
            out.push_str("read_only=да (команд чтения настроек нет; только батарея)\n");
            match self.rf_send(rf::BATTERY, 1, 0, &[0]) {
                Ok(Some(echo)) => {
                    out.push_str(&format!("battery_ping={}\n", hidshell::hex_dump(&echo)));
                    out.push_str(&format!(
                        "battery={}\n",
                        echo.get(3).copied().unwrap_or(0)
                    ));
                }
                Ok(None) => out.push_str("battery_ping_error=нет эха (клавиатура спит на линке)\n"),
                Err(e) => out.push_str(&format!("battery_ping_error={e}\n")),
            }
            return Ok(out);
        }

        // device-info (48 байт; [17] — заряд).
        match self.get(cmd::GET_DEVICE_INFO, 48) {
            Ok(b) => {
                out.push_str(&format!("device_info={}\n", hidshell::hex_dump(&b)));
                out.push_str(&format!(
                    "battery={}\n",
                    b.get(17).copied().unwrap_or(0)
                ));
            }
            Err(e) => out.push_str(&format!("device_info_error={e}\n")),
        }

        // Текущий глобальный LED-эффект (16 байт, [0] — режим).
        match self.get(cmd::GET_LED_EFFECT, 16) {
            Ok(b) => {
                out.push_str(&format!("led_effect={}\n", hidshell::hex_dump(&b)));
                out.push_str(&format!(
                    "led_effect_mode=0x{:02X}\n",
                    b.first().copied().unwrap_or(0)
                ));
            }
            Err(e) => out.push_str(&format!("led_effect_error={e}\n")),
        }

        // game-mode-блок (56 байт, задел wsclx).
        match self.get(cmd::GET_GAME_MODE, 56) {
            Ok(b) => out.push_str(&format!("game_mode={}\n", hidshell::hex_dump(&b))),
            Err(e) => out.push_str(&format!("game_mode_error={e}\n")),
        }

        // custom-LED буфер (512 байт = 128 × RGBA).
        match self.get_many(cmd::GET_CUSTOM_LED_DATA, LED_COUNT * 4) {
            Ok(b) => out.push_str(&format!("custom_led={}\n", hidshell::hex_dump(&b))),
            Err(e) => out.push_str(&format!("custom_led_error={e}\n")),
        }

        // keymap: текущий Default/FN1 + заводские матрицы (все read-only).
        for (label, cmd_byte) in [
            ("keymap_base", cmd::GET_KEY),
            ("keymap_fn", cmd::GET_FN_KEY),
            ("keymap_default_base", cmd::GET_DEFAULT_KEY_MATRIX),
            ("keymap_default_fn", cmd::GET_DEFAULT_FN_KEY_MATRIX),
        ] {
            match self.get_many(cmd_byte, KEYMAP_BYTES) {
                Ok(b) => out.push_str(&format!("{label}={}\n", hidshell::hex_dump(&b))),
                Err(e) => out.push_str(&format!("{label}_error={e}\n")),
            }
        }

        // Макросы: прошивка не отдаёт их на чтение (0xFF68 у нас мёртв, чтения
        // по 0xFF13 нет). Показываем только кэш текущего процесса.
        match self.get_macros() {
            Ok(ms) => {
                out.push_str("macros_source=кэш текущего процесса (прошивка не читается)\n");
                out.push_str(&format!("macros_count={}\n", ms.len()));
                for m in &ms {
                    let ev: Vec<String> = m
                        .events
                        .iter()
                        .map(|e| {
                            format!(
                                "{}:{}:{}:{}",
                                e.delay_ms,
                                e.key_code,
                                e.family,
                                if e.is_press { 1 } else { 0 }
                            )
                        })
                        .collect();
                    out.push_str(&format!("macro_{}={}\n", m.id, ev.join(",")));
                }
            }
            Err(e) => out.push_str(&format!("macros_error={e}\n")),
        }

        Ok(out)
    }

    // --- System ----------------------------------------------------------

    fn set_polling_rate(&self, _rate: u16) -> Result<()> {
        Err(Error::Unsupported(
            "polling rate: кодировка report_rate в wsclx не декодирована",
        ))
    }

    fn set_debounce(&self, _ms: u16) -> Result<()> {
        Err(Error::Unsupported(
            "debounce: единицы key_delay в wsclx не декодированы",
        ))
    }

    /// Таймер сна: 4 ступени (gohv SleepTime): 0=never, <=60=1m, <=300=5m, иначе=30m.
    /// Control-протокол (журнал 10.60): START (0x18, b8=1) → SLEEP-config
    /// (0x17, byte2=0x01, b8=1) → data(!report id 0x00!, [8]=ступень, trailer AA 55).
    /// FINISH (0xF0) по gohv для сна НЕ нужен.
    fn set_sleep_timer(&self, seconds: u16) -> Result<()> {
        if self.is_wireless() {
            // Order 0x07 (SYSTEM) отправлялся по ресёрчу deddroid, но на живой
            // клавиатуре владельца (2026-11-16) таймер НЕ применяется — эхо
            // приходит, эффекта нет. Честный отказ: сон работает только по USB
            // (0xFF13, подтверждено 10.112). Не перебираем варианты вслепую
            // (политика «НЕ ГАДАТЬ»).
            return Err(Error::Unsupported(
                "таймер сна по 2.4G: команда (order 0x07) на этой ревизии не применяется \
                 (проверено на железе 2026-11-16); сон работает при подключении по USB",
            ));
        }
        let step = if seconds == 0 {
            0
        } else if seconds <= 60 {
            1
        } else if seconds <= 300 {
            2
        } else {
            3
        };

        let dev = self.open_control()?;
        self.send_ctl(&dev, &Self::control_packet(ctl::START, 0x00, 0x01))?;
        self.send_ctl(&dev, &Self::control_packet(ctl::SLEEP, 0x01, 0x01))?;

        // data-пакет: логический первый байт 0x00, ступень сна на байте 8,
        // трейлер AA 55. send_ctl навешивает ведущий report id 0x00 (framing B).
        let mut data = [0u8; ctl::PACKET_LEN];
        data[8] = step;
        data[62] = ctl::DELIM.0;
        data[63] = ctl::DELIM.1;
        self.send_ctl(&dev, &data)?;

        Ok(())
    }

    fn get_battery_level(&self) -> Result<Option<u8>> {
        // 2.4G-донгл: кадр order 0x20 (BATTERY), len 1, data [0]; ресивер
        // отвечает эхом первых 3 байт + проценты в байте [3]
        // (deddroid/ak820-rs `battery`; 0 = неизвестно, 1..=100 — проценты).
        if self.is_wireless() {
            return match self.rf_send(rf::BATTERY, 1, 0, &[0])? {
                Some(echo) => Ok(echo.get(3).copied().filter(|v| (1..=100).contains(v))),
                None => Ok(None), // спит на линке
            };
        }
        // Заряд читается по control-каналу 0xFF13 (Twyx22 `Device::battery`:
        // пакет `[1]=0x20 [2]=0x01` в 65-байтном буфере, ответ `0x20 0x01`,
        // уровень в 4-м байте, 0xFF = «на проводе»/100%, <=100 — проценты).
        if let Ok(dev) = self.open_control() {
            let mut pkt = [0u8; ctl::PACKET_LEN];
            pkt[0] = 0x20;
            pkt[1] = 0x01;
            if let Ok(Some(r)) = Self::ctl_exchange(&dev, &pkt) {
                // r — логический пакет без report id: r[0]=0x20, r[1]=0x01.
                if r[0] == 0x20 && r[1] == 0x01 {
                    let level = r[3];
                    if level == 0xFF {
                        return Ok(Some(100));
                    }
                    if level <= 100 {
                        return Ok(Some(level));
                    }
                }
            }
        }
        // Фоллбек на device_info[17] через 0xFF68 (на нашем железе не
        // отвечает, журнал 10.106/10.112); при недоступности — честный None.
        match self.get_device_info() {
            Ok(info) => Ok(Some(info[17])),
            Err(_) => Ok(None),
        }
    }

    // --- EEPROM ----------------------------------------------------------

    fn save_profile_to_eeprom(&self, _profile_id: u8) -> Result<()> {
        Err(Error::Unsupported(
            "профили Ajazz: настройки пишутся во флеш сразу, отдельных save-слотов нет",
        ))
    }

    fn load_profile_from_eeprom(&self, _profile_id: u8) -> Result<()> {
        Err(Error::Unsupported(
            "профили Ajazz: отдельной загрузки слота нет",
        ))
    }

    // --- Персистентное применение ---------------------------------------

    fn apply_rgb_persistent(
        &self,
        mode: RgbMode,
        color: Option<Color>,
        brightness: u8,
        speed: u8,
    ) -> Result<bool> {
        // Ajazz пишет настройки подсветки во флеш сразу (save есть) — persistent.
        if let Some(c) = color {
            self.last_color.set([c.r, c.g, c.b]);
        }
        self.last_brightness
            .set((brightness as u16 * 5 / 255).min(5) as u8);
        self.last_speed.set((speed as u16 * 5 / 255).min(5) as u8);

        // Custom (per-key): заливаем ВСЮ таблицу цветов одним цветом через
        // подтверждённый control-канал 0xFF13 (`0x23`, 144 слота), затем
        // включаем режим Custom. Без цвета — только переключаем режим.
        // Прежний путь через 0xFF68 (SET_CUSTOM_LED_DATA) на нашей ревизии
        // таймаутил (журнал 10.106/10.112).
        if mode == RgbMode::Custom {
            if let Some(c) = color {
                let rgb: Vec<u8> = std::iter::repeat([c.r, c.g, c.b])
                    .take(ctl::TABLE_SLOTS)
                    .flatten()
                    .collect();
                let table = Self::color_table_from_rgb(&rgb)?;
                let dev = self.open_control()?;
                self.write_key_colors(&dev, &table)?;
            }
            return self.apply_lighting(AjazzMode::Custom).map(|()| true);
        }

        self.apply_lighting(AjazzMode::from_our_mode(mode))?;
        Ok(true)
    }

    fn apply_rgb_persistent_code(
        &self,
        hw_effect: u8,
        color: Option<Color>,
        brightness: u8,
        speed: u8,
    ) -> Result<bool> {
        // Коды hw-эффектов (0x00..0x13) поддерживает только 2.4G-донгл:
        // на USB кодировка эффектов своя (0xFF13-транзакция, mapping из
        // RgbMode), а не wsclx-код.
        if !self.is_wireless() {
            return Ok(false);
        }
        let Some(mode) = AjazzMode::from_code(hw_effect) else {
            return Err(Error::Protocol(format!(
                "неизвестный код эффекта Ajazz: 0x{hw_effect:02X} (диапазон 0x00..0x13)"
            )));
        };
        if let Some(c) = color {
            self.last_color.set([c.r, c.g, c.b]);
        }
        self.last_brightness
            .set((brightness as u16 * 5 / 255).min(5) as u8);
        self.last_speed.set((speed as u16 * 5 / 255).min(5) as u8);
        self.apply_lighting_wl(mode)?;
        Ok(true)
    }

    fn apply_light_persistent(&self, on: bool) -> Result<bool> {
        let mode = if on { AjazzMode::Static } else { AjazzMode::Off };
        self.apply_lighting(mode)?;
        Ok(true)
    }
}

// Применение последнего аппаратного режима из last_mode (для CLI/GUI).
impl AjazzAk820 {
    /// Цвет одноцветного кадра: `Some(c)` — все цвета совпадают (пустой кадр =
    /// чёрный = выключить), `None` — кадр разноцветный (нужен per-key live-кадр).
    fn uniform_frame_color(colors: &[Color]) -> Option<Color> {
        match colors.first() {
            None => Some(Color::new(0, 0, 0)),
            Some(&first) if colors.iter().all(|&c| c == first) => Some(first),
            Some(_) => None,
        }
    }

    fn apply_current_mode(&self) -> Result<()> {
        let mode_byte = self.last_mode.get();
        let mode = if mode_byte >= 0x80 {
            AjazzMode::Custom
        } else {
            // Любой байт из диапазона 0..0x13 становится вариантом.
            match mode_byte {
                0x00 => AjazzMode::Off,
                0x01 => AjazzMode::Static,
                0x02 => AjazzMode::SingleOn,
                0x03 => AjazzMode::SingleOff,
                0x04 => AjazzMode::Glittering,
                0x05 => AjazzMode::Falling,
                0x06 => AjazzMode::Colourful,
                0x07 => AjazzMode::Breath,
                0x08 => AjazzMode::Spectrum,
                0x09 => AjazzMode::Outward,
                0x0A => AjazzMode::Scrolling,
                0x0B => AjazzMode::Rolling,
                0x0C => AjazzMode::Rotating,
                0x0D => AjazzMode::Explode,
                0x0E => AjazzMode::Launch,
                0x0F => AjazzMode::Ripples,
                0x10 => AjazzMode::Flowing,
                0x11 => AjazzMode::Pulsating,
                0x12 => AjazzMode::Tilt,
                _ => AjazzMode::Shuttle,
            }
        };
        self.apply_lighting(mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{KeyAction, KeyId};

    #[test]
    fn slot_encode_decode_roundtrip() {
        let cases = [
            (KeyAction::Press(0x2C), [2, 0, 0x2C, 0]),
            (KeyAction::Media(0xE2), [3, 0xE2, 0, 0]),
            (KeyAction::FnLayer(1), [10, 1, 0, 0]),
            (KeyAction::Macro(7), [6, 7, 0, 0]),
        ];
        for (action, want) in cases {
            let encoded = AjazzAk820::slot_encode(&action);
            assert_eq!(encoded, want, "кодирование {action:?}");
            let decoded = AjazzAk820::slot_decode(encoded).unwrap();
            assert_eq!(decoded, action, "декодирование {action:?}");
        }
    }

    #[test]
    fn slot_encode_modifier_masks_but_decode_keeps_press() {
        // Модификатор кодируется как page 2 + 0xE0+m (как Press-слот), поэтому
        // обратного mapping в decode нет: [2,0,0xE1,0] → Press(0xE1).
        let encoded = AjazzAk820::slot_encode(&KeyAction::Modifier(1));
        assert_eq!(encoded, [2, 0, 0xE1, 0]);
        assert_eq!(AjazzAk820::slot_decode(encoded).unwrap(), KeyAction::Press(0xE1));
        let out_of_range = AjazzAk820::slot_encode(&KeyAction::Modifier(9));
        assert_eq!(out_of_range, [2, 0, 0xE1, 0], "9 & 0x07 = 1 → 0xE1");
    }

    #[test]
    fn slot_decode_unknown_page_is_none() {
        assert_eq!(AjazzAk820::slot_decode([0, 0, 0, 0]), None, "page 0 — default");
        assert_eq!(AjazzAk820::slot_decode([0xFF, 0, 0, 0]), None, "незнакомая страница");
    }

    #[test]
    fn actions_from_keys_maps_each_key_to_press_and_release() {
        let out = AjazzAk820::actions_from_keys(&[
            KeyAction::Press(0x2C),
            KeyAction::Media(0xE2),
            KeyAction::Modifier(1),
        ]);
        // Каждый ввод → пара press/release (push). Wire-флаги Keyboard: 0xB0/0x30.
        assert_eq!(out.len(), 6);
        assert_eq!(
            out[0],
            (0, 0x2C, MacroFamily::Keyboard, true),
            "Press → press"
        );
        assert_eq!(
            out[1],
            (0, 0x2C, MacroFamily::Keyboard, false),
            "Press → release"
        );
        assert_eq!(out[2], (0, 0xE2, MacroFamily::Keyboard, true), "Media код");
        assert_eq!(out[4], (0, 0xE1, MacroFamily::Keyboard, true), "Modifier 1 = 0xE1");
        assert_eq!(MacroFamily::Keyboard.wire_flags(true), 0xB0);
        assert_eq!(MacroFamily::Keyboard.wire_flags(false), 0x30);
    }

    #[test]
    fn build_frame_layout_and_last_flag() {
        // Кадр: [0xAA, cmd, len, addr_lo, addr_hi, 0, last, 0, payload.., пад].
        let payload = [0x11, 0x22, 0x33];
        let pkt = AjazzAk820::build_frame(0x23, 3, 0x0102, &payload, true);
        assert_eq!(pkt[0], MAGIC_OUTGOING);
        assert_eq!(pkt[1], 0x23, "cmd");
        assert_eq!(pkt[2], 3, "len_or_type");
        assert_eq!(pkt[3], 0x02, "addr lo");
        assert_eq!(pkt[4], 0x01, "addr hi");
        assert_eq!(pkt[6], 1, "last-флаг");
        assert_eq!(&pkt[8..8 + payload.len()], &payload, "payload с offset 8");
        let non_last = AjazzAk820::build_frame(0x23, 3, 0x0102, &payload, false);
        assert_eq!(non_last[6], 0, "last-флаг сброшен");
    }

    #[test]
    fn ctl_frame_prepends_report_id_zero() {
        // framing B (журнал 10.112): 65-б транспортный буфер = report id 0x00 +
        // 64-б логический пакет как есть. Ни один логический байт не теряется.
        let pkt = AjazzAk820::control_packet(ctl::START, 0x00, 0x01);
        assert_eq!(pkt[0], ctl::REPORT_ID_CMD, "логический первый байт = 0x04");
        assert_eq!(pkt[1], ctl::START);
        let framed = AjazzAk820::ctl_frame(&pkt);
        assert_eq!(framed.len(), ctl::PACKET_LEN + 1);
        assert_eq!(framed[0], 0x00, "ведущий report id");
        assert_eq!(&framed[1..], &pkt[..], "логический пакет сохранён");

        // mode-data: логический первый байт = код эффекта (0x01 Static).
        let data = AjazzAk820::mode_data_packet(
            AjazzMode::Static,
            [0xFF, 0x00, 0x00],
            false,
            2,
            0,
            0,
        );
        let framed = AjazzAk820::ctl_frame(&data);
        assert_eq!(framed[0], 0x00, "ведущий report id");
        assert_eq!(framed[1], AjazzMode::Static as u8, "код эффекта");
        assert_eq!(&framed[2..5], &[0xFF, 0x00, 0x00], "RGB");
        assert_eq!(framed[15], ctl::DELIM.1, "трейлер 0x55");
        assert_eq!(framed[16], ctl::DELIM.0, "трейлер 0xAA");
    }

    #[test]
    fn uniform_frame_color_detects_per_key() {
        use crate::types::Color;
        // Пустой кадр = выключить свет (чёрный).
        assert_eq!(
            AjazzAk820::uniform_frame_color(&[]),
            Some(Color::new(0, 0, 0))
        );
        // Одноцветный кадр = этот цвет.
        let red = Color::new(255, 0, 0);
        assert_eq!(
            AjazzAk820::uniform_frame_color(&[red, red, red]),
            Some(red)
        );
        // Разноцветный кадр → per-key live-кадр (None).
        let green = Color::new(0, 255, 0);
        assert_eq!(AjazzAk820::uniform_frame_color(&[red, green]), None);
    }

    #[test]
    fn control_count_encodes_le_count() {
        let p = AjazzAk820::control_count(ctl::KEY_COLORS, 0, 9);
        assert_eq!(p[0], ctl::REPORT_ID_CMD);
        assert_eq!(p[1], ctl::KEY_COLORS);
        assert_eq!(p[8], 9);
        assert_eq!(p[9], 0);
        let big = AjazzAk820::control_count(ctl::LIVE_COLORS, 0, 0x0102);
        assert_eq!(&big[8..10], &[0x02, 0x01], "count little-endian");
    }

    #[test]
    fn sum16_wraps_and_matches_firmware_expectation() {
        // Пустая таблица и таблица из нулей — сумма 0.
        assert_eq!(AjazzAk820::sum16(&[]), 0);
        assert_eq!(AjazzAk820::sum16(&[0u8; 64]), 0);
        // 255*257 = 65535 = u16::MAX — ровно на границе, без переполнения.
        assert_eq!(AjazzAk820::sum16(&[0xFF; 257]), 0xFFFF);
        // 255*258 = 65790 → wrapping даёт 254 (0x00FE), а не панику.
        assert_eq!(AjazzAk820::sum16(&[0xFF; 258]), 0x00FE);
        assert_eq!(AjazzAk820::sum16(&[1, 2, 3]), 6);
    }

    #[test]
    fn color_table_from_rgb_packs_slots_and_trailer() {
        // 432 б упакованного RGB → 576 б `[index, R, G, B]` + трейлер AA 55.
        let mut rgb = vec![0u8; ctl::TABLE_SLOTS * 3];
        rgb[0..3].copy_from_slice(&[0x11, 0x22, 0x33]); // слот 0
        rgb[3..6].copy_from_slice(&[0x44, 0x55, 0x66]); // слот 1
        let table = AjazzAk820::color_table_from_rgb(&rgb).unwrap();
        assert_eq!(table.len(), ctl::TABLE_LEN);
        assert_eq!(&table[0..4], &[0, 0x11, 0x22, 0x33]);
        assert_eq!(&table[4..8], &[1, 0x44, 0x55, 0x66]);
        assert_eq!(&table[ctl::TABLE_LEN - 2..], &[0xAA, 0x55]);
        // Слот 143 (off 572) не влезает до трейлера — остаётся нулевым,
        // а последние два байта занимает трейлер.
        assert_eq!(&table[572..576], &[0, 0, 0xAA, 0x55]);
        // Короткий буфер — ошибка, а не тихая запись мусора.
        assert!(AjazzAk820::color_table_from_rgb(&[0u8; 10]).is_err());
    }

    #[test]
    fn light_index_lookup_uses_official_table() {
        assert_eq!(AjazzAk820::light_index_for_usage(0x29), Some(1), "Esc");
        assert_eq!(AjazzAk820::light_index_for_usage(0x3A), Some(2), "F1");
        assert_eq!(AjazzAk820::light_index_for_usage(0x2C), Some(94), "Space");
        assert_eq!(AjazzAk820::light_index_for_usage(0xAF), Some(96), "Fn");
        // Usage, которого нет в официальной раскладке.
        assert_eq!(AjazzAk820::light_index_for_usage(0xFFFF), None);
    }

    #[test]
    fn factory_keymap_has_all_81_official_keys() {
        let km = factory_keymap_ak820();
        assert_eq!(km.base.len(), 81, "81 клавиша официального лейаута");
        assert!(
            km.fn_layer.bindings.is_empty(),
            "заводской Fn-слой в установщике отсутствует"
        );
        assert!(
            km.base.contains(&(KeyId(96), KeyAction::Press(0xAF))),
            "Fn (вендорский 0xAF)"
        );
        assert!(km.base.contains(&(KeyId(1), KeyAction::Press(0x29))), "Esc");
        assert!(km.base.contains(&(KeyId(94), KeyAction::Press(0x2C))), "Space");
        // KeyId уникальны и лежат в диапазоне слотов keymap (0..128).
        let mut ids: Vec<u16> = km.base.iter().map(|(k, _)| k.0).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 81, "KeyId уникальны");
        assert!(ids.iter().all(|&id| (id as usize) < TOTAL_SLOTS));
    }

    #[test]
    fn live_frame_pads_to_512() {
        let red = [0xFF, 0x00, 0x00];
        let data = AjazzAk820::live_frame_from_pairs(&[(1, red), (5, [0, 0xFF, 0])]);
        assert_eq!(data.len(), ctl::LIVE_LEN);
        assert_eq!(&data[0..4], &[1, 0xFF, 0x00, 0x00]);
        assert_eq!(&data[4..8], &[5, 0x00, 0xFF, 0x00]);
        assert!(data[8..].iter().all(|&b| b == 0), "хвост нулевой");
        // Больше 128 пар — лишние отбрасываются, длина всё равно 512.
        let many: Vec<(u8, [u8; 3])> = (0..200).map(|i| (i as u8, red)).collect();
        assert_eq!(AjazzAk820::live_frame_from_pairs(&many).len(), ctl::LIVE_LEN);
    }

    #[test]
    fn rf_frame_layout_and_checksum() {
        // Кадр ресивера: [order, len, chunk#, data, checksum] — checksum =
        // сумма [0..30] & 0xFF на [31] (deddroid/ak820-rs `rf_frames`).
        let payload = [0x11, 0x22, 0x33];
        let pkt = AjazzAk820::build_rf_frame(0x05, 16, 2, &payload);
        assert_eq!(pkt.len(), 32);
        assert_eq!(pkt[0], 0x05, "order");
        assert_eq!(pkt[1], 16, "len");
        assert_eq!(pkt[2], 2, "chunk#");
        assert_eq!(&pkt[3..6], &payload, "data с offset 3");
        let sum: u32 = pkt[..31].iter().map(|&b| u32::from(b)).sum();
        assert_eq!(pkt[31], (sum & 0xFF) as u8, "checksum = сумма [0..30]");
        // Данные больше 28 байт режутся, лишнего не пишется.
        let big = [0xAAu8; 40];
        let clipped = AjazzAk820::build_rf_frame(0x10, 28, 0, &big);
        assert_eq!(&clipped[3..31], &[0xAAu8; 28], "чанк = ровно 28 б");
    }

    #[test]
    fn rf_lighting_data_layout() {
        // 16 б: mode, RGB, rainbow[8], brightness[9], speed[10], direction[11],
        // AA 55 на [14..15] (deddroid `rf_short` от проводного пакета).
        let p = AjazzAk820::rf_lighting_data(AjazzMode::Static, [0x10, 0x20, 0x30], false, 4, 3, 0);
        assert_eq!(p[0], AjazzMode::Static as u8);
        assert_eq!(&p[1..4], &[0x10, 0x20, 0x30]);
        assert_eq!(p[8], 0, "rainbow");
        assert_eq!(p[9], 4, "brightness");
        assert_eq!(p[10], 3, "speed");
        assert_eq!(p[11], 0, "direction");
        assert_eq!(p[14], 0xAA, "делимитер AA");
        assert_eq!(p[15], 0x55, "делимитер 55");
        // Яркость/скорость ограничены 0..5.
        let clamped = AjazzAk820::rf_lighting_data(AjazzMode::Static, [0; 3], false, 250, 9, 0);
        assert_eq!(clamped[9], 5, "яркость clamped до 5");
        assert_eq!(clamped[10], 5, "скорость clamped до 5");
        // Off: RGB не заполняется (как deddroid `Lighting::packet`).
        let off = AjazzAk820::rf_lighting_data(AjazzMode::Off, [0xAA; 3], false, 5, 0, 0);
        assert_eq!(&off[1..4], &[0, 0, 0], "Off без RGB");
    }

    #[test]
    fn macro_table_layout() {
        let mut macros: Vec<Option<Vec<KeyAction>>> = vec![None; 100];
        macros[2] = Some(vec![KeyAction::Press(4), KeyAction::Press(5)]);
        macros[7] = Some(vec![KeyAction::Press(6)]);
        let table = AjazzAk820::build_macro_table(&macros);

        // 100×4 index + записи + выравнивание до 64-байтных блоков + AA 55.
        assert_eq!(table.len() % ctl::PACKET_LEN, 0, "таблица кратна 64 б");
        assert_eq!(&table[0..4], &[0xFF; 4], "пустой слот = FF FF FF FF");
        assert_eq!(&table[4..8], &[0xFF; 4], "пустой слот 1 = FF FF FF FF");

        // Слот 2: offset = 400 (0x0190), events*2 = 8 (4 события × 2).
        assert_eq!(&table[2 * 4..2 * 4 + 2], &400u16.to_le_bytes());
        assert_eq!(&table[2 * 4 + 2..2 * 4 + 4], &[0xFF, 0xFF], "upper offset");
        assert_eq!(&table[400..402], &8u16.to_le_bytes(), "events*2");
        assert_eq!(&table[402..408], &[0; 6], "reserved");
        // press(4) + release(4), press(5) + release(5), delay = max(10).
        assert_eq!(&table[408..412], &[10, 0, 4, 0xB0]);
        assert_eq!(&table[412..416], &[10, 0, 4, 0x30]);
        assert_eq!(&table[416..420], &[10, 0, 5, 0xB0]);
        assert_eq!(&table[420..424], &[10, 0, 5, 0x30]);
        // Следующий слот начинается сразу после записи слота 2.
        assert_eq!(&table[7 * 4..7 * 4 + 2], &424u16.to_le_bytes());
        assert_eq!(&table[424..426], &4u16.to_le_bytes(), "events*2 слота 7");

        let len = table.len();
        assert_eq!(
            &table[len - 2..],
            &[ctl::DELIM.0, ctl::DELIM.1],
            "трейлер AA 55"
        );
    }

    #[test]
    fn wl_mode_names_and_codes_cover_full_range() {
        assert_eq!(wl_mode_name(0x00), Some("Off"));
        assert_eq!(wl_mode_name(0x13), Some("Shuttle"));
        assert_eq!(wl_mode_name(0x14), None);
        assert!(AjazzMode::from_code(0x01).is_some());
        assert!(AjazzMode::from_code(0x14).is_none(), "коды >0x13 не существуют");
    }
}