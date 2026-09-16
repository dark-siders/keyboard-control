// --- Ajazz AK820 Pro ---
// Реальная реализация протокола по wsclx/ak820pro-modder (fw 1.07,
// hardware-verified авторами на живом железе; на нашем устройстве пока НЕ
// проверялась — физической клавиатуры нет, журнал 10.53).
//
// Транспорт: 64-байтные output-репорты (REPORT_ID = 0), кадр:
//   [0xAA, cmd, len_or_type, addr_lo, addr_hi, 0, last_flag, 0, payload…]
// Ответ начинается с 0x55, команда в байте 1, данные — с байта 8.
// Большие блоки (keymap 512 Б, custom-LED 512 Б, макросы) идут пачками
// по 56 байт полезной нагрузки с инкрементом адреса (get_many/set_many).

use super::hid::HidBridge;
use super::{Error, KeyboardDevice, Result};
use crate::types::*;
use std::time::{Duration, Instant};

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
    pub const GET_GAME_MODE: u8 = 17;
    pub const GET_KEY: u8 = 18;
    pub const GET_FN_KEY: u8 = 22;
    pub const GET_CUSTOM_LED_DATA: u8 = 20;
    pub const SET_GAME_MODE: u8 = 33;
    pub const SET_KEY: u8 = 34;
    pub const SET_LED_EFFECT: u8 = 35;
    pub const SET_CUSTOM_LED_DATA: u8 = 36;
    pub const SET_MACRO: u8 = 37;
    pub const SET_FN_KEY: u8 = 38;
    pub const SET_TFT_USER_ANIMATION: u8 = 80;
    pub const SET_TFT_BUILT_IN_INDEX: u8 = 81;
}

/// 128-LED матрица (один слот на клавишу keymap'а).
const LED_COUNT: usize = 128;
/// Keymap: 128 слотов × 4 байта.
const TOTAL_SLOTS: usize = 128;
const SLOT_BYTES: usize = 4;
const KEYMAP_BYTES: usize = TOTAL_SLOTS * SLOT_BYTES;

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
        }
    }
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
        let chunk = PAYLOAD_PER_PACKET;
        let num_chunks = content_size.div_ceil(chunk).max(1);
        let mut out = Vec::with_capacity(content_size);
        for i in 0..num_chunks {
            let addr = (i * chunk) as u16;
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

    // --- Подсветка ----------------------------------------------------

    /// 16-байтный payload SET_LED_EFFECT (wsclx lighting::led_effect_payload).
    fn led_effect_payload(
        mode: AjazzMode,
        color: [u8; 3],
        secondary: [u8; 3],
        color_mode: u8,
        effect_mode_type: u8,
        brightness: u8,
        speed: u8,
        direction: u8,
    ) -> [u8; 16] {
        let mut p = [0u8; 16];
        p[0] = mode as u8;
        p[1] = color[0];
        p[2] = color[1];
        p[3] = color[2];
        p[4] = 0xFF; // driverSetting (официальный драйвер)
        p[5] = secondary[0];
        p[6] = secondary[1];
        p[7] = secondary[2];
        p[8] = color_mode;
        p[9] = brightness.min(5);
        p[10] = speed.min(5);
        p[11] = direction;
        p[12] = effect_mode_type;
        p[14] = 0xAA; // checkCodeL
        p[15] = 0x55; // checkCodeH
        p
    }

    fn apply_lighting(&self, mode: AjazzMode) -> Result<()> {
        let payload = Self::led_effect_payload(
            mode,
            self.last_color.get(),
            [0, 0, 0],
            0,
            0,
            self.last_brightness.get(),
            self.last_speed.get(),
            0,
        );
        self.set(cmd::SET_LED_EFFECT, &payload)?;
        self.last_mode.set(mode as u8);
        Ok(())
    }

    /// Чтение текущего custom-LED буфера (512 байт, 128×4).
    fn get_custom_led(&self) -> Result<Vec<u8>> {
        self.get_many(cmd::GET_CUSTOM_LED_DATA, LED_COUNT * 4)
    }

    /// Запись custom-LED буфера (512 байт).
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
        self.get_device_info().is_ok()
    }

    fn vid_pid(&self) -> (u16, u16) {
        (self.bridge.vid, self.bridge.pid)
    }

    // --- RGB -----------------------------------------------------------

    fn set_rgb_mode(&self, mode: RgbMode) -> Result<()> {
        self.apply_lighting(AjazzMode::from_our_mode(mode))
    }

    fn set_rgb_color(&self, key: KeyId, color: Color) -> Result<()> {
        let mut buffer = self.get_custom_led()?;
        let slot = (key.0 as usize) * 4;
        buffer[slot] = slot as u8;
        buffer[slot + 1] = color.r;
        buffer[slot + 2] = color.g;
        buffer[slot + 3] = color.b;
        self.set_custom_led(&buffer)
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

    /// Хост-рендеринг: заливаем custom-LED буфер и включаем Custom-режим.
    fn render_frame(&self, colors: &[Color]) -> Result<()> {
        let mut map = vec![0u8; LED_COUNT * 4];
        for i in 0..LED_COUNT {
            map[i * 4] = i as u8;
            if let Some(c) = colors.get(i) {
                map[i * 4 + 1] = c.r;
                map[i * 4 + 2] = c.g;
                map[i * 4 + 3] = c.b;
            }
        }
        self.set_custom_led(&map)?;
        self.apply_lighting(AjazzMode::Custom)
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
        let mut slots = self.get_fn_keymap()?;
        for (key, action) in &layer.bindings {
            if let Some(pos) = slots.iter_mut().find(|(s, _)| *s == key.0 as u8) {
                pos.1 = Self::slot_encode(action);
            }
        }
        self.set_fn_keymap(&slots)
    }

    fn get_current_keymap(&self) -> Result<Keymap> {
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

    // --- Screen (Ajazz-only) -------------------------------------------

    /// Загрузка кадра/анимации на TFT. Кадры от нашего конвертера — RGB565
    /// big-endian; TFT ждёт little-endian — при загрузке пиксели переставляются.
    fn set_screen_image(&self, frame: ScreenFrame) -> Result<()> {
        // Разрешение жёстко 128×128 (панель NFP085B-10AF), см. wsclx tft.rs.
        if frame.width != 128 || frame.height != 128 {
            return Err(Error::Protocol(format!(
                "TFT Ajazz: ожидается 128×128, получено {}×{}",
                frame.width, frame.height
            )));
        }
        const FRAME_BYTES: usize = 128 * 128 * 2;
        if frame.data_rgb565.len() != FRAME_BYTES {
            return Err(Error::Protocol(format!(
                "TFT Ajazz: ожидается {FRAME_BYTES} байт кадра, получено {}",
                frame.data_rgb565.len()
            )));
        }

        // 256-байтный заголовок (1 кадр → count=1, terminator на индексе 1, дальше 0xFF).
        let mut payload = vec![0u8; 256 + FRAME_BYTES];
        payload[0] = 1;
        payload[1] = 0x00;
        for i in 2..256 {
            payload[i] = 0xFF;
        }
        // Байты big-endian → little-endian.
        for (dst, src) in payload[256..].chunks_exact_mut(2).zip(frame.data_rgb565.chunks_exact(2)) {
            dst[0] = src[1];
            dst[1] = src[0];
        }

        // Открываем интерфейс TFT-загрузки (usage page 0xFF67, большой output report).
        let api = hidapi::HidApi::new().map_err(Error::from)?;
        let tft = api
            .device_list()
            .find(|d| d.vendor_id() == self.bridge.vid && d.product_id() == self.bridge.pid && d.usage_page() == 0xFF67);
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
        }

        // Переключение дисплея на user-содержимое. wsclx: «try index 0, если не
        // показывает — перебрать до ~10». На железе подберём.
        self.set(cmd::SET_TFT_BUILT_IN_INDEX, &[0])?;
        Ok(())
    }

    fn set_screen_brightness(&self, _level: u8) -> Result<()> {
        Err(Error::Unsupported(
            "яркость TFT-экрана Ajazz: отдельная команда не декодирована в wsclx",
        ))
    }

    fn sync_system_time(&self) -> Result<()> {
        Err(Error::Unsupported(
            "sync времени TFT: формат SET_TEMPORARY_COMMAND_DATA (cmd 52) в wsclx не реализован",
        ))
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
        if id as usize >= 100 {
            return Err(Error::Protocol("макрос-слот вне диапазона 0..99".into()));
        }
        // Собираем index + data, как wsclx macros::encode_macros.
        let events = Self::actions_from_keys(&actions);
        if events.is_empty() {
            return Ok(()); // пустой макрос не пишем
        }
        let header = ((events.len() as u16) * 2).to_le_bytes();
        let mut data = Vec::new();
        data.extend_from_slice(&header);
        data.extend_from_slice(&[0, 0]);
        for (delay, code, family, press) in &events {
            data.extend_from_slice(&delay.to_le_bytes());
            data.push(*code);
            data.push(family.wire_flags(*press));
        }
        if data.len() > 320 {
            return Err(Error::Protocol("макрос превышает лимит 320 байт".into()));
        }

        // Двухфазная запись: index (без commit) → data (с commit).
        let mut index = vec![0u8; 100 * 4];
        index[id as usize * 4..id as usize * 4 + 4].copy_from_slice(&400u32.to_le_bytes());
        self.set_many_at(cmd::SET_MACRO, 0, &index, false)?;
        self.set_many_at(cmd::SET_MACRO, 400, &data, true)?;
        Ok(())
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

    fn set_sleep_timer(&self, seconds: u16) -> Result<()> {
        let sleep_time = if seconds == 0 {
            0
        } else if seconds <= 60 {
            1
        } else if seconds <= 300 {
            2
        } else if seconds <= 600 {
            3
        } else if seconds <= 900 {
            4
        } else {
            5
        };
        let mut gm = self.get_game_mode()?;
        gm[3] = sleep_time;
        self.set(cmd::SET_GAME_MODE, &gm)?;
        Ok(())
    }

    fn get_battery_level(&self) -> Result<Option<u8>> {
        let info = self.get_device_info()?;
        Ok(Some(info[17]))
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

        let aj_mode = AjazzMode::from_our_mode(mode);
        if mode == RgbMode::Custom {
            // В Custom-режиме красим весь буфер цветом и включаем Custom.
            if let Some(c) = color {
                let mut map = vec![0u8; LED_COUNT * 4];
                for i in 0..LED_COUNT {
                    map[i * 4] = i as u8;
                    map[i * 4 + 1] = c.r;
                    map[i * 4 + 2] = c.g;
                    map[i * 4 + 3] = c.b;
                }
                self.set_custom_led(&map)?;
            }
        }
        self.apply_lighting(aj_mode)?;
        Ok(true)
    }

    fn apply_light_persistent(&self, on: bool) -> Result<bool> {
        let mode = if on { AjazzMode::Static } else { AjazzMode::Off };
        let payload = Self::led_effect_payload(
            mode,
            self.last_color.get(),
            [0, 0, 0],
            0,
            0,
            self.last_brightness.get(),
            self.last_speed.get(),
            0,
        );
        self.set(cmd::SET_LED_EFFECT, &payload)?;
        self.last_mode.set(mode as u8);
        Ok(true)
    }
}

// Применение последнего аппаратного режима из last_mode (для CLI/GUI).
impl AjazzAk820 {
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
        let payload = Self::led_effect_payload(
            mode,
            self.last_color.get(),
            [0, 0, 0],
            0,
            0,
            self.last_brightness.get(),
            self.last_speed.get(),
            0,
        );
        self.set(cmd::SET_LED_EFFECT, &payload)?;
        self.last_mode.set(mode_byte);
        Ok(())
    }
}