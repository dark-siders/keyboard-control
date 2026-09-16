// --- AULA F99 ---
// Реализация KeyboardDevice для AULA F99 (USB / 2.4G / BT).
//
// F99 — тот же SinoWealth SH68F90, что и F75 (журнал 10.51, KB.ini):
// VID/PID проводного = 0x258A:0x010C, конфиг-регион/эффект-таблица такие же,
// маркер 5A A5, читается/пишется тот же feature-канал 0x06 (520 байт).
// Отличается моделью (Psd=0xA4; по ней build_device и разводит F75/F99 при
// одинаковом PID 0x010C — см. hid.rs `detect_model`, OpenRGB) и числом
// светодиодов: F99 = 113 (96% с NumPad), а не 90 у F75.
//
// Транспорт, host-эффекты и запись в память — перенесены из проверенного
// драйвера F75 (см. aula_f75.rs). ВСЁ, что здесь реализовано через 0x06,
// подтверждено эмпирически на F75 и по KB.ini F99; на физическом F99
// НЕ проверялось (железа у владельца нет).

use super::hid::HidBridge;
use super::{Error, KeyboardDevice, Result};
use crate::types::*;

/// Количество клавиш в 96% раскладке F99.
pub const KEY_COUNT: u16 = 100;

/// Количество светодиодов матрицы подсветки F99.
/// По OpenRGB (SinowealthKeyboard10cController, aula_f99_layout): 96%,
/// LED-индексы до 112 → 113 светодиодов (журнал 10.51/10.54).
pub const RGB_LED_COUNT: usize = 113;

/// Количество колонок в 96% раскладке (для host-эффектов волны).
pub const RGB_COLS: usize = 24;

/// Vendor usage page командного канала F99 (как F75).
const REPORT_FEATURE: u8 = 0x06;

/// Длина feature-репорта канала конфигурации (совпадает с F75).
const PACKET_LEN: usize = 520;

pub struct AulaF99 {
    bridge: HidBridge,
}

impl AulaF99 {
    pub fn open(bridge: HidBridge) -> Self {
        Self { bridge }
    }

    /// Собрать 520-байтовый пакет с одинаковым заголовком (как F75).
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

    /// Записать конфиг-регион (128 байт, маркер 5A A5 обязан присутствовать)
    /// в память клавиатуры. Пакет — 06 04 00 00 01 00 80 00 + data + пад до 520.
    /// После записи клавиатура перезапускается (кик) — это и есть commit
    /// (журнал 10.24/10.25: на F75 readback подтверждает хранение).
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
}

/// Индикаторы F99 (задел; протокол индикаторов не ревёрснут).
#[derive(Debug, Clone, Copy)]
pub enum Indicator {
    Caps,
    Num,
    WinLock,
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

    // --- RGB -----------------------------------------------------------
    // Живые эффекты — host-rendering (latch на проводном F75/F99 не найден,
    // журнал 10.14/10.16): считаем кадр на хосте (`host_frame_with`) и шлём
    // по прямому каналу render_frame(), как у F75 (журнал 10.10).
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

    // ВЫМЫСЕЛ на F75 (не проверено); у F99 НЕ реализуем — честный отказ.
    fn set_rgb_mode(&self, _mode: RgbMode) -> Result<()> {
        Err(Error::Unsupported(
            "аппаратный режим подсветки не реализован (host-эффекты через render_frame)",
        ))
    }

    fn set_rgb_color(&self, _key: KeyId, _color: Color) -> Result<()> {
        Err(Error::Unsupported(
            "поклавишный цвет только через host-render_frame (per-key не ревёрснут)",
        ))
    }

    fn set_rgb_brightness(&self, _level: u8) -> Result<()> {
        Err(Error::Unsupported(
            "аппаратная яркость не реализована (сохраняется в слоты эффект-таблицы)",
        ))
    }

    fn set_rgb_speed(&self, _speed: u8) -> Result<()> {
        Err(Error::Unsupported(
            "аппаратная скорость не реализована (сохраняется в слоты эффект-таблицы)",
        ))
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

    // --- Keymap: канал ремапа не ревёрснут — честный отказ ------------
    fn remap_key(&self, _from: KeyId, _to: KeyAction) -> Result<()> {
        Err(Error::Unsupported("ремап F99 не ревёрснут (протокол keymap неизвестен)"))
    }

    fn set_fn_layer(&self, _layer: FnLayer) -> Result<()> {
        Err(Error::Unsupported("слой Fn F99 не ревёрснут (протокол keymap неизвестен)"))
    }

    fn get_current_keymap(&self) -> Result<Keymap> {
        Err(Error::Unsupported("считывание keymap F99 не ревёрснуто"))
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
    // Ролик громкости/подсветки: команды не ревёрснуты fоr F99.
    fn set_encoder_action(&self, _action: EncoderAction) -> Result<()> {
        Err(Error::Unsupported("ролик F99: протокол энкодера не ревёрснут"))
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
    fn set_polling_rate(&self, _rate: u16) -> Result<()> {
        Err(Error::Unsupported("polling rate F99: cmd-id не ревёрснут"))
    }

    fn set_debounce(&self, _ms: u16) -> Result<()> {
        Err(Error::Unsupported("debounce F99: cmd-id не ревёрснут"))
    }

    fn set_sleep_timer(&self, _seconds: u16) -> Result<()> {
        Err(Error::Unsupported("sleep timer F99: cmd-id не ревёрснут"))
    }

    // Батарея: как у F75 (журнал 10.52) — проводной канал заряда у AULA
    // отсутствует вовсе. 2.4G-канал обрабатывается ресивером 3554:FA09
    // (на нём устройство представлено как AulaF75), поэтому здесь честно None.
    fn get_battery_level(&self) -> Result<Option<u8>> {
        Ok(None)
    }

    // --- EEPROM --------------------------------------------------------
    fn save_profile_to_eeprom(&self, _profile_id: u8) -> Result<()> {
        // Реальная запись конфига в память (как F75, журнал 10.25): свет
        // включается полной яркостью во всех слотах эффект-таблицы.
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

    // Аппаратный эффект прошивки (селектор data[10]): как F75 (журнал 10.26).
    // Записываем код эффекта и включаем свет полной яркостью во всех слотах.
    fn save_hardware_effect(&self, effect: u8) -> Result<()> {
        let snap = self.read_config()?;
        if snap.len() < 136 {
            return Err(Error::Protocol("снимок конфига неполный".into()));
        }
        let mut data = vec![0u8; 128];
        data.copy_from_slice(&snap[8..136]);
        data[10] = effect; // селектор активного эффекта
        for i in (58..98).step_by(2) {
            data[i] = 0x09; // свет включаем полной яркостью для всех эффектов
        }
        self.write_config(&data)?;
        Ok(())
    }

    // Эффект + яркость + скорость ВМЕСТЕ (GUI «Записать в память», 10.50):
    // data[10] + слот эффект-таблицы [58+2*(effect-1)] (a=яркость, старшая
    // тетрада b=скорость, цвет в младшей тетраде сохраняем). Как F75 10.50.
    fn save_effect_with_style(&self, effect: u8, brightness: u8, speed: u8) -> Result<()> {
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
    // эффект-таблицы data[58..96] (шаг 2). 0x09 = вкл, 0x00 = выкл при старте.
    // Как F75 (журнал 10.36): тумблер — слоты эффектов, а не общий байт.
    fn save_hardware_light(&self, on: bool) -> Result<()> {
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

    // --- Персистентные хуки ---------------------------------------------
    // Проводной F99 (как F75) аппаратного latch НЕ имеет — возвращаем
    // Ok(false), чтобы GUI/CLI гоняли host-рендеринг (журнал 10.14/10.16).
    // 2.4G-канал F99 не реализован (ресивер обрабатывается AulaF75), поэтому
    // здесь всегда false.
    fn apply_rgb_persistent(
        &self,
        _mode: RgbMode,
        _color: Option<Color>,
        _brightness: u8,
        _speed: u8,
    ) -> Result<bool> {
        Ok(false)
    }

    fn apply_light_persistent(&self, _on: bool) -> Result<bool> {
        Ok(false)
    }
}

/// Управление индикаторами (специфично для F99).
/// Протокол индикаторов НЕ ревёрснут — честный отказ (на F75 такого канала
/// также нет; вымышленные cmd-id 0x21 из старой заглушки удалены).
pub fn set_indicator(_kbd: &AulaF99, _indicator: Indicator, _enabled: bool) -> Result<()> {
    Err(Error::Unsupported("индикаторы F99: протокол не ревёрснут"))
}