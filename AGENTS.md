# AGENTS.md — Keyboard-Center

> **Язык агента:** ТОЛЬКО русский. Никакого английского в ответах пользователю.
> **Границы:** Работа ТОЛЬКО внутри репозитория `/home/darkside/keyboard-control/`. Ни в коем случае не выходить за пределы проекта.
> **Запрет на установку:** НИКОГДА не использовать `nix profile install`, `nix profile remove`, `nix profile list`, `nix-env -i`, `nix-env -e`, `nix-env -u` или любые другие команды управления профилями/окружениями nix. Софт ставится ТОЛЬКО через конфигурацию (flake, home-manager). Исключение — только по явной команде владельца.
> **Разрешение на изменения:** Любые правки в код вносить ТОЛЬКО после явного разрешения владельца. Сначала уточнять, потом делать.
> **Stable ONLY:** Ничего не ставить и не подтягивать из unstable/nixpkgs-unstable.


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

## 0. Кратко о проекте

- **Название:** Keyboard-Center
- **CLI-бинарник:** `kbc`
- **GUI-бинарник:** `keyboard-center`
- **Автор/владелец:** darkside (Dark_Siders)
- **Язык коммуникации:** русский. Код — Rust, комментарии на русском.
- **Лицензия:** MIT

## 1. Технологии и стек

- **Язык:** Rust (edition 2021+)
- **GUI:** GTK4 + Libadwaita (`gtk4-rs`, `libadwaita-rs`)
- **HID-взаимодействие:** `hidapi` / прямой доступ к `/dev/hidraw`
- **Обработка изображений:** `image` crate
- **Сборка:** Nix Flake через `crane`
- **Целевая ОС:** Linux (фокус на NixOS)

## 2. Поддерживаемые устройства

| Устройство | Layout | Особенности |
|---|---|---|
| Ajazz AK820 Pro | 75% | TFT-экран 0.85", металлический энкодер |
| AULA F99 | 96% | NumPad, индикаторы (ролик/энкодера НЕТ) |
| AULA F75 | 75% | Ролик громкости, **БЕЗ Numpad** (75%: 84 клавиши) |

Все устройства поддерживаются в 3 режимах: USB / 2.4 GHz Receiver / Bluetooth.

> **Что есть физически у владельца (актуально на 2026-09-28):** **AULA F75** (USB, ревизия «BY Tech Gaming Keyboard» PID `0x010C`, подтверждён на ПК). **AULA F99** — проверена на железе пользователя (orflem) **USB + 2.4G работают полностью** (диагностика 2026-09-24). **Ajazz AK820 Pro** — появился у владельца на **Fedora-машине** 2026-09-28 (`0c45:8009`, USB); канал `0xFF13` подтверждён на живьём, визуальный результат CMD_MODE ждёт проверки (10.106). Вся живая разведка/дампы/протокол — на F75, F99 и Ajazz.

### VID/PID (известные)

**Ajazz AK820 Pro (Sonix SN32F299):**
- USB: VID `0x0C45` PID `0x8009` (наш, ревизии также `0x800A` / `0x8099`); ISP `0x0C45:0x7140` — **работает** (10.106)
- BT: VID `0x0C45` PID `0xFEFE` — не тестировалась
- ⚠ Ранее в журнале стояли неверные `0x0328:8207/8208/8209` — это PID из заброшенного ресёрча, НЕ наш чип. Удалены (10.106).
- ⚠ `0x0C45:0xFDFD` «Microdia 2.4G Dongle» — **НЕ клавиатура**, постоянно висит на шине; легко спутать с Ajazz по общему VID (10.106).

**AULA F99:**
- USB: VID `0x258A` PID `0x010C` (ревизия «BY Tech», делит PID с F75; отличается моделью PSD `0xA4`) — **работает**
- Dongle: VID `0x258A` PID `0x004F` / общий ресивер `3554:FA09` — **работает**
- BT: VID `0x258A` PID `0x0050` — не тестировался, закрыт

**AULA F75:**
- USB: VID `0x258A` PID `0x004B`
- Dongle: VID `0x258A` PID `0x004C`
- BT: VID `0x258A` PID `0x004D`
- Ревизия «BY Tech Gaming Keyboard» (USB): VID `0x258A` PID `0x010C` — **реально подтверждён на ПК владельца (2026-09-11)**

> **ВАЖНО:** VID/PID могут отличаться в зависимости от ревизии. Реальные значения нужно определять через `lsusb` / `/sys/class/hidraw/*/device/uevent`.

## 3. Ключевой функционал

### 3.1 Модульная архитектура драйверов
- Rust trait `KeyboardDevice` для абстракции команд
- Автосканер HIDRAW по VID/PID с Hot-plug

### 3.2 GUI (GTK4 / Libadwaita)
- Интерактивный макет клавиш (75% / 96%)
- Screen Manager (Ajazz AK820 Pro) — предпросмотр GIF/картинки
- RGB & Light Studio — поклавишная настройка, пресеты
- Keymap & Fn-Layer Editor — переназначение клавиш
- System & Power Settings — Polling Rate, Debounce, Sleep Timer

### 3.3 Профили
- Локальные файлы (TOML)
- Запись во встроенную EEPROM

## 4. Структура проекта

```
keyboard-control/
├── AGENTS.md
├── Cargo.toml
├── build.rs
├── flake.nix
├── flake.lock
├── default.nix
├── 99-keyboard-center.rules
├── data/
│   ├── keyboard-center.desktop
│   └── keyboard-center.svg
├── nix/
│   ├── nixos-module.nix
│   └── home-module.nix
└── src/
    ├── main.rs              # CLI точка входа (kbc)
    ├── lib.rs               # Библиотечный корень
    ├── devices/
    │   ├── mod.rs           # trait KeyboardDevice + реестр
    │   ├── hid.rs           # HidBridge поверх hidapi
    │   ├── hidshell.rs      # HID-разведка и raw-доступ (probe/raw)
    │   ├── ajazz_ak820.rs   # Ajazz AK820 Pro
    │   ├── aula_f75.rs      # AULA F75
    │   ├── aula_f99.rs      # AULA F99
    │   └── scanner.rs       # HIDRAW сканер + hot-plug
    ├── gui/
    │   ├── mod.rs
    │   ├── app.rs           # Главное окно AdwApplication
    │   ├── device_panel.rs  # Панель выбора устройства
    │   ├── keyboard_layout.rs # Визуальный макет клавиш
    │   ├── rgb_studio.rs    # RGB & Light Studio
    │   ├── screen_manager.rs # Screen Manager (Ajazz)
    │   ├── keymap_editor.rs # Keymap & Fn-Layer
    │   ├── settings_panel.rs # System & Power
    │   └── tray.rs          # Системный трей (SNI, ksni) — 10.111
    ├── screen/
    │   ├── mod.rs
    │   └── converter.rs     # GIF/PNG → формат экрана
    └── profiles/
        ├── mod.rs
        └── storage.rs       # TOML serde + EEPROM
```

## 5. Модуль `KeyboardDevice`

```rust
pub trait KeyboardDevice {
    fn name(&self) -> &str;
    fn connection_type(&self) -> ConnectionType;
    fn is_connected(&self) -> bool;

    // RGB
    fn set_rgb_mode(&self, mode: RgbMode) -> Result<()>;
    fn set_rgb_color(&self, key: KeyId, color: Color) -> Result<()>;
    fn set_rgb_brightness(&self, level: u8) -> Result<()>;
    fn set_rgb_speed(&self, speed: u8) -> Result<()>;
    fn get_rgb_preset(&self) -> Result<Vec<RgbPreset>>;

    // Keymap
    fn remap_key(&self, from: KeyId, to: KeyAction) -> Result<()>;
    fn set_fn_layer(&self, layer: FnLayer) -> Result<()>;
    fn get_current_keymap(&self) -> Result<Keymap>;

    // Screen (Ajazz AK820 Pro only)
    fn set_screen_image(&self, frame: ScreenFrame) -> Result<()>;
    fn set_screen_brightness(&self, level: u8) -> Result<()>;
    fn sync_system_time(&self) -> Result<()>;

    // Encoder / Knob
    fn set_encoder_action(&self, action: EncoderAction) -> Result<()>;

    // Macros
    fn record_macro(&self, macro_id: u8) -> Result<()>;
    fn stop_macro(&self) -> Result<()>;
    fn set_macro(&self, id: u8, actions: Vec<KeyAction>) -> Result<()>;

    // System
    fn set_polling_rate(&self, rate: u16) -> Result<()>;
    fn set_debounce(&self, ms: u16) -> Result<()>;
    fn set_sleep_timer(&self, seconds: u16) -> Result<()>;
    fn get_battery_level(&self) -> Result<u8>;

    // EEPROM
    fn save_profile_to_eeprom(&self, profile_id: u8) -> Result<()>;
    fn load_profile_from_eeprom(&self, profile_id: u8) -> Result<()>;
}
```

## 6. Зависимости (Cargo.toml)

- `gtk = { package = "gtk4", version = "0.9", features = ["v4_12"] }`
- `adw = { package = "libadwaita", version = "0.7", features = ["v1_4"] }`
- `hidapi = "2.5"`
- `image = "0.25"`
- `serde` + `serde_derive` + `toml`
- `tokio` (async для сканера)
- `clap` (CLI)
- `anyhow` / `thiserror`
- `glib` + `gio`

## 7. Nix-интеграция

- `flake.nix`: craneBuild для Rust +GTK4, прокидка зависимостей
- `nix/nixos-module.nix`: udev-правила, группы, PATH
- `nix/home-module.nix`: установка бинарников, .desktop, иконка
- `99-keyboard-center.rules`: полный набор VID/PID

## 8. Конвенции

- Отступы: 4 пробела (Rust стандарт)
- Комментарии: русские
- Именование: snake_case (Rust), CamelCase для типов
- Ошибки: `anyhow::Result` или кастомные через `thiserror`
- Nix: 2 пробела, комментарии русские

## 9. Запреты

- НЕ устанавливать пакеты через nix profile / nix-env
- НЕ выходить за пределы репозитория без разрешения
- НЕ коммитить без явного запроса
- НЕ использовать unstable-каналы
- НЕ добавлять файлы без спроса


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

# 10. Журнал работ (не забыть!)

> **ОГРАНИЧЕНИЯ ВАЖНО УЧИТЫВАТЬ:** запрещены `nix profile install/remove/list`, любые `nix-env`-операции, окружения и профили nix — софт ставится ТОЛЬКО через конфигурацию (flake, home-manager) и только по явной команде владельца. Только STABLE-каналы (никакого unstable/nixpkgs-unstable). Язык коммуникации с владельцем — русский (в журнале допускаются технические термины). Правки кода — только после явного разрешения владельца; журнал можно обновлять по ходу работ.

> **ПОЛИТИКА «НЕ ГАДАТЬ»:** никаких «выдуманных» cmd-id/protocol-команд. Метод без проверки на железе — либо открытый ресёрч с явным указанием источника (и пометкой «НЕ проверено на нашем железе»), либо честный `Unsupported`. Запись в живую клавиатуру — только в рамках «write-тестов» по скриптам (`f75-usb-rev.sh` / `f99-usb-rev.sh`) с разрешения владельца; маркер `5A A5` никогда не затирать.

### КАК ЧИТАТЬ ЖУРНАЛ

- **10.0 СВОДКА ПРОТОКОЛА** — единственный источник правды по протоколам. Перед любой работой по устройству читать её, а не копаться в сессиях. Обновляется по мере подтверждений на железе.
- **ИНДЕКС СЕССИЙ** — одна строка на сессию (номер — дата — суть). Быстрый поиск по истории. Длинные строки не сокращать: там ссылки на последующие пункты.
- **Блоки `## 10.N …`** — подробности только для тех сессий, где была неочевидная работа (диагностика, разбор ошибок, нестандартные решения). Идут **хронологически**. Обычные сессии («добавил кнопку», «починил опечатку») живут только строкой в индексе — специального блока не заводят.
- **«Грабли» внутри сводки** (раздел C по Ajazz и т.п.) — воспроизведённые ошибки. Прежде чем повторить эксперимент, проверить свой пакет/путь/синтаксис по этому списку.
- Числа в скобках вида `(10.106)` — ссылки на сессию-источник. Новый факт без такой ссылки считается непроверенным.
- Пропуски в нумерации (`10.16`, `10.42a`, `10.97b–d`) — исторические, не восстанавливать.


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

## 10.0 СВОДКА ПРОТОКОЛА (актуально на 2026-09-28)

### Устройства и VID/PID

| Устройство | USB | 2.4G-ресивер | Bluetooth |
|---|---|---|---|
| **AULA F75** | `258A:010C` (ревизия «BY Tech Gaming Keyboard») | `3554:FA09` (общий ресивер F75/F99) | `258A:004D` (не тестировался) |
| **AULA F99** | `258A:010C` (делит PID с F75!); отличается моделью PSD `0xA4` | `3554:FA09` (общий) | `258A:0050` (не тестировался) |
| **Ajazz AK820 Pro** | `0C45:8009` / `0C45:800A` / `0C45:8099` (ревизии); ISP `0C45:7140` | — | `0C45:FEFE` (не тестировался) |

- **F75 и F99 — один PID `258A:010C`**, различаются ТОЛЬКО моделью в PSD: `0xA4` = F99, `0xCD/0xE1/0x91` = F75 (по ревизиям). В `kbc list` имя берётся из реестра по PID (может показывать «AULA F75» при физической F99) — ориентироваться на `kbc info` (модель из PSD).
- F99: конфиг и эффект-таблица как у F75, НО **34 пары слотов эффектов `[58..125]`** (не 20 как у проводного F75), **113 LED / 24 колонки** для обычного host-рендеринга; аудио Bars использует отдельную геометрию 99 известных позиций / 19 колонок, включая NumPad.
- Физически у владельца (2026-09-11+): **AULA F75** (USB + ресивер 2.4G). **AULA F99** — проверена на железе пользователя (orflem) **USB + 2.4G работают полностью** (диагностика 2026-09-24). **Ajazz AK820 Pro** — появился у владельца на Fedora-машине 2026-09-28 (`0c45:8009`, USB); канал `0xFF13` подтверждён на живьём, визуальный результат CMD_MODE ждёт проверки (10.106). Вся живая разведка/дампы/протокол — на F75, F99 и Ajazz.

### Каналы USB (feature-report 0x06, ровно 520 байт)

- **Конфиг-регион:** READ `06 84 00 00 01 00 80 00` + пад до 520 → GET 06 520 → `[8..136]` = 128 б data + `5A A5` маркер на `[126..127]`. WRITE `06 04 00 00 01 00 80 00` + 128 б data + 384 пад. После записи клавиатура «кикается» (пересоздаёт hidraw) — **кик и есть commit**, отдельного latch-пакета НЕ существует (10.85 T1).
- **Direct-канал (host-рендеринг):** `06 08 00 00 01 00 7A 01` + RGB-тройки (3 байта × число LED, F75=90, F99=113) + пад до 520. Требует **постоянного keepalive** (без потока кадров — подсветка гаснет ~1 с). В GUI/CLI тик **40 мс** (20–25 FPS, 10.71). Enable-последовательности НЕТ (0x39/0x3C = Broken pipe, 10.85 T4). При старте ПК — пауза 2 с перед первым кадром против «кривой вёрстки» (10.94; изначально 4 с — уменьшена по желанию владельца 2026-09-24, см. 10.97).
- **Color table (per-key цвета):** READ `06 8A 00 00 01 00 00 02` + пад до 520 → 512 б = 128 записей × 4 б (`RR GG BB XX`). WRITE `06 0A 00 00 01 00 00 02` + 512 б = ровно 520 (без пада!). Читается/пишется на живом F75 (10.85 T2, 10.86). **Self-define селектор НЕ существует** (полный перебор data[10] ∈ 0x01..0x1F негативен, 10.86) — записанная таблица визуально НЕ отображается ни в одном режиме; живой цвет — только через direct. Трейлер региона (`FF FF FF 03` + модель + `5A A5`) не трогать.
- **Keymap (ремап, чтение+запись F75):** READ `06 83 00 <layer 0|1> 01 00 00 02` + пад → 512 б = 128 слотов × 4 б (`page, 0, 0, usage`). WRITE blob `06 03 <layer> 00 01 00 00 FF` + 128 слотов + трейлер `5A A5` на `[518..519]`. Защита: слоты F1–F12 (12,18,24,30,36,42,48,54,60,66,72,78) и Fn (слот 53) залочены прошивкой — запись в них запрещена. Формат слота: Press → `[0x00,0,0,usage]` (page 0 = HID usage page 7), Media → `[0x02,0,0,usage]` (page 2 = consumer 0x0C), Modifier → `[0x00,0,0xE0+m,0]` (0..7 = LCtrl..RWin). **Нюанс:** запись слота в Default меняет и тот же слот в FN1 (прошивка пишет blob в оба слоя, 10.77/10.81). После любого теста ремапа — обязательный readback отката (10.81).

### Каналы 2.4G-ресивера (output-report 0x13, 20 байт)

- Кадр out/in: `[0]=0x13, [1]=cmd, [2]=total, [3]=page_idx, [4]=len(≤14), [5..18]=data, [19]=checksum (сумма [0..18] & 0xFF)` (наш dnviti-транспорт, работает на железе). Ресивер отвечает только когда клавиатура **НЕ спит на линке** (разбудить физическим нажатием клавиши).
- **LED config:** READ `0x44`, WRITE `0x04` (дважды с паузой 300 мс — «flash and revert»). Селектор эффекта = `led[10]`; яркость/скорость — слоты `[58 + 2*(led[10]-1)]` (a = яркость, старшая тетрада b = скорость, младшая = цвет). Свет вкл/выкл — обнуление слотов всех эффектов.
- **RGB table:** READ `0x49`, WRITE `0x09` — кастомные цвета эффектов.
- **Батарея:** READ cmd `0x05` (PSD/identity) → ответ `0x0A`, байт[6] = проценты (0x64 = 100%), байт[7] = 0x01 wireless. Команда `0x0B` ресивером НЕ отвечает — не использовать (10.43).
- **Per-key на ресивере** (CMD 0x02/SUBCMD 0x1C) — форматы cloudglides и dnviti расходятся → `Unsupported` до живого подтверждения (10.76).

### Ajazz AK820 Pro (Sonix SN32F299) — USB `0C45:8009`, 4 HID-интерфейса

> **Живое железо появилось 2026-09-28** — у владельца на Fedora-машине (см. 10.106). До этого вся поддержка Ajazz была построена на ресёрче.

#### A. Канал `0xFF13` (control) — ЕДИНСТВЕННЫЙ подтверждённый на нашем железе

- **Как найти узел:** НЕ хардкодить `/dev/hidrawN` — нумерация сдвигается при каждом переподключении. Искать по `report_descriptor`, в котором есть `06 13 FF` (usage page `0xFF13`, usage `0x0001`):
  ```bash
  for d in /sys/class/hidraw/hidraw*; do
    python3 -c "import sys;print(open(sys.argv[1],'rb').read().hex().upper())" "$d/device/report_descriptor" | grep -q '0613FF' && echo "${d##*/}"
  done
  ```
- **Дескриптор:** `0613ff0901a101150026ff007508954009028102090391020a00ffb102c0` — Usage Page `FF13`, Usage `0001`, Input 64 б / Output 64 б / **Feature 64 б** (unnumbered, без Report ID в дескрипторе, но `buf[0]` в HIDAPI = report ID = `0x00`).
- **Транспорт / ФРЕЙМИНГ (КРИТИЧНО, подтверждено 10.112):** логический пакет — 64 байта; в hidraw/hidapi его надо слать как **65 байт: ведущий `0x00` (report id) + те же 64 байта**. Причина: 0xFF13 — НЕнумерованный feature-report, а `hidapi::send_feature_report` трактует `buf[0]` как НОМЕР report-а. Если послать 64 байта с `buf[0]=0x04`, уйдёт «report id 4» (которого у устройства нет) и прошивка молча игнорирует пакет (transmission при этом без ошибок!). Рабочий вызов: `kbc raw 0C45:8009 --path /dev/hidrawNN feature-send <130 hex = 65 б>`. Проверка `GET_REPORT 0x04` на нашем железе отдаёт `Broken pipe` — хендшейк не нужен (безвреден).
- **Протокол (MemerGamer / `AjazzAk820ProUnofficialWebdriver`, подтверждён на нашем железе 10.112).** Три шага: `RESET` → `CONFIGURE <команда>` → data-пакет, финал `FINISH (0xF0)`.
  - Преамбула: `p[0]=0x04`, `p[1]=<команда>`, `p[8]=0x01` (для sleep ещё `p[2]=0x01`).
  - Команды: `0x18` RESET, `0x13` MODE (подсветка), `0x28` TIME (часы), `0x17` SLEEP, `0x02` SAVE, финал `0xF0` FINISH.
  - Data-пакет подсветки: `p[0]=<эффект 0x00..0x13>`, `p[1..3]=RGB`, `p[8]=rainbow`, `p[9]=яркость 0..5`, `p[10]=скорость 0..5`, `p[11]=direction`, `p[14]=0x55`, `p[15]=0xAA`.
  - **Эффекты `0x00..0x13`:** `00` Off, `01` Static, `02` Single On, `03` Single Off, `04` Glittering, `05` Falling, `06` Colourful, `07` Breath, `08` Spectrum, `09` Outward, `0a` Scrolling, `0b` Rolling, `0c` Rotating, `0d` Explode, `0e` Launch, `0f` Ripples, `10` Flowing, `11` Pulsating, `12` Tilt, `13` Shuttle.
  - **Яркость/скорость 0..5, а НЕ 0..255.** `0x05` — максимум. Ошибка «поставил 0xFF» ведёт к нулю на плате.
  - Data-пакеты времени/sleep: `p[0..1]`, маркер `p[62]=0xAA`, `p[63]=0x55`.
- **СТАТУС 10.106→10.112:** транспорт и фрейминг (65 байт = 00+64) подтверждены; **подсветка работает** через последовательность `04 18 → 04 13 → data-пакет → 04 F0` (framing B). Время подтверждено с десятичным форматом (НЕ BCD). GET_REPORT 0x04 отдаёт Broken pipe (безвредно).

#### B. Канал `0xFF68` (wsclx fw 1.07) — ресёрч, НА НАШЕМ ЖЕЛЕЗЕ НЕ РАБОТАЕТ

- Транспорт: output-репорты, report ID 0, кадр `[0xAA, cmd, len, addr_lo, addr_hi, 0, last, 0, payload…]` (64 б), ответ `0x55`, payload от байта 8. **На живом железе — таймаут на `output-send` (10.106).**
- Причина (вероятная): дескриптор `0668ff0961a1010962150026ff009540750881020963150026ff0096001075089102c0` объявляет **Output Report Count = `96 00` = 4096 байт**, а firmware общается 64-байтными пакетами. Плюс у FF68 **нет Feature report** вообще → `feature-send` даёт EPIPE.
- Команды: GET_DEVICE_INFO=16 (байт[17] = заряд), GET_LED_EFFECT=19, GET_KEY=18, GET_FN_KEY=22, GET_CUSTOM_LED_DATA=20, GET_MACRO=21, GET_DEFAULT_KEY_MATRIX=31, GET_DEFAULT_FN_KEY_MATRIX=28, SET_KEY=34, SET_FN_KEY=38, SET_LED_EFFECT=35 (режимы `0x00`–`0x13` + Custom `0x80`, payload 16 б, checkCode `AA 55` на [14..15]), SET_CUSTOM_LED_DATA=36, SET_MACRO=37, SET_FACTORY_RESET=15, SET_TFT_USER_ANIMATION=80, SET_TFT_BUILT_IN_INDEX=81.
- Макросы: index 400 б (100 слотов × 4 б LE offset) + data-блоки; action 4 б (`delay LE u16 + keyCode + flags`; bit7=press, bits6..4=actionType: 3=keyboard 0xB0/0x30, 1=mouse 0x90/0x10, 2=consumer/text). Лимит 320 б/слот.
- Keymap: слоты 128×4, page: KEYBOARD=2, CONSUMER=3, MACRO=6, TGL=10. Энкодер — firmware-fixed (Volume/Mute).
- TFT-канал `0xFF67`: 128×128 RGB565 LE; заголовок 256 б (`[0]=N кадров, задержки по 1 б (единица 2 мс), terminator 0x00, пад 0xFF`), кадры 32768 б, чанки 4096 б + 8 б chunk-header (magic `0x50 0x06` = 0x0650, критично — баг `0x064F` ломал переключение дисплея). Ack на каждый чанк — cmd 65. `set_screen_not.key` → SET_TFT_BUILT_IN_INDEX [0].

#### C. Грабли Ajazz (НЕ ПОВТОРЯТЬ — все воспроизведены 10.106)

- **`0C45:FDFD` «Microdia 2.4G Dongle» — НЕ клавиатура.** Постоянно висит на той же шине, 5 HID-интерфейсов, все interrupt-IN, ноль Output/Feature, ни `FF13`, ни `FF68`. В usbmon-разборе легко спутать с Ajazz (тот же VID `0c45`).
- **Синтаксис `kbc raw`:** `VID:PID` — **позиционный** аргумент, `--path` — отдельный флаг. `kbc raw 0C45:8009 --path /dev/hidraw13 feature-send <hex>`. Вариант `--device 0C45:8009` не существует.
- **Путь — только `/dev/hidrawN`**, sysfs-путь `/sys/class/hidraw/hidrawN` в `hidapi` не откроется.
- **HEX должен быть ровно 130 символов** (65 байт). 129 → «нечётное число hex-цифр». При копировании в терминал символы теряются — генерировать пайтоном, а не вручную.
- **Команда в `p[1]`, а `p[0]` — это код эффекта.** Ошибка «положил `0x13` в `p[0]`» = отправлен эффект `Shuttle` с цветом `R=0x01` (≈0,4% — не видно).
- **Без преамбулы `0x04 <команда>` канал подсветки не открывается** — data-пакет молча игнорируется.

### Селекторы и регионы конфига (USB F75/F99)

| Поле | Назначение |
|---|---|
| `data[10]` | Селектор аппаратного эффекта (F75/F99 USB): `02`=Дыхание, `03`=Смена цветов, `04`=Реактивная, `05`=Падающие блоки, `06`=Радуга с пульсацией, `07`=Диско, `08`=Радуга пульс., `0A`=Змейка, `0B`=Перелив-волна, `0C`=Эффект 0C (имя не выяснено) |
| `data[58..125]` | Слоты эффектов: пары `(a=яркость, b=(speed<<4)|color)`; активный слот = `58 + 2*((data[10])-1)`; свет выкл/вкл = все `a` = `00`/`09` |
| `data[126..127]` | Маркер `5A A5` — **НЕ ТРОГАТЬ** (затирание = откат к заводскому конфигу, 10.36) |
| led[10] (2.4G) | Селектор эффекта на ресивере (своя карта кодов: 0x01..0x1E; см. `WL_EFFECTS`/`apply_rgb_persistent_code`) |

### Закрыто навсегда / не возвращаться

- **Latch/commit-пакета НЕТ.** Commit = кик/перезапуск клавиатуры после записи (10.85 T1). `06 0a …` = самоперезапуск устройства (не latch, не использовать, 10.14). Дизасм `OemDrv.exe` (VitalyArt/Delphi-resolver `0x5d01d8`) — закрыт: это ДРУГОЕ железо (`0c45:800a`, не наш чип) (10.15/10.17).
- **Self-define селектор color-table не существует** (полный перебор, 10.86).
- **Индикаторы Caps/Num/Scroll** через boot output-report 0x02 не работают (Broken pipe, 10.85 T3) — в код не вносить.
- **Батарея USB — канала НЕТ** вообще (железо не передаёт, 10.52). **Батарея BT — закрыта** решением владельца (10.68). Живой канал батареи — только 2.4G (0x05→0x0A).
- **Enable-последовательность direct** (0x39/0x3C) не существует — не нужна (10.85 T4).
- **Offset эффект-таблицы = 58** (не 64 у cloudglides) — подтверждено на железе (10.85 T5).
- Винные/сторонние профили в nix — запрещены (см. шапку).

### Инфраструктура проекта

- Rust edition 2021; GUI GTK4 + Libadwaita (`adw 0.7`/`gtk4 0.9`); `hidapi 2.5`, `image 0.25`, `serde/toml`, `chrono 0.4` (синк часов Ajazz), `clap`, `anyhow/thiserror`; аудио-реактив — `rustfft 6` + `psimple`/`pulse` (libpulse-*-binding 2). Сборка: flake crane, `nix develop . --command cargo …` (`cargo` напрямую без обёртки не запускается).
- CLI `kbc`: `list / info / probe / ajazz-diag / raw feature-send/feature-get/output-send/input-read / wl-read-pages / rgb [--mode (вкл. audio — USB-only)|--style row|bars|--color|--brightness|--speed|--hold|--hw-effect|--light-on/--light-off] / screen --image / time / battery / save [--effect NN|--brightness|--speed] / remap --list|--key|--to|--media|--modifier|--layer / set-poll <Hz> / set-debounce <ms> / set-sleep <sec> / profile save|load|eeprom-save / monitor / preset save|list|apply|remove / daemon [--preset|--style row|bars|--frame-ms|…]` (встроенные аудио-пресеты `audio-viz` и `cava`, в фоне без GUI).
- GUI `keyboard-center`: Вкладки — Устройства, RGB Studio (host-движок + persistent хуки 2.4G, аудио-реактив фоновым потоком (USB, стили `row`/`bars`), файловые пресеты TOML, сохранить в память, гашение), Screen Manager (TFT Ajazz), Keymap & Fn Layer (клик-редактор F75: Press/Media/Modifier, показ раскладки, сброс), Settings (Status/battery). Потоки — через каналы + `frame_gen` (поколение кадров вместо `SourceId::remove()` — больше никаких паник glib, 10.39); аудио — отдельный `Arc<AtomicBool>` stop-флаг (10.97c). Автосейв при закрытии окна (10.38). **Сворачивание в системный трей (SNI) через ksni** (10.111): крестик прячет окно в трей, реальный выход — из меню трея/header (автосейв); отключается `KEYBOARD_CENTER_NO_TRAY`; без SNI-хоста — прежнее поведение.
- Пресеты TOML: `$XDG_CONFIG_HOME/keyboard-center/presets` (по умолч. `~/.config/keyboard-center/presets`), структура `{version, type, color, brightness, speed, frame_ms, audio}`, где `[audio]` содержит `sensitivity`, `color_low`, `color_high`, `style`; пресет `default` неудаляем; встроенные `audio-viz` (`row`) и `cava` (`bars`) также неудаляемы и доступны любой установленной сборке без внешних файлов.
- Nix-интеграция: `nixos-module.nix` / `home-module.nix` (udev, группы plugdev, systemd-user unit `keyboard-center-daemon`), `99-keyboard-center.rules` → `258a:010c/010b/…`, `3554:fa09`, `0c45:8009/800a/8099/fefe/7140`, `258a:004e/004f/0050`, `0328:8207/8208/8209`. **В правилах обязательно `ATTRS{}`, не `ATTR{}`** — у hidraw нет своего `idVendor` (он в USB-родителе), `ATTR{}` не срабатывает никогда (10.103). На NixOS ручная установка в `/etc/udev/rules.d` невозможна (EROFS) — только через конфиг: `services.udev.packages` (наш модуль) или `services.udev.extraRules`; модуль постоянно подключён в `~/nixosniri` на хосте `darkside_pc` (10.103).
- Fedora: `Makefile` (`make release`, `install/install-user/install-udev/update/uninstall/clean/help/install-deps`), единый `fedora.sh` (интерактивное меню + режимы `install [--user|--system|--udev-only]` / `update [--auto|--user|--system]` (пересборка + переустановка по месту) / `uninstall [--purge|--keep-rules]` / `deps`; флаги `--install-deps`/`--dry-run`; заменил `fedora_install.sh` и `fedora_uninstall.sh`, 10.107), CI-джоб `fedora-build` (dnf-пакеты одной строкой в `.gitlab-ci.yml`, без `\`-переносов — иначе YAML-folding склеивает, 10.58; + `bash -n` для всех скриптов, 10.105).
- Диагностика: `scripts/collect-data.sh` (read-only, `--device f99|ajazz`, `--sg`, `--auto`, результат `~/kbc-diag-ГГГГММДД-ЧЧММСС/{01..37}*.txt` + tar.gz; для F99 `--write-tests` делегирует в `scripts/f99-usb-rev.sh`; для Ajazz добавляет `28-ajazz-diag.txt`), `scripts/ajazz-usb-diag.sh` (Ajazz, read-only, `--device`/`--sg`, архив `~/kbc-diag-ajazz-*.tar.gz`), `docs/collect-data.md`, `docs/f99-guide.md` (для владельцев F99), `docs/ajazz-guide.md` (для владельцев Ajazz). Дамп прошивки F75: `data/firmware/f75_firmware.hex` (61440 б, Intel HEX; read-only, MD5 прошивки `564524ecd3aed509c3bba432f0398aca`).
- **Открытые вопросы / такие задачи:** 2) ремап F99 W-канал (0x03 не работает, read-only, 10.82); 3) Ajazz — канал `0xFF13` подтверждён (транспорт и фрейминг), подсветка/время через control-канал работают (10.112); wsclx `0xFF68` — таймаут, альтернатива (10.106); 4) BT-канал (не тестировался, закрыт по батарее); 5) ~~откат временных правок `~/nixosniri` (10.2) + решение судьбы модуля в системном флейке~~ — **ЗАКРЫТО 10.103**: временных правок не осталось, модуль подключён постоянно (input `keyboard-center` + `nixosModules.default`, хост `darkside_pc`); 6) публикация в GitLab / релиз (по желанию владельца); 7) идеи на будущее (реализуемые, после F99+Ajazz): уведомление о низком заряде 2.4G, D-Bus API (интеграция с Waybar), авто-бэкап/restore перед remap (страховка от инцидентов типа 10.81), расширение host-эффектов (Fire/Matrix/…), горячая смена профилей по окну — по решению владельца. (трей — СДЕЛАНО, см. 10.111; audio-reactive — СДЕЛАНО, см. 10.97–10.97e.)


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

## ИНДЕКС СЕССИЙ (10.x — дата — суть)

10.1 — 2026-09-11 — разведка протокола AULA F75: добавлен `hidshell.rs` (probe/raw), CLI `kbc probe/raw`, PID `258A:010C`, реестр/сканер/правила; обнаружен главный канал команд (vendor feature 0x06, 520 б); проблема доступа `plugdev`/`sg plugdev`.

10.2 — 2026-09-11 — ВРЕМЕННЫЕ правки в `~/nixosniri` (input keyboard-center + `programs.keyboard-center.enable`) — ОБЯЗАТЕЛЬНО ОТКАТИТЬ позже; `nix flake update keyboard-center` обязателен.

10.3 — 2026-09-11 — фикс `kbc raw` (hex как String); живой протокол подтверждён: config read `06 84`→GET 06 520 (128 б + `5A A5`); эталон `/tmp/f75_config_dump.txt`.

10.4 — 2026-09-11 — план следующих шагов: доступ к hidraw6, охота за commit/latch, дамп прошивки (0x88/0x89/…), заполнение драйверов, F75 84 клавиши без нумпада, откат правок, заряд АКБ на всех каналах.

10.5 — 2026-09-11 — попытки дампа прошивки и color table: только чтение конфига надёжно; некоторые комбинации вызывали переподключение клавиатуры; формат адреса прошивки/color table не выяснен.

10.6 — 2026-09-11 — текущий статус: работает read config, эффект-таблица, `5A A5`; не работает прошивка/color table/commit; только F75 физически.

10.7 — 2026-09-11 — прорыв direct-режима: OpenRGB `SinowealthKeyboard10cController` — `06 08 … 7A 01` + RGB, keepalive ~500 мс; Vive клава «умирает» без потока.

10.8 — 2026-09-11 — дамп прошивки через `sinowisp` (SH68F90, ISP канал 0x05/0x06): 61440 б, `/tmp/f75_firmware.hex`; положен в репо `data/firmware/`.

10.9 — 2026-09-11 — вывод по commit: для host-рендеринга commit не нужен (direct + keepalive); 20-байтовый протокол F87 (0x13) — другой канал, не переносить.

10.10 — 2026-09-11 — AulaF75 рабочий: render_frame (90 LED) + read_config + get_rgb_preset (эффект-таблица); HidBridge выбор вендорского интерфейса; CLI `kbc rgb --color/--hold`; GUI живой цвет keepalive.

10.11 — 2026-09-12 — живой визуальный тест: 3 direct-кадра (RGB) видны; запись эффект-таблицы принимается, но визуально применяется только после перезагрузки — latch всё ещё нужен.

10.12 — 2026-09-12 — проблемы: 2.4G-скан не находит клаву в режиме ресивера; BT не тестирован; заряд нигде не показывается.

10.13 — 2026-09-12 — реверс дампа: собственный Python 8051-дизасм (~40 опкодов); константные строки команд в дампе отсутствуют — latch генерируется расчётно; адрес обработчика `A33A`.

10.14 — 2026-09-12 — ЕДИНАЯ СЕКЦИЯ (с 10.17/10.15): «цели диспетчера A5» — артефакт, отменён; A5-полосы = незанятые зоны EEPROM; кандидат `06 0a …` = самоперезапуск (не latch); VitalyArt — ДРУГОЕ железо.

10.15 — 2026-09-12 — ресивер 2.4G read-only: `3554:FA09`; на любой read-адрес — константный 520-б блок (config-канал `06 84` не работает); батарея не читается; дизасм `OemDrv.exe` (Delphi, слот `0x5d01d8`, keepalive 300 мс).

10.17 — 2026-09-12 — итог read-only ресивера (закрыта ветка): resolver-слот `0x5d01d8`, точный 2.4G-кадр read-only недостижим; вернулись к USB — яркость/скорость/эффекты «как Windows».

10.18 — 2026-09-12 — в `aula_f75.rs` вписан host-движок эффектов: `HostEffectParams`/`host_frame` (Static/Дыхание/Волна/Спектр/Риппл/Дождь/Реактивная), чистая детерминированная логика.

10.19 — 2026-09-12 — финальный статус сессии: движок в CLI (`cmd_rgb` переведён на host, выдумки убраны); GUI не тронут; `cargo check` не выполнялся.

10.20 — 2026-09-12 — сборка очищена + GUI привязан: 4 ошибки/1 warning исправлены; apply-кнопка на host-движке; check 0/0, test 2/2, release OK; живой тест волны подтверждён.

10.21 — 2026-09-12 — фикс: повторное «Применить» не обновляло режим/яркость/скорость (таймер стартовал только один раз) — стоп+новый таймер на каждый клик.

10.22 — 2026-09-12 — проценты у яркости/скорости + фикс скорости (алиасинг/нормализация фазы) и яркости в Static (bright_k); GUI метки «%»; дефолт скорости 60.

10.23 — 2026-09-12 — пресеты связаны с режимами (Static→0, Дыхание→1, Волна→2, Спектр→3, Дождь→5) + базовый цвет + применение.

10.24 — 2026-09-12 — эксперимент «сохранить в память»: кнопка была заглушкой (0xF0 — выдумка); аппаратная запись конфига `06 04` работает и держится; host-эффекты в память не пишутся.

10.25 — 2026-09-12 — «Записать в память» = реальная запись конфига (`write_config` с data[18]→данные свет); CLI `kbc save`; GUI статус. (data[18] позже оказался НЕ тумблером — см. 10.28/10.36.)

10.26 — 2026-09-12 — найден селектор аппаратных эффектов **data[10]** (карта 02..0B); `save_hardware_effect`; CLI `--effect`; GUI список эффектов; подтверждено на железе (дыхание держится без приложения).

10.27 — 2026-09-12 — фикс: host-keepalive «съедал» запись в память (останавливать поток перед записью); добавлен `save_hardware_light`.

10.28 — 2026-09-12 — «Выключить свет» через data[76]=00/09 (тогда считалось тумблером) — позже скорректировано (10.36: тумблер = слоты эффектов).

10.29 — 2026-09-12 — GUI-пометка «статика в память не пишется»; ресёрч F99 (SinoWealth, статика не сохраняется) и Ajazz (Sonix, есть save — статика сохраняется).

10.30 — 2026-09-12 — фикс «Выключить свет возвращался через 2 с»: постоянный чёрный keepalive вместо одного кадра.

10.31 — 2026-09-12 — «Записать в память» после гашения не возвращало свет: переоткрытие устройства после кика (`app.rs` `open_device`/`reopen_last`), флаг `light_off`.

10.32 — 2026-09-12 — итоговый статус дня: готово (дамп, протокол, host-рендеринг, память, CLI/GUI); доработать — откат nixosniri, батарея, 2.4G-канал, BT, драйверы F99/Ajazz.

10.33 — 2026-09-13 — фикс: «Записать в память» после гашения — зависание GUI (синхронный reconnect) + возврат света; отложенный reconnect 2.5 с.

10.34 — 2026-09-13 — запись в память вынесена из главного потока (`std::thread::spawn` + канал), `open_vidpid`, авто-восстановление канала (ретраи).

10.35 — 2026-09-13 — свет возвращался после power-cycle: «Записать в память» имел приоритет выбранного эффекта — гашение сбрасывает dropdown в «—».

10.36 — 2026-09-13 — КОРРЕКТИРОВКА: тумблер света НЕ data[76], а **слоты эффектов** `[58..96]` (данные a=00/09); затирание `5A A5` = откат к заводскому дефолту (никогда); эффект 0x0C.

10.37 — 2026-09-13 — фикс краша «Записать в память»→«Выключить свет»: `SourceId::remove()` на мёртвом таймере — обёртка `remove_source`.

10.38 — 2026-09-13 — автосохранение при закрытии окна (close-request → фоновый `SaveKind` probe → запись затем закрытие).

10.39 — 2026-09-13 — уход от `SourceId::remove()` ПОЛНОСТЬЮ: «поколение кадров» `frame_gen` (безопасная остановка всех таймеров); `remove_source` удалена.

10.40 — 2026-09-13 — подтверждено на железе: фиксы 10.31–10.39 работают, крашей нет; протокол закрыт по host-рендерингу, памяти и автосейву.

10.41 — 2026-09-13 — батарея 2.4G: ресивер в реестре (`3554:FA09`), `wireless_battery()` (0x0B по dnviti — তখন клава «спит на линке»); честные сообщения GUI/CLI. (cmd позже заменён на 0x05, 10.43.)

10.42 — 2026-09-14 — RGB на ресивере 2.4G часть 1: двухфазный выбор hidraw; trait-хуки `apply_rgb_persistent`/`apply_light_persistent`; протокол 0x13 (wl_read/wl_write_pages). СЕССИЯ ОСТАНОВЛЕНА НА ПОЛОВИНЕ.

10.42a — 2026-09-14 — закрыто CLI/GUI: `cmd_rgb` ветвление 2.4G (persistent, в память), флаги `kbc rgb ... --light-on/--light-off`, GUI `WlTask`/`spawn_wl_task` (запись в фоновом потоке); ресивер — без «Записать в память» (пишет сразу). Live: линк спит — ждём физ. переключение клавиатуры.

10.43 — 2026-09-14 — живой тест ресивера ПОДТВЕРЖДЁН: батарея cmd **0x05** (не 0x0B!) → 0x0A байт[6]; RGB/свет в память ресивера; свет off = обнуление всех 34 слотов.

10.44 — 2026-09-14 — «Записать в память» для ресивера: список эффектов подстраивается под устройство (`WL_EFFECTS` из dnviti); `save_hardware_effect` ветка Wireless; rebuild списка при смене USB↔2.4G.

10.45 — 2026-09-15 — найден НАСТОЯЩИЙ селектор ресивера **led[10]** (не led[1]); эмпирическая карта кодов (0x01..0x13, вкл. 0x13=выкл); запись только led[10] in-place; подтверждено визуально.

10.46 — 2026-09-15 — открытая задача: яркость/скорость на ресивере не работают (гипотеза — слоты пар эффектов, а не led[3]/led[4]) — закрыто в 10.49.

10.47 — 2026-09-15 — планы владельца: Ajazz-проводник, cargo check, Fedora+документация, иконка.

10.48 — 2026-09-15 — иконка подключена (`Window::set_default_icon_name`); Ajazz-проводник ужé был готов (FileDialog) — нечего править.

10.49 — 2026-09-15 — яркость/скорость ресивера НАЙДЕНЫ: слот `[58+2*(led[10]-1)]` (a=яркость, старшая тетрада b=скорость); `wireless_apply_effect` переписан; подтверждено визуально и readback.

10.50 — 2026-09-15 — GUI «Записать в память» применяет слайдеры яркости/скорости вместе с эффектом (`save_effect_with_style`).

10.51 — 2026-09-15 — иконка + USB-батарея «честно» + начало F99: KB.ini F99 = тот же SinoWealth SH68F90 (VID/PID и эффект-таблица как F75); 138 клавиш, 100 с геометрией.

10.52 — 2026-09-15 — USB-канал заряда ЗАКРЫТ: его НЕТ у F75 (железо не передаёт). README обновлён. Живые каналы: 2.4G ✅, BT ⬜.

10.53 — 2026-09-15 — ресёрч Ajazz AK820 Pro: протоколы wsclx/gohv/MemerGamer/fwpb/… — battery читается, энкодер firmware-fixed, TFT 0.85″/GC9107, статика сохраняется (save).

10.54 — 2026-09-16 — Ajazz-драйвер ПЕРЕПИСАН на wsclx (транспорт 0xAA/0x55, команды, RGB, keymap, макросы, TFT, battery, persistent-хуки); VID/PID 8009/800A/8099/fefe; организационные правки в udev/реестр/hid.

10.55 — 2026-09-16 — F99 реальный драйвер вместо заглушки: тот же протокол F75 (конфиг, direct 113 LED, память, data[10]); убраны все выдуманные cmd-id; CPU-`.

10.56 — 2026-09-16 — аудит GUI: согласован с устройствами, критич. багов нет; зафиксированы недоделки (живой тест, kbc save на ресивере, BT, keymap/макросы/энкодер в GUI, детекция F99, откат nixosniri, Fedora, README 160→128).

10.57 — 2026-09-17 — Fedora: `Makefile` + `fedora_install.sh` + README «Вариант 5» (dnf-пакеты, user/system, udev).

10.58 — 2026-09-17 — переименование fedora_install.sh + авто-стак зависимостей `--install-deps` + CI `fedora-build`; фикс CI (dnf-пакеты одной строкой без `\`).

10.59 — 2026-09-17 — ресёрч 4 репо (cloudglides/openaula/aula-f99-controller/gohv): найден ремап F75 (512-б таблица 0x83/03, защита Fn#53 и F1–F12); openaula slot-форматы; эффект-таблица offset 58 vs 64 (вопрос).

10.60 — 2026-09-17 — сверка Ajazz-драйвера со свежим wsclx PROTOCOL.md: транспорт/команды/payload/mакросы/keymap/TFT — совпадают; note про TFT-ack cmd 65 и magic 0x0650.

10.61 — 2026-09-17 — ресёрч 6 репо AK820 Pro: PID-варианты 0x800A/0x8099 добавлены в реестр/udev/README; TFT delay 2 vs 5 мс; sleep 4 ступени; два фрейминга картинки (3 байта).

10.62 — 2026-09-17 — F75 ремап РЕАЛИЗОВАН (read-modify-write): `read/write_keymap_raw`, slot-кодирование Press/Media, защита Fn#53 и F1–F12; CLI `kbc remap`; защита соединение только по USB.

10.63 — 2026-09-17 — Ajazz (ДУБЛЬ сохранить как один пункт): TFT-анимация (заголовок 256 б, delay 2 мс), синк времени (control-канал 0xFF13, chrono), sleep 4 ступени; GUI отправка GIF → set_screen_animation.

10.64 — 2026-09-17 — «забрано» из ресёрча: GET_LED_EFFECT(19)/SET_FACTORY_RESET(15)/GET_DEFAULT_*_MATRIX(28/31)/BOOT-анимация(64)/TFT-ack(65); GUI «Сбросить всё» через get_factory_keymap.

10.65 — 2026-09-17 — повторный чек-ресёрч + GET_MACRO(21): `get_macros` (index 400 б + data-блоки), типы Macro/MacroEvent; всё забрано.

10.66 — 2026-09-17 — полный аудит: для каждой клавиатуры что работает/нет (F75 — почти всё; F99 — конфиг/host/память, нет ремапа live/индикаторы/т.п.; Ajazz — весь wsclx без живого теста).

10.67 — 2026-09-17 — «добить» F75: выдуманные cmd-id убраны; set_rgb_* стали честными обёртками (set_rgb_color — пери-кей через color table; brightness/speed через слот активного эффекта); get_rgb_preset улучшен.

10.68 — 2026-09-17 — BT-батарея ЗАКРЫТА решением владельца; новый `docs/collect-data.md` для владельцев F99/Ajazz (read-only сбор).

10.69 — 2026-09-17 — `scripts/collect-data.sh` (read-only, `--device`, `--sg`, результат `~/kbc-diag-*` + tar.gz); фикс packet_read (hex-конвертация).

10.70 — 2026-09-18 — живая диагностика F99: **PID 258A:010C как у F75**, модель PSD 0xA4; конфиг 34 пары слотов; **color table `06 8A` читается** (вопрос 10.5 закрыт); collect-data.sh фикс автодетекта.

10.71 — 2026-09-18 — плавность host-рендеринга: тик 500→40 мс (20–25 FPS) в GUI apply, CLI hold и стриме; чёрный keepalive не тронут.

10.72 — 2026-09-19 — карта слотов F75 подтверждена дампом (68=`[`, 73=`=` и т.д.); `F75_SLOT_LABELS`/`slot_label`; CLI `--list` с метками; GUI «Показать раскладку».

10.73 — 2026-09-19 — GUI keymap: прокрутка (ScrolledWindow), понятное сообщение на 2.4G («раскладка только по USB»).

10.74 — 2026-09-19 — GUI: блок «Энкодер/ролик» удалён (сменить ролик нельзя); честный статус keymap F75 (только USB, клик-редактор — TODO).

10.75 — 2026-09-20 — F99 чтение keymap (0x83, Default+FN1), trait `slot_label_name`, collect-data.sh + [12] keymap карта.

10.76 — 2026-09-20 — модификаторы 0xE0–0xE7 в F75 slot_encode/decode; решение: per-key 2.4G = Unsupported до железа; сравнение двух транспортных форматов 2.4G (dnviti vs cloudglides).

10.77 — 2026-09-20 — CLI `kbc remap --modifier 0..7` + README; подтверждено на железе (слот 73→LShift), НО наблюдение: запись в Default меняет и FN1 (общий blob).

10.78 — 2026-09-20 — фикс F99 по живой диагностике: 34 пары слотов [58..125] (не 20); баг автодетекта (5 пробелов в `kbc info` — regex `Модель:\s+`).

10.79 — 2026-09-20 — `kbc daemon` + TOML-пресеты (`$XDG_CONFIG_HOME/keyboard-center/presets`) + systemd-user unit в home-module; тесты 5/5.

10.80 — 2026-09-20 — GUI «Файловые пресеты» подключены к PresetStore (apply/save/delete/refresh; sanitize имён).

10.81 — 2026-09-20 — инцидент: клавиша `=` (слот 73) «висела» как LShift; фикс `--key 73 --to 2E`; УРОК: всегда readback отката, тестовые ремапы только на пустых слотах.

10.82 — 2026-09-20 — F99 W-канал ремапа: `0x03` НЕ работает на железе (Unsupported, слот не меняется) → ремап F99 = read-only.

10.83 — 2026-09-21 — 2.4G интерактивные тесты в collect-data.sh для F99 (battery/rgb/light через ресивер; WL_EFFECTS файл).

10.84 — 2026-09-21 — `scripts/f99-usb-rev.sh` (write-тесты T1–T3 с восстановлением, никогда — маркер 5A A5; write-форматы config/color подтверждены).

10.85 — 2026-09-21 — живые write-тесты F75: T1 config `06 04` = commit (кик), latch-пакета НЕТ; T2 color table читается/пишется, self-define НЕ найден; T3 индикаторы boot 0x02 не работают; T4 enable-seq нет; T5 offset=58.

10.86 — 2026-09-21 — self-define селектор ЗАКРЫТ полным перебором data[10] ∈ 0x01..0x1F (негатив) — отображение цветовой таблицы невозможно, живой цвет только direct.

10.87 — 2026-09-21 — GUI клик-редактор keymap реализован (клик по клавише → выбор слота → назначение Press/Media/Modifier); trait `slot_for_keycode`, `Layout::key_at`.

10.88 — 2026-09-21 — автономные тесты USB F75: read-only/CLI всё подтверждено (list/info/probe/remap/battery/preset/rgb/save/raw/monitor/daemon); спросить владельца — живые A/B/C.

10.89 — 2026-09-21 — финальная поддержка F75 (USB+2.4G+GUI): GUI «Сбросить всё», слот 84 спец-Z идентифицирован, модификаторы, пресеты, поколение кадров, post-kick reconnect; пройдено на железе.

10.90 — 2026-09-22 — collect-data.sh `--write-tests` для F99 делегирует write-тесты в f99-usb-rev.sh; документация приведена в соответствие (4 файла).

10.91 — 2026-09-23 — CLI `kbc rgb --hw-effect` (2.4G, код в память ресивера): Ripple Shining 0x14 подтверждён на F75 (волна слева направо); WL_EFFECTS расширен до 21 кода.

10.92 — 2026-09-23 — аудит F99 USB по коду: поддержка НЕ полная (ремап read-only, per-key Unsupported, индикаторы/polling/debounce/sleep — не ревёрснуты); апдейт get_rgb_preset — TODO.

10.93 — 2026-09-23 — F99/2.4G: `kbc raw wl-read-pages` (дампы 0x44/0x49), sweep кодов эффектов под `--write-tests`, `--hw-effect` с приоритетом над `--mode`, порт get_rgb_preset F99 по образцу F75.

10.94 — 2026-09-23 — фикс «кривая вёрстка» на старте ПК: host-рендеринг начинал слать direct-кадры до полной инициализации LED-матрицы → 1–2 кривых кадра. Пауза перед первым кадром (изначально 4 с, в 10.97 снижена до 2 с по желанию владельца) в `cmd_daemon`; покрывает и `kbc preset apply`.

10.97e — 2026-09-24 — автономность аудио-реактива: ленивое открытие + авто-переоткрытие аудио-потока в `cmd_daemon` (перезапуск PipeWire/PulseAudio, поздний старт звука/клавиатуры больше не роняют демон); уточнение `daemon.preset` в `nix/home-module.nix` и README; работает ТОЛЬКО по USB.

10.95 — 2026-09-23 — аудит API перед заморозкой: `trait KeyboardDevice` — внутренний (не экспортируется), публичные контракты = CLI `kbc` + TOML-пресеты + udev/nix/systemd. Решение: функциональный код НЕ менять (мёртвые методы — честный «задел под Ajazz», не баг); версия поднята 0.2.0 → 0.3.0; сборка после бампа зелёная (check 0 ошибок / тесты 5/5 / release OK). Подробности — блок «10.95 АУДИТ API» ниже.

10.96 — 2026-09-24 — «полноценная API»: версионированы TOML-пресеты (`version = 1`, fallback для старых файлов, отказ от новее-1, `validate()` при загрузке); справочник CLI-команд и формат TOML-пресета в README; исправлены расхождения журнала (сводка CLI: добавлены screen/time/set-*/profile, `light` → флаги `--light-on/--light-off`); юнит-тесты протокола F75/F99/Ajazz (slot encode/decode, защита слотов, build_frame, actions_from_keys; 17/17); путь пресетов в README/журнале приведён к фактическому `~/.config/keyboard-center/presets`. Подробности — блок «10.96 ПОЛНОЦЕННАЯ API» ниже.

10.97 — 2026-09-24 — аудио-реактив (VU-спектрограмм): `RgbMode::Audio` («audio»), новый модуль `src/audio.rs` (захват системного звука через PulseAudio/PipeWire монитор `@DEFAULT_MONITOR@` — libpulse-simple, FFT-спектр 16 лог-полос через rustfft, сглаживание attack/decay, кадр `audio_frame` = «столбики» по колонкам, активные полосы спектральным hue, фон — базовый цвет); работает ТОЛЬКО host-рендерингом по USB в `kbc rgb --mode audio` и `kbc daemon`/systemd (без GUI), на 2.4G/BT — честный отказ; F99 — та же аудио-матрица 113/24. Зависимости: `psimple`/`pulse` (libpulse-*-binding 2) + `rustfft 6`; flake buildInputs += pulseaudio; Fedora — pulseaudio-libs-devel (fedora_install.sh/Makefile). Тесты аудио: полосы лог-шкалы, тишина→~0, бас доминирует на 100 Гц, форма кадра; ИТОГ `cargo test --lib` **21/21** (было 17, +4), check --all-targets 0 ошибок, release OK. Подробности — блок «10.97 АУДИО-РЕАКТИВ» ниже.

10.97b — 2026-09-24 — доводка аудио-реактива по живым отзывам владельца: (1) ФИКС ЛАГА ~6 с при повторном старте музыки — в аудио-цикле `read_vu_bands()` уже блокирует ~46 мс, а сверху был ещё `sleep(frame_ms)` → читали медленнее реального времени, capture-буфер копил отставание; `sleep` убран в `cmd_rgb`/`cmd_daemon` (аудио-режим), темп задаёт монитор, плюс `BufferAttr.maxlength=4096` (старьё не копится). Владелец: «идеально». (2) ВЫРАЗИТЕЛЬНОСТЬ/СКОРОСТЬ: `FFT_WINDOW 2048→1024` (кадр ~23 мс, реакция ×2), спад `0.45→0.6`, усиление `rms*32→rms*14→rms*9`, подъём ВЧ `gain 1.0→2.5`, порог `0.03→0.015`, sat `0.9→1.0`; `F_MIN 20→40 Гц` (при окне 1024 бины ~43 Гц). (3) ВСТРОЕННЫЙ ПРЕСЕТ `audio-viz` в `src/presets.rs` (`builtin_presets`/`builtin`/`is_builtin`): зашит в бинарник, виден в `kbc preset list` (`[встроенный]`) и в GUI «Файловые пресеты» у любого, `remove` запрещён, свой файл с тем же именем перекрывает. Тесты **23/23** (+2: builtin/override). README обновлён.

10.97c — 2026-09-24 — аудио-реактив в GUI: владелец сообщил «в gui не применяются». Причина: `RgbMode::Audio` отсутствовал в dropdown режимов, GUI host-цикл умел только таймер. Сделано: в `populate_modes` добавлен пункт «Аудио-реактив» (индекс 7, «Пользовательская» → 8); `mode_from_index`/`mode_to_index` обновлены; в USB-ветке `apply` при `mode == Audio` запускается ФОНОВЫЙ поток (`std::thread`) — открывает устройство через `open_vidpid`, `AudioReactive::open()`, крутит `read_vu_bands` → `audio_frame` → `render_frame` (без sleep, темп задаёт read); остановка — `Arc<AtomicBool>` в `audio_stop: Rc<RefCell<Option<Arc<AtomicBool>>>>` (в главном потоке блокирующий read подвесил бы GUI). Стоп-флаг снимается при: любом новом «Применить» (в начале обработчика), «Выключить свет», и при выборе файлового пресета (идёт через apply). Доступность монитора проверяется синхронно (`AudioReactive::open()`) — понятная ошибка в статусе. Обработчик файлового пресета больше НЕ делает special-case для Audio (применяется штатно). Тесты 23/23, check --all-targets 0 ошибок, release OK.

10.97d — 2026-09-24 — аудио-реактив: тишина + только верхний ряд. Жалобы владельца: «эффект со скоростью начинается ещё до воспроизведения музыки» и «сделать, чтобы реагировал только ряд F1-F12, а буквы светились статично».
(1) ПОРОГ ТИШИНЫ: причина «эффекта до музыки» — нормировка полос по RMS раздувала фоновый шум (mag/(rms*9) при почти нулевом rms). Добавлен `RMS_FLOOR = 0.002`: если RMS блока ниже — полосы обнуляются (сглаживание плавно гасит), `read_vu_bands` и тестовая `compute_bands_from_samples` синхронны. Теперь в тишине/без звука кадр полностью статичен.
(2) ТОЛЬКО ВЕРХНИЙ РЯД: `audio_frame` переписан — вместо «столбиков по колонкам» (`i % cols`) принимает `active_leds: &[usize]` (LED-индексы верхнего ряда, слева→направо), каждый активный LED = полоса спектра (hue), все прочие LED — статичный базовый цвет (`base` на полной яркости). Порог активации полосы `ACTIVATION = 0.015`.
(3) LED-ИНДЕКСЫ ВЕРХНЕГО РЯДА — ИЗ OpenRGB (не догадка): найден `SinowealthKeyboard10cDevices.cpp` (GitHub-зеркало OpenRGB). LED-индекс = слот клавиши (OpenRGB `aula_f75_layout`, модель 0xCD: Esc=0, F1=12, шаг 6 … F12=78; `aula_f99_layout`, 0xA4: Esc=0, F1=6, шаг 6 … F12=72). Наши keymap-слоты F75 (12,18,…,78) совпали с OpenRGB — подтверждает, что порядок `render_frame` = порядок OpenRGB. Константы `AUDIO_TOP_ROW_LEDS` в `aula_f75.rs`/`aula_f99.rs`; helper `devices::audio_top_row_leds(led_count)` (90→F75, 113→F99, иначе пусто→статика). Вызовы обновлены в `cli.rs` (cmd_rgb/cmd_daemon) и GUI-потоке.
(4) ТЕСТЫ: `audio_frame_only_top_row_reacts` (активный LED цветной, буква статична, в тишине верхний ряд статичен) + обновлён `audio_frame_shape`. ИТОГ `cargo test --lib` **24/24**, check --all-targets 0 ошибок, release OK. README обновлён.
(5) РЕШЕНИЕ ВЛАДЕЛЬЦА: Esc в верхнем ряду и полная яркость статичных «букв» — оставить как есть (2026-09-24).
(6) ФИКС GUI «выбираю пресет — всё как было, эффект без музыки»: в ветке `mode == Audio` в `apply` НЕ инкрементировался `frame_gen`, поэтому прежний host-таймер (старый эффект) продолжал гнать кадры поверх аудио. Добавлен `frame_gen.set(get()+1)` перед запуском аудио-потока (как в прочих ветках). Статус-текст поправлен на «верхний ряд (Esc+F1..F12)». Пересобраны и release, и debug (важно: `target/debug/keyboard-center` был устаревшим — запуск через `cargo run` давал старый код). Тесты 24/24.
(7) ЧУВСТВИТЕЛЬНОСТЬ (жалоба «F-ряд будто статичен, лишь изредка моргает»): `ACTIVATION 0.015→0.006`, делитель нормировки `rms*9→rms*3.5` (константа `NORM_DIVISOR`, синхронно в `read_vu_bands` и `compute_bands_from_samples`), кривая яркости активной полосы `level→sqrt(level)`, `k = bright_k*(0.45+0.55*eff)`. Порог тишины `RMS_FLOOR` не тронут. Тесты 24/24, release+debug пересобраны.
(8) БАГ FFT-БИНОВ + ещё +80% (жалоба «мигает только F12, остальное статично»): индекс бина считался `f/nyquist * FFT_WINDOW`, а надо `f/nyquist * (FFT_WINDOW/2)` (положительных бинов ровно N/2) — все полосы «съезжали» по частоте вдвое (низкие ловили не свой диапазон, верхние уходили в ~20 кГц). Исправлено в `read_vu_bands` и `compute_bands_from_samples`; проверено диагностикой (120 Гц→низ, 1 кГц→середина, 8 кГц→верх). Плюс `NORM_DIVISOR 3.5→1.94` (+80% по просьбе владельца). Диагностический тест удалён. Тесты 24/24, release+debug пересобраны.
(9) КАЛИБРОВКА: на `NORM_DIVISOR=1.94` полосы «прилипали» к максимуму — визуально чисто статика (жалоба владельца «нет эффектов, чисто статика»). Снижено до 30% от той чувствительности: `NORM_DIVISOR 1.94→6.5`. Тесты 24/24, release+debug пересобраны.
(10) ПЕРЕХОД НА AGC (нормировка по пику кадра): при 6.5 стало наоборот — все полосы ниже порога, снова «статика». Вывод: нормировка по RMS не даёт рабочего окна (либо всё в максимум, либо всё в ноль). Заменена на AGC — «сырые» магнитуды полос нормируются по самой громкой полосе текущего кадра (`raw[b]/max_mag`), амплитуда сигнала не влияет, всегда виден разброс по спектру; порог тишины `RMS_FLOOR` сохранён (нет «эффекта до музыки»). `NORM_DIVISOR` удалён, `gain` ВЧ снижен 1.5→1.0. Синхронно в `read_vu_bands` и `compute_bands_from_samples`. Тесты 24/24, release+debug пересобраны.
(11) ЦВЕТ АКТИВНОСТИ: по просьбе владельца радужный hue (0..260) заменён на градиент **зелёный (низы) → синий (верхи)**: `hue = 120 + (bi/bands)*120`. Привязка цвета к полосе спектра сохранена. Тесты 24/24, release+debug пересобраны.
(12) ТОНКАЯ ЧУВСТВИТЕЛЬНОСТЬ: введён множитель `SENSITIVITY` (после AGC) — ручка общей реактивности без ломки AGC. Установлен 0.75 по просьбе владельца («чуть уменьши чувствительность»). Тесты 24/24, release+debug пересобраны.
(13) ПЕРЕНОС НА РЯД ЦИФР (ОТКАЧЕНО в тот же день): аудио-реактив пробовали на ряде цифр, но владельцу не понравилось («вернем f ряд»). Возвращён верхний ряд (Esc+F1..F12). Имена `AUDIO_ACTIVE_ROW_LEDS`/`audio_active_row_leds` оставлены (корректнее прежних `*_TOP_ROW_*`), значения — снова F-ряд: F75 `[0,12,18,…,78]`, F99 `[0,6,…,72]`. Тесты `audio_frame_shape`/`audio_frame_only_active_row_reacts` обновлены под F-ряд, 24/24, release+debug пересобраны.

10.98 — 2026-09-24 — read-only диагностика Ajazz AK820 Pro: новая CLI-сабкоманда `kbc ajazz-diag` + дефолтный метод трейта `diagnostic_dump()`; скрипт `scripts/ajazz-usb-diag.sh`; доки `docs/ajazz-guide.md`; интеграция в `collect-data.sh`/`collect-data.md`/README. Тесты 24/24. Подробности — блок «10.98 AJAZZ READ-ONLY DIAG» ниже.

10.99 — 2026-09-24 — Phase 1 аудио-реактив параметризация: TOML секция `[audio]` (sensitivity/color_low/color_high), CLI флаги `--color-low/--color-high/--sensitivity` для `rgb`/`daemon`/`preset save`, GUI секция «Аудио-реактив (USB)» с цветовыми полями и слайдером, пресет save/load. `AudioReactive::open_with(sens, speed)`, `audio_frame` с интерполяцией RGB. Тесты 25/25. Подробности — блок «10.99 АУДИО-РЕАКТИВ ПАРАМЕТРИЗАЦИЯ (ФАЗА 1)» ниже.

10.100 — 2026-09-25 — Phase 2 аудио-реактива: Cava-стиль `style = "bars"` с точной геометрией OpenRGB для F75/F99, CLI/GUI-выбор стиля и встроенный пресет `cava`; `audio-viz` сохранён как совместимый row-режим. Тесты 31/31. Подробности — блок «10.100 АУДИО-РЕАКТИВ PHASE 2: CAVA-СТИЛЬ (BARS)» ниже.

10.101 — 2026-09-25 — синхронизация документации после 10.100: удалены временные пользовательские тестовые пресеты `test-apply`/`test-audio`/`test-audio2` (в репозитории не лежали), уточнено, что `cava` — встроенный пресет бинарника (доступен любой установленной сборке без внешних файлов), исправлено описание геометрии F99 (известные позиции NumPad участвуют в Bars), README и `docs/f99-guide.md` приведены в соответствие. Код не менялся; `cargo test --lib` — 31/31, `cargo check --all-targets` — 0 ошибок, `cargo build --release` — OK.

10.103 — 2026-09-28 — доступ к hidraw после `nixos-rebuild` разобран и исправлен: в `99-keyboard-center.rules` `ATTR{}` → `ATTRS{}` (было — правила молча не срабатывали никогда); на NixOS ручная установка правил невозможна (`/etc/udev/rules.d` → симлинк на read-only `/etc/static`), модуль `keyboard-center` ПОСТОЯННО подключён в `~/nixosniri` (input + `nixosModules.default` на хосте `darkside_pc`), до перезапуска — страховка `services.udev.extraRules`; `plugdev` уже был в `core/userset.nix`; `target/`-бинари переживают GC, т.к. все их библиотеки закреплены системой. Фикс правил закоммичен локально (`c424502`), **без пуша**. Подробности — блок «10.103 ДОСТУП К HIDRAW / UDEV-ПРАВИЛА» ниже.

10.104 — 2026-09-28 — иконка приложения не отображалась в меню (лаунчер): в `flake.nix` `postInstall` копировал `.desktop` и `.svg` через `cp <исходник> $out/share/<каталог>/` (без явного имени приёмника) → Nix 2.34 кладёт файлы репо в store как `<хэш>-keyboard-center.desktop` / `<хэш>-keyboard-center.svg`, `cp` брал basename, и в `$out` файлы получали префикс-хэш (`kgswh2nnv…-keyboard-center.desktop`, `if0rp850…-keyboard-center.svg`). Лаунчер имя `.desktop` не учитывает, но `Icon=keyboard-center` не резолвился. Фикс: `install -Dm444` с явными именами (desktop, svg, udev-правило). `nix build .#keyboard-center` — успешно, имена в `$out` правильные. Подробности — блок «10.104 ИКОНКА ПРИЛОЖЕНИЯ В МЕНЮ» ниже.

10.105 — 2026-09-28 — удаление на Fedora/без nix вынесено в отдельный скрипт: новый `fedora_uninstall.sh` (полное удаление системной + пользовательской установки — бинарники, `.desktop`, иконка, udev-правила + reload/trigger; флаги `--purge` (пресеты + кэш иконок) / `--keep-rules` / `--dry-run`; требует sudo при наличии системных файлов); `Makefile` — `UNINSTALLER`, цель `uninstall` идёт через новый скрипт (+ `UNINSTALL_FLAGS=--purge`); `fedora_install.sh` — шапка/подсказка про новый скрипт, `--help` больше не печатает shebang; README — «Шаг 6. Удаление», таблица таргетов, параметры обоих скриптов, дерево проекта, чеклист; CI-джоб `fedora-build` — `bash -n` для установочных и диагностических скриптов. Проверено: `bash -n` для всех 6 скриптов — OK, `make help` / `make -n uninstall` — OK, `--dry-run` — OK. Правки разрешены владельцем; **коммит делает владелец сам**.

10.102 — 2026-09-25 — подготовка версии 0.4.0: версия синхронизирована в `Cargo.toml`, `Cargo.lock`, `flake.nix` и README; в README добавлен раздел с составом релиза. `cargo check --all-targets` — 0 ошибок, `cargo test --lib` — 31/31, `cargo build --release` — успешно, `nix build .#keyboard-center` — успешно; `target/release/kbc --version` и `result/bin/kbc --version` — `0.4.0`. Сохранились два предсуществующих предупреждения `unused_mut` в `src/gui/keymap_editor.rs:308,323`.

10.106 — 2026-09-28 — **живая разведка Ajazz AK820 Pro на Fedora-машине владельца.** Переподключение USB сдвинуло нумерацию hidraw → старые пути умерли; `/dev/hidrawN` хардкодить нельзя, узел ищется по `06 13 FF` в `report_descriptor`. `0xFF13` = unnumbered Feature **64 б**, HIDAPI-буфер = 65 б (report ID `0x00` + 64). **Подтверждено живьём:** `feature-send` 64 б уходит без EPIPE (у FF68 Feature нет вообще → EPIPE, и `output-send` FF68 таймаутит из-за `Report Count = 4096` в дескрипторе против 64-байтных пакетов firmware). Найден первоисточник протокола подсветки (MemerGamer `AjazzAk820ProUnofficialWebdriver`, кредит gohv + KyleBoyer/TFTTimeSync-node): 3 шага `04 18` RESET → `04 13 …` CONFIGURE → data-пакет; **команда в `p[1]`, `p[0]` = код эффекта**, яркость/скорость **0..5**. Первые попытки были собраны неверно (команда в `p[0]`, яркость `0xFF`, без преамбулы) — визуального эффекта не дали, отправка при этом проходила. Также: `0C45:FDFD` «Microdia 2.4G Dongle» — не клавиатура (5 интерфейсов, только interrupt-IN, нет FF13/FF68), в usbmon легко спутать; ошибки «нечётное число hex-цифр» = потеря символа при копировании. Правильная последовательность на живом железе **ещё не проверена**.

10.107 — 2026-09-28 — Fedora: два скрипта заменены одним `fedora.sh` (интерактивное меню: установка с нуля / обновление / удаление; установка и обновление — user или system). Правки логотипа GUI откачены (владелец отказался от идеи): `src/gui/app.rs`/`Cargo.toml`/`Cargo.lock` вернуты из git, `data/keyboard-center.png` удалён. Подробности — блок «10.107 ЕДИНЫЙ FEDORA.SH» ниже.

10.108 — 2026-09-30 — фикс `scripts/f99-usb-rev.sh` (тесты владельца F99, orflem): (1) `run_kbc` при `--sg` не экранировал аргументы → `sg plugdev -c` разбивал 520-байтный hex по пробелам, clap видел лишние аргументы (`unexpected argument '84'`); перенесён фикс `printf '%q'` из `collect-data.sh`; (2) `packet_read_keymap` форматировал слой через `%s` (байт `0` полутетрадой вместо `00`) → «нечётное число hex-цифр: 06 83 00 0 01 00 00 02»; теперь `%02X`; (3) `yes 00 | head -NNN` в write-пакетах T1/T4 килл-sировал скрипт через **SIGPIPE 141** (`head` закрывал пайп, `yes` получал SIGPIPE, `pipefail` превращал в 141; на выходе `{ec: 141}` после ответа «да» в T1): пад переписан циклами `for ((i=0;i<384;i++)); do printf ' 00'; done` (в T4 `yes | head -0` был пустым и просто удалён). Заодно `packet_read`/`packet_read_keymap` приведены к варианту `collect-data.sh` (цикл до 512 нулей вместо `yes 00 | head`). Проверено: `bash -n`, длина hex ровно 520 б.

10.109 — 2026-09-30 — полный прогон `scripts/f99-usb-rev.sh --device f99 --write-tests` на железе (orflem, F99 USB `258A:010C`, PSD `0xA4`). Все 6 тестов прошли без SIGPIPE. Ключевые находки:
- **T1 config round-trip (06 04):** ✓ запись/чтение конфига работает, `data[10]` (селектор эффекта) меняется и читается обратно. **Визуально эффект не меняется** — self-define селектора нет (как F75, 10.86), host-рендеринг перебивает.
- **T2 boot indicators (output-report 0x02):** ✓ **индикаторы Caps/Num/Scroll РАБОТАЮТ** через boot keyboard output-report (на F75 — Broken pipe!). Значит F99 поддерживает стандартный HID LED report.
- **T3 self-define hunt (06 0A color table):** ✓ color table пишется/читается (красные 3 клавиши в readback). **Self-define селектор НЕ НАЙДЕН** — per-key цвета визуально не отображаются ни при каком `Fn+\`. Подтверждает: self-define не существует (как F75 10.86).
- **T4 keymap W-channel (0x03):** пропущен (owner ответил "нет"). По 10.82 — ожидаемо read-only.
- **T5 color table per-key + self-define:** readback не совпал с записанным, self-define не найден.
- **T6 indicators config diff:** **НАЙДЕНЫ байты индикаторов в конфиге** (дифф показывает изменения вне `[58..125]`): `data[9]` (0x01→0x00) и `data[10]` (0x12→0x00). `data[10]` — также селектор эффекта (конфликт: нажатие NumLock сбросило эффект в 0x00). Требует уточнения: возможно, индикаторы маппятся на те же байты, или прошивка F99 специфична. Файлы: `T6-config-ind-diff.txt`, `T6-indicators-diff.txt`.

Выводы для `aula_f99.rs`:
1. `write_config` (06 04) — реализовать для USB (как F75).
2. `set_rgb_color` для индикаторов — через boot output-report `0x02` (работает на F99).
3. Self-define — убрать попытки, честный `Unsupported`.
4. W-channel keymap `0x03` — оставить `Unsupported` (read-only).
5. Добавить константы индикаторов из T6 diff после уточнения offsets.

10.110 — 2026-09-30 — реализация находок F99 (журнал 10.109) в `src/devices/aula_f99.rs` и `src/devices/hid.rs`:
- **Boot indicators (output-report 0x02):** реализован `set_rgb_color` для специальных KeyId: `0xFF00` = CapsLock (bit 1), `0xFF01` = NumLock (bit 0), `0xFF02` = ScrollLock (bit 2). Чёрный цвет (0,0,0) = выключить, любой другой = включить. Добавлен `HidBridge::open_boot_keyboard()` — поиск boot keyboard интерфейса (usage_page=0x0001, usage=0x0006) и отправка output-report `[0x02, mask]`.
- **Индикаторы в конфиге:** добавлены константы `CFG_INDICATOR_CAPS = 9` и `CFG_INDICATOR_NUM_EFFECT = 10` (T6 diff, журнал 10.109). `data[10]` — конфликт: селектор эффекта + NumLock (нажатие NumLock сбрасывает эффект в 0x00).
- **Self-define / W-channel keymap:** уже честный `Unsupported` — оставлено без изменений.
- **`write_config` (06 04):** уже был реализован (строки 199–213), диагностика подтвердила работоспособность.
- Тесты: `cargo test --lib` — 31/31, `cargo build --release` — OK.

10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

## 10.94 FIX «КРИВАЯ ВЁРСТКА» НА СТАРТЕ ПК: ПАУЗА 4 С ПЕРЕД ПЕРВЫМ КАДРОМ HOST-РЕНДЕРИНГА (сессия 2026-09-23)

> Жалоба владельца F99: «после включения ПК пресет один раз ложится криво (смазанный/разорванный кадр), потом переподключение клавиатуры чинит» или схожее на старте сессии через `kbc preset apply`/daemon. Решение владельца: «обнови журнал и обнови код чтобы пофиксить баг» — запись в журнал и правки кода явно разрешены.

### ДИАГНОЗ (но без живого железа — объяснение по протоколу)
- **Гонка инициализации:** после boot ПК клавиатура (USB, PID `258A:010C`, F99/F75) ещё инициализирует LED-матрицу/контроллер, когда наш host-рендеринг уже начинает слать direct-кадры `06 08 … 7A 01`. Ранние кадры ложатся на «полуготовую» матрицу → один-два кадра выглядят «кривыми» (смазанными/разорванными).
- **Почему переподключение чинит:** unplug/plug = полный USB re-enum/reset → клавиатура перезагружается, конфиг/матрица инициализируются заново → последующие кадры ровные. Постоянного фреймминг-бага НЕТ.
- **Проверка отсутствия постоянного фрейминга (для владельца):** `kbc rgb 258A:010C --mode static --color FF0000` → должна быть ровная красная заливка; если да — проблема только в гонке на старте.
- **Обход БЕЗ кода (до фикса):** подождать ~3–5 с после загрузки сессии перед `kbc preset apply`/запуском daemon.

### ФИКС В КОДЕ (`src/cli.rs`, `cmd_daemon`, host-ветка)
- Перед первым кадром host-цикла добавлена **пауза 4 с** (`std::thread::sleep(std::time::Duration::from_secs(4))`) с сообщением «ожидание инициализации подсветки клавиатуры ~4 с (fix „кривая вёрстка“ на старте)…».
- Размещение: в host-ветке после println «host-эффект до Ctrl-C…» и до `let mut t_ms = 0u32;` — то есть пауза один раз перед стартом цикла кадров (не между кадрами).
- Покрывает и `kbc preset apply` (делегирует в `cmd_daemon`, журнал 10.79) — «кривая вёрстка» уходит и для пресетов.
- Длительность 4 с выбрана как безопасный запас: клавиатура успевает доинициализироваться; при живых жалобах «всё ещё криво» — увеличить до 5–6 с.

### СБОРКА
- `cargo build --release` — успех; `cargo check --all-targets` — 0 ошибок (только 2 предсуществующих `unused_mut` в `keymap_editor.rs` 308/323 — вне скоупа); `cargo test --lib` — 5/5.

### ПРОВЕРКА — ожидается для владельца F99
1. Перезагрузить ПК, сразу `kbc preset apply` (или daemon) → первый кадр ровный, без «кривой вёрстки» (пауза 4 с работает).
2. Для дельты паузы: если старт всё ещё кривой — поднять 4 → 5–6 с и пересобрать.
3. F75 по USB host-эффекты — пауза тоже применяется (общий `cmd_daemon`), не мешает.

### ОСТАЛОСЬ (открыто, вне этой задачи)
- Живой прогон чек-листа 10.93 на F99/receciver (`3554:FA09`): дампы [2.4G-4], sweep [2.4G-6], `--hw-effect 0x14`, `get_rgb_preset` — от или владельца F99; до результатов ничего не менять.
- Ремап-запись F99 (W-канал не 0x03, 10.82); per-key, индикаторы, polling/debounce/sleep — по 10.92.
- Ajazz на железе; откат временных правок `~/nixosniri` (10.2); публикация — по решению владельца.


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

## 10.95 АУДИТ API ПЕРЕД ЗАМОРОЗКОЙ (сессия 2026-09-23)

> Просьба владельца: разобраться с API перед версией 1.0. Решение владельца: «что сделать на данном этапе правильно и безопасно» → принято: **функциональный код НЕ менять**, зафиксировать тракт и задокументировать задел.

### ГРАНИЦЫ API
- **Внутренний (не публичный):** `trait KeyboardDevice` (`src/devices/mod.rs:72`) — 43 метода, 7 блоков (базовое / RGB / keymap-Fn / screen / encoder / macros / system / EEPROM / persistent 2.4G). lib входит ТОЛЬКО в бинарник, наружу не экспортируется.
- **Публичные контракты (это и есть «API» для пользователя):** CLI `kbc`, формат TOML-пресетов (`RgbPreset {mode,color,brightness,speed,frame_ms}`, serde snake_case), udev-правила, nix-модули, systemd-unit `keyboard-center-daemon`.

### ЧТО НАШЛИ (аудит вызовов метода trait из CLI/GUI — `set_*`/`get_*` по grep)
- **Мёртвые методы (не вызываются нигде, реализованы как честный `Unsupported` или задел):**
  - `set_encoder_action` — GUI блок энкодера удалён (10.74); у всех трёх драйверов Unsupported.
  - `set_macro` — не вызывается (в GUI только `record_macro`/`stop_macro`, keymap_editor 353/359).
  - `get_macros`, `get_current_effect`, `factory_reset` — реализованы только у Ajazz, из приложения не вызываются (GUI «Сбросить всё» идёт через `get_factory_keymap` → `set_fn_layer`).
  - `set_rgb_brightness`/`set_rgb_speed` — вызовов нет: яркость/скорость идут через host-параметры (`HostEffectParams`) и `apply_rgb_persistent`.
- **Пересечение «запись в память» (5 методов):** `apply_rgb_persistent`, `apply_rgb_persistent_code`, `save_hardware_effect`, `save_effect_with_style`, `save_hardware_light` — семантически накладываются; менять не стали (риск регрессии на живом F75).
- **Непоследовательность «обязательные vs дефолтные»:** ~20 обязательных методов в большинстве драйверов — честные Unsupported-заглушки; `set_screen_image` обязателен, хотя `set_screen_animation` дефолтный.

### РЕШЕНИЕ
- Код НЕ трогаем: мёртвые методы — это «задел под Ajazz/будущее» с честным `Unsupported`, не баг. Чистка trait без живого железа по всем трём моделям = риск регрессии без пользы.
- Версия поднята **0.2.0 → 0.3.0** (Cargo.toml / flake.nix / README badge) — новое функциональное наполнение vs 0.2.0 (F75 USB+2.4G, host-движок, пресеты+daemon, ремап, F99, Ajazz+Screen Manager, Fedora/Makefile).
- Сборка после бампа зелёная: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в `keymap_editor.rs` 308/323, вне скоупа); `cargo test --lib` — 5/5; `cargo build --release` — OK.

### НА БУДУЩЕЕ (к чему вернуться при подготовке 1.0)
- Решение судьбы мёртвых методов trait (удалить или оставить задел) — при согласовании с владельцем, лучше после живого теста Ajazz.
- Стабилизировать CLI-сабкоманды и TOML-формат документально (тест `preset_roundtrip` уже покрывает сериализацию).
- Отличия обязательных методов → дефолтные заглушки — по желанию, не критично.


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

## 10.96 ПОЛНОЦЕННАЯ API (сессия 2026-09-24)

> Просьба владельца: «что нам ещё нужно для полноценной api» → команда «сделай всё, что необходимо» (явное разрешение на правки кода и документации). Итог: TOML-пресеты версионированы (обратная совместимость), CLI-контракт зафиксирован в README, протокольные декодеры покрыты юнит-тестами.

### ЧТО СДЕЛАНО
- **Версионирование TOML-пресетов** (`src/presets.rs`):
  - добавлено поле `version: u32` в `ModeParams` (текущая `PRESET_FORMAT_VERSION = 1`).
  - `#[serde(default = "default_format_version")]` — старые файлы без `version` читаются как v1 (обратная совместимость, подтверждено тестом).
  - метод `ModeParams::validate()`: версия > понятной (`PRESET_FORMAT_VERSION`) → ошибка «пресет из новой схемы» с подсказкой; вызывается при загрузке в `PresetStore::load`.
  - инициализаторы обновлены: `Default` (presets.rs), GUI-сохранение пресета (`rgb_studio.rs:722`); дефолт путь `DEFAULT_FRAME_MS` не менялся.
- **Путь пресетов приведён в соответствие с кодом:** фактический `$XDG_CONFIG_HOME/keyboard-center/presets` (fallback `~/.config/...`), `src/presets.rs:86`. Были `~/.local/share/kbc-presets` в README и журнале — исправлено в README:88 и AGENTS.md (сводка + структура + п.10.79). nix-модули путь не упоминают.
- **Справочник CLI-контракта в README** (после блока примеров `kbc`): таблица всех сабкоманд (`list/info/rgb/screen/time/battery/save/remap/set-poll/set-debounce/set-sleep/profile/preset/daemon/monitor/probe/raw`), ключевые флаги, формат `VID:PID`/цвета/эффектов, пометка experimental (`--hw-effect`, `wl-read-pages`). Рядом — раздел «Формат TOML-пресета» с `version`.
- **Журнал приведён к фактическому CLI:** в сводке команд `light on|off` заменено на флаги `rgb --light-on/--light-off` (отдельной команды `light` в коде НЕТ — только флаги, `cli.rs:118/121`); добавлены `screen/time/set-*/profile`, `save [--effect|--brightness|--speed]`, `wl-read-pages`. Запись 10.42a поправлена аналогично.
- **Юнит-тесты протокола** (в добавленных `#[cfg(test)]`-модулях драйверов):
  - `aula_f75.rs`: slot encode/decode roundtrip (Press/Media/Modifier), маскирование модификатора `& 0x07` (9→0xE1, обратно Modifier(1)), decode None для unbind/page 13/page 3, защита слотов (Fn#53, F1–F12, трейлер 127).
  - `aula_f99.rs`: slot_decode Press/Media/None для unbind/page 13.
  - `ajazz_ak820.rs`: slot encode/decode roundtrip (Press/Media/FnLayer/Macro), Modifier кодируется как page 2 + 0xE0+m и декодируется как Press(0xE1) (ОБРАТНОГО mapping НЕТ — зафиксировано тестом), `actions_from_keys` (пары press/release, wire-флаги 0xB0/0x30), `build_frame` (magic 0xAA, cmd, len, addr LE, last-флаг, payload с offset 8).
  - ИТОГ: `cargo test --lib` → **17/17** (было 5/5; новые 12), `cargo check --all-targets` 0 ошибок, `cargo build --release` OK.

### НЮАНСЫ / ВАЖНО
- У F75 и F99 модификатор ДЕКОДИРУЕТСЯ обратно в `KeyAction::Modifier` (0xE0..0xE7 → Modifier(m)), у Ajazz — НЕТ: page 2 всегда читается как `Press`. Это особенность декодера wsclx (формат кастома), не баг; отражено комментарием и тестом.
- `check_writable_slot` в F75: слот 126 (предпоследний) — валиден для записи, запрещён только 127 (трейлер `5A A5`). Защита F1–F12 и Fn#53 — по cloudglides/openaula.
- Функциональный код протокола НЕ менялся (контракт 10.95 соблюдён) — только добавлены декодерные тесты и поле `version` в структуру пресета (обратно-совместимо).

### ОСТАЛОСЬ (открыто, вне этой задачи)
- Живой прогон F99/2.4G (10.93 чек-лист + 10.94 фикс); ремап-запись F99 W-канал (10.82); Ajazz на железе; BT-канал; откат временных правок `~/nixosniri` (10.2) + судьба модуля в системном флейке; публикация/релиз — по решению владельца.


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

## 10.97 АУДИО-РЕАКТИВ (VU-СПЕКТРОГРАММ) (сессия 2026-09-24)

> Историческая запись состояния на 2026-09-24. Описанный ниже первоначальный аудиорежим и ограничения GUI относятся к этой версии; актуальные стили `row`/`bars`, геометрия и текущий GUI описаны в 10.100–10.101.

> Идея из п.7 «идей на будущее» (audio-reactive). Владелец: «давай реализовывать, только так, чтобы и без открытого приложения работало» → решение: работает через `kbc rgb --mode audio` И `kbc daemon`/systemd-user unit (GUИ не требуется). Владелец выбрал: источник = PulseAudio/PipeWire монитор `@DEFAULT_MONITOR@` (libpulse-simple), стиль = спектр-полосы (VU).

### ЧТО СДЕЛАНО
- **`RgbMode::Audio`** (`src/types.rs`; serde snake_case → `"audio"`); `mode_name` → «аудио-реактив» (`src/presets.rs`); `parse_rgb_mode` принимает `audio|audio-reactive` (CLI; в `kbc rgb` и пресетах).
- **Новый модуль `src/audio.rs`:**
  - `AudioReactive::open()` — `psimple::Simple` на `Direction::Record`, устройство `@DEFAULT_MONITOR@`, `Spec { F32le, channels: 2, rate: 44100 }`. Ошибка открытия = сессия без PulseAudio/PipeWire → понятный текст.
  - `read_vu_bands()` — читает блок FFT_WINDOW=2048 стерео-сэмплов, окно Ханна, свертка в моно, `rustfft` (plan_fft_forward), лог-шкала 16 полос (20 Гц..nyquist, `f_band_edge`), нормализация по RMS блока (эмпирика), сглаживание attack 0.8/decay 0.45 → `Vec<f32>` 0..1.
  - `audio_frame(base, brightness, vu, led_count, cols)` — «столбики» VU по колонкам: активная полоса (level ≥ 0.03) — спектральный hue 0..260° (красный→синий) с яркостью от уровня/столбца; неактивные — приглушённый базовый цвет (фон). F75 90/22, F99 113/24 — общий.
- **Интеграция host-канал** (ТОЛЬКО USB; на 2.4G/BT — честный отказ):
  - `cmd_daemon` (`src/cli.rs`): если `mode == Audio` и устройство НЕ `ConnectionType::Usb` → ошибка «аудио-реактив доступен только на USB». Persistent-блок при Audio пропускается (в память аудио не пишется). Host-цикл: `AudioReactive::open()` → на каждом тике (frame_ms) `read_vu_bands()` → `audio_frame` → `render_frame`. Пауза перед первым кадром **сокращена 4 с → 2 с** (решение владельца 2026-09-24: музыка включается уже после загрузки сессии, ждать 4 с не нужно; защита 10.94 для не-аудио host-эффектов сохранена).
  - `cmd_rgb` (host-ветка): аналогичный путь для `--mode audio`; на 2.4G-ветке `Audio` → явная ошибка.
- **Исчерпывающие match'и закрыты** (всё — заглушки, в т.ч. честные для недостижимых путей): `AjazzMode::from_our_mode` → Custom; `usb_effect_for_mode`/`wl_effect_for_mode` → 0x03; `host_frame_with` → static base; на этом этапе GUI `mode_to_index` → Custom (в dropdown GUI Audio ещё не добавлялся; в 10.100 добавлены аудио и стили `row`/`bars`).
- **Зависимости:** Cargo.toml — `psimple = libpulse-simple-binding 2` + `pulse = libpulse-binding 2` (алиасы как принято в экосистеме) + `rustfft 6`. flake.nix buildInputs += `pulseaudio`. Fedora: fedora_install.sh + Makefile — `pulseaudio-libs-devel`.
- **Тесты (в src/audio.rs):** лог-границы полос монотонны и ≤ nyquist; тишина → полосы ~0; синус 100 Гц → бас доминирует над ВЧ (втрое+); форма кадра (90 LED). ИТОГ `cargo test --lib` **21/21** (было 17, +4), `cargo check --all-targets` 0 ошибок (только предсуществующие 2 `unused_mut` в keymap_editor.rs — вне скоупа), `cargo build --release` OK.
- **README:** примеры `kbc rgb … --mode audio` и `kbc preset save viz --mode audio… && kbc daemon --preset viz`; `type = "audio"` в списке snake_case; в справочной таблице `rgb` помечен `--mode (вкл. audio — USB-only)`.

### ДОРАБОТКИ 2026-09-24 (2) — лаг, скорость, встроенный пресет
- **ФИКС ЛАГА ~6 с** при повторном воспроизведении (владелец: «музыка идёт, визуализация через ~6 с»): причина — аудио-цикл делал блокирующий `read_vu_bands()` (~46 мс данных) И сверху `sleep(frame_ms)` 40 мс → читали медленнее реального времени, capture-буфер PipeWire копил отставание. Убран `sleep` в аудио-ветках `cmd_rgb`/`cmd_daemon` (темп задаёт блокирующий read монитора); в `BufferAttr` `maxlength=4096`/`minreq=1024` (старьё не копится). Владелец: «по остановке и воспроизведению просто идеально».
- **СКОРОСТЬ/ВЫРАЗИТЕЛЬНОСТЬ** (владелец: «загорается статика, еле что-то делает»): `FFT_WINDOW 2048→1024` (кадр ~23 мс, ~43 FPS, реакция ×2); спад `smoothing 0.45→0.6` (динамика, а не статика); усиление нормировки `rms*14→rms*9`; подъём ВЧ `gain = 1 + (band/VU_BANDS)*1.5`; порог активации `0.03→0.015`; `sat 0.9→1.0`, `l 0.55→0.6`; `F_MIN 20→40 Гц` (при окне 1024 бины ~43 Гц, иначе полоса упиралась в DC). `compute_bands_from_samples` (тесты) синхронизирован.
- **ВСТРОЕННЫЙ ПРЕСЕТ `audio-viz`** (`src/presets.rs`): на этом этапе `builtin_presets()` содержал один `type="audio"`-пресет (цвет `#FF5A00`, яркость 255, скорость 60, кадр 40), `builtin(name)`, `is_builtin(name)`. `PresetStore::list()` добавляет встроенные к файлам (дедуп, сортировка); `has`/`load`/`ensure` учитывают встроенный (файл с тем же именем перекрывает); `remove` встроенного без файла — ошибка. CLI `kbc preset list` помечает `[встроенный]`. На текущем этапе GUI также запускает аудио-реактив и поддерживает стили `row`/`bars`; удаление встроенных пресетов запрещено. Тесты этого этапа: `builtin_audio_viz_visible_and_loadable`, `user_file_overrides_builtin` → `cargo test --lib` **23/23**, check --all-targets 0 ошибок, release OK.

### КАК ПОЛЬЗОВАТЬСЯ
```bash
kbc rgb 258A:010C --mode audio --brightness 200        # живой визуализатор (USB, до Ctrl-C)
kbc daemon --preset audio-viz                          # ВСТРОЕННЫЙ аудио-пресет (у любого, без настройки)
kbc preset save viz --mode audio --brightness 200      # свой пресет TOML (type = "audio")
kbc daemon --preset viz                                # до Ctrl-C; systemd: unit keyboard-center-daemon
```

### ОГРАНИЧЕНИЯ / ЧЕСТНОСТЬ
- Аудио-реактив — **host-фича USB**: у ресивера 2.4G/Ajazz нет «послушать звук», сохранения в память НЕТ (иначе пользователь получил бы статику). GUI dropdown не расширялся — для автозапуска без GUI используйте `kbc daemon`/systemd (Nix home-module `programs.keyboard-center.daemon.enable`).
- Нормализация по RMS блока — эмпирическая (± воспринимаемая громкость через монитор); подстроить пороги (порог активации 0.015, усиление 9.0, `gain` ВЧ, attack/decay) можно в `src/audio.rs` на живом F75. Встроенный `audio-viz` переопределяется своим файлом с тем же именем.
- В NixOS округления mono/stereo монитора зависит от активного sound server (PulseAudio или pipewire-pulse); `@DEFAULT_MONITOR@` резолвится в активный.

### ПРОВЕРКА — ожидается для владельца (F75, USB)
1. `nix develop . --command cargo build --release` (или `make release` на Fedora) — собралось.
2. Поднять звук в системе (музыка/видео) и запустить `kbc rgb 258A:010C --mode audio --brightness 200` → клавиатура показывает VU-полосы (бас слева/низ hue красный → ВЧ справа синий).
3. Тишина/пауза → полосы плавно опадают к фону (базовый цвет приглушённый).
4. `kbc preset save viz --mode audio && kbc daemon --preset viz` и (для автозапуска) пресет `viz` в systemd-home-module.
5. На 2.4G (`--mode audio` с PID ресивера) — честная ошибка «только USB».

### ОСТАЛОСЬ (открыто, вне этой задачи)
- Подстройка порогов/калибровка на живом железе (см. выше); живые отзывы по «уровню» VU.
- (Прочее — по 10.93/10.82/10.68/10.2/10.92: F99/2.4G чек-лист, ремап W-канал, BT, откат nixosniri, Ajazz, релиз.)


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

## 10.97e АВТОНОМНЫЙ АУДИО-РЕАКТИВ (ДЕМОН БЕЗ GUI) (сессия 2026-09-24)

> Запрос владельца: «на USB сделай, чтобы работало с закрытым приложением, насколько это возможно». Аудио уже шло через `kbc daemon` (10.97) + встроенный пресет `audio-viz` + systemd-user unit в `nix/home-module.nix`. Доведена автономность/живучесть.

- **Ленивое открытие + авто-переоткрытие аудио-потока** (`cmd_daemon`, `src/cli.rs`): `AudioReactive::open()` больше НЕ вызывается до цикла (раньше ошибка = выход демона). Теперь поток открывается в цикле при необходимости; при недоступности монитора — сообщение + повтор через 3 с; при ошибке чтения (`read_vu_bands`) — сброс потока и переоткрытие через 2 с. Итог: перезапуск PipeWire/PulseAudio, поздний старт звука или клавиатуры не роняют демон. Сон между кадрами теперь по `if !audio_mode` (было `vu_audio.is_none()`).
- **Systemd/автозапуск**: ничего нового в коде не нужно — в `home-module.nix` уточнено описание `daemon.preset` (для аудио доступны встроенные `"audio-viz"` и `"cava"`). README дополнен блоком «Аудио-реактив в фоне, без GUI».
- Тесты 24/24, `cargo build --release` и debug — OK.
- **Честно:** работает ТОЛЬКО по USB (2.4G/BT — `cmd_daemon` завершается ошибкой «только USB»). На 2.4G live-канал не найден (см. 10.97/раздел «Закрыто навсегда»), «записать аудио в память» невозможно.
- Как включить у себя (nixosniri, вне репо): `programs.keyboard-center.daemon.enable = true; programs.keyboard-center.daemon.preset = "audio-viz";`


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

> **ПОСЛЕ РЕРАЙТА:** полная история сессий сохранена в исходном AGENTS.md (HEAD до 2026-09-23), в т.ч. dated подробности по каждому пункту 10.1–10.93. Точные числа/пакеты/диффы относящиеся к отдельным тестам — в `git log` / `git diff` / `/tmp/opencode/`. Запись 10.94 (фикс «кривая вёрстка», пауза 4 с) следует за этим разделом verbatim из оригинального файла.


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

## 10.98 AJAZZ READ-ONLY DIAG (сессия 2026-09-24)

> Запрос владельца: сделать для Ajazz AK820 Pro диагностику «по максимуму, насколько возможно» — **сначала только USB и только чтение**. Решение владельца: делать через **новую read-only CLI-сабкоманду** `kbc ajazz-diag` (правки `src/cli.rs` + вызовы драйвера разрешены), а скрипт/доки — тонкие обёртки над ней. Вариант «только shell без правок кода» отклонён.

### ЧТО СДЕЛАНО
- **Дефолтный метод трейта** (`src/devices/mod.rs`): `fn diagnostic_dump(&self) -> Result<String>` — по умолчанию `Err(Error::Unsupported(...))`. Внутренний API (см. 10.95), наружу не экспортируется.
- **Реализация для Ajazz** (`src/devices/ajazz_ak820.rs`, `impl KeyboardDevice for AjazzAk820`): `diagnostic_dump` выполняет **только GET-команды** и печатает `ключ=hex` по строке на секцию; ошибки отдельных команд → `ключ_error=…` (дамп продолжается). Секции: `name`, `vid_pid`, `connection`, `device_info`(+`battery=` из байта `[17]`), `led_effect`(+`led_effect_mode=`), `game_mode`, `custom_led` (`GET_CUSTOM_LED_DATA`, `LED_COUNT*4`), `keymap_base`/`keymap_fn`/`keymap_default_base`/`keymap_default_fn` (GET 18/22/31/28), `macros_index` (`GET_MACRO`, 400 б) + `macros_count`/`macro_<id>=delay:keycode:family:press,…` (через `get_macros()`). Никаких догадок о неизвестных полях — только hex + battery + led_effect_mode. Добавлен `use super::hidshell;` (для `hex_dump`).
- **CLI** (`src/cli.rs`): сабкоманда `AjazzDiag(DeviceArg)` («Read-only диагностика Ajazz AK820 Pro (USB)…»), диспатч `open(vid,pid)?.diagnostic_dump()?` → print; строка в шапке-справке CLI.
- **Скрипт** `scripts/ajazz-usb-diag.sh` (executable, `bash -n` ok): read-only авто-детект Ajazz (`0C45:XXXX`), флаги `--device`/`--sg`/`--help`; собирает `kbc list/info/probe/battery/ajazz-diag/remap --list`, `lsusb`, uevent → папка `~/kbc-diag-ajazz-ГГГГММДД-ЧЧММСС/` (файлы `00-обзор` … `09-сводка`, главный — `05-ajazz-diag.txt`) + `~/kbc-diag-ajazz-*.tar.gz`. `kbc`-функция с `--sg`, поиск бинарника через `type -P` (не `command -v` — рекурсия функции).
- **`scripts/collect-data.sh`**: для `--device ajazz` добавлен шаг `[12b]` → `kbc ajazz-diag` в `28-ajazz-diag.txt` (read-only, только GET); строка в сводке.
- **Доки**: новый `docs/ajazz-guide.md` (что реализовано/не проверено, скрипт, формат дампа, транспорт `0xAA`/`0x55`, таблица команд GET/SET, многочанковость, keymap-слот, что искать в дампе, участие); `docs/collect-data.md` — шаг 3.5b + TIP на отдельный скрипт + строка в таблице; `README.md` — пример `kbc ajazz-diag`, строка в справочнике CLI, дерево `docs/`/`scripts/`, таблица документации, блок для владельцев Ajazz.
- **Сборка/тесты**: `cargo check --all-targets` 0 ошибок (только 2 предсуществующих `unused_mut` в `keymap_editor.rs` 308/323 — вне скоупа), `cargo test --lib` **24/24**, `cargo build --release` OK; `kbc --help` показывает `ajazz-diag`.

### ЧЕСТНОСТЬ / ОГРАНИЧЕНИЯ
- Живого Ajazz нет — `diagnostic_dump` на железе НЕ проверен. Все команды read-only; **write-тестов для Ajazz пока НЕТ** (в отличие от F99 `--write-tests`). `kbc ajazz-diag` не использует SET-команды (34/35/36/37/38), `SET_FACTORY_RESET` (15), TFT (80/81) и control-канал `0xFF13`.
- Некоторые секции могут вернуть `*_error=Protocol` — это ожидаемый и ценный результат (канал/прошивка отличаются от ресёрча wsclx), а не «провал».

### ОСТАЛОСЬ (открыто, вне этой задачи)
- Живой прогон `scripts/ajazz-usb-diag.sh` владельцем Ajazz; по дампу — правки драйвера/сканера; позже (отдельно) — write-тесты TFT/макросов/keymap с откатом.
- (Прочее — по 10.93/10.82/10.68/10.2/10.92: F99/2.4G чек-лист, ремап W-канал, BT, откат nixosniri, релиз.)


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

## 10.99 АУДИО-РЕАКТИВ ПАРАМЕТРИЗАЦИЯ (ФАЗА 1) (сессия 2026-09-24)

> Историческая запись Phase 1: реализованные по требованию владельца настройки аудио-визуализатора (`RgbMode::Audio`) — сохранение в пресеты TOML, настройка через GUI, CLI-флаги. На этом этапе `cava`-стиль (`style = "bars"`) был отложен до Phase 2; Phase 2 реализован в 10.100.

### РЕШЕНИЯ ВЛАДЕЛЬЦА (опрос перед работой)
1. **Дефолт чувствительности:** оставить текущий `SENSITIVITY=0.75` (вид не меняется).
2. **Объём Phase 1:** параметры + GUI + пресеты; `cava` тогда был отложен до Phase 2, которая реализована в 10.100.
3. **Формат TOML:** секция `[audio]` в пресете (обратно-совместимо: старые файлы без `[audio]` читаются, дефолты подставляются).

### ЧТО СДЕЛАНО
- **`src/presets.rs`**: новая структура `AudioParams { sensitivity: f32, color_low: String, color_high: String }` с дефолтами, сохраняющими текущий вид (`sensitivity=0.75`, `color_low="#33FF33"` = hsl(120,1,0.6), `color_high="#3333FF"` = hsl(240,1,0.6)). В `ModeParams` добавлено опциональное поле `audio: Option<AudioParams>` с `#[serde(default, skip_serializing_if = "Option::is_none")]` — секция `[audio]` в TOML появляется только у аудио-пресетов. Встроенный пресет `audio-viz` получает `audio: Some(AudioParams::default())`. Добавлен helper `audio_params()` (возвращает дефолты, если секции нет). Тесты: `audio_params_roundtrip` (сериализация/десериализация, отсутствие секции у не-аудио режимов), обратная совместимость проверена.
- **`src/audio.rs`**: убран `const SENSITIVITY`; добавлен `pub const DEFAULT_SENSITIVITY: f32 = 0.75` и `pub fn smoothing_for_speed(speed: u8) -> f32` (map 0..255 → 0.2..0.95, скорость 60 → 0.6). `AudioReactive` теперь хранит `sensitivity` и `smoothing`; `open()` делегирует в `open_with(DEFAULT_SENSITIVITY, 60)`. `read_vu_bands` использует `self.sensitivity`. `audio_frame` принимает `color_low: Color, color_high: Color` вместо захардкоженного hue 120→240 — интерполяция линейная в RGB по позиции полосы. Удалена мёртвая `hsl_to_rgb`. `compute_bands_from_samples` использует `DEFAULT_SENSITIVITY`. Тесты `audio_frame_*` обновлены под новую сигнатуру.
- **`src/cli.rs`**: в `RgbArgs` и `DaemonArgs` добавлены `--color-low`, `--color-high` (hex), `--sensitivity` (f32 0.1..2.0). В `PresetSave` — те же флаги. В `cmd_rgb` аудио-ветка парсит флаги, вызывает `AudioReactive::open_with(sens, speed)`, `audio_frame(..., low, high)`. В `cmd_daemon` параметры берутся из пресета (`preset.params.audio_params()`) + оверрайды CLI; `open_with(audio_sens, params.speed)` + `audio_frame(..., audio_low, audio_high)`. В `PresetAction::Save` — аудио-оверрайды при сохранении пресета.
- **`src/gui/rgb_studio.rs`**: после блока «Скорость» добавлена сворачиваемая секция «Аудио-реактив (USB)» (изначально скрыта): два поля `#RRGGBB` (низы/верхи), слайдер чувствительности 0.10..2.00 с надписью значения. Видимость переключается через `mode_dd.connect_selected_notify` — видна только при `RgbMode::Audio`. В `apply` (аудио-ветка): парсинг GUI-виджетов, `open_with`, `audio_frame` с цветами. В `pf_save_btn`: при `mode==Audio` сохраняет `audio: Some(AudioParams{...})`. В `pf_apply`: при загрузке аудио-пресета заполняет GUI-виджеты из `preset.params.audio`. Подсказка в секции: «Спектр системного звука по USB: реагирует верхний ряд (Esc+F1..F12), буквы — базовым цветом. В память клавиатуры не пишется.»
- **Проверки**: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в `keymap_editor.rs`), `cargo test --lib` — **25/25** (было 24, +1 новый `audio_params_roundtrip`), `cargo build --release` — OK.
- **CLI примеры**: 
  - `kbc rgb 258A:010C --mode audio --color-low #00C800 --color-high #2864FF --sensitivity 1.2 --brightness 200`
  - `kbc preset save my-viz --mode audio --color "#FF5A00" --brightness 255 --speed 60 --color-low "#33FF33" --color-high "#3333FF" --sensitivity 0.75`
  - `kbc daemon --preset my-viz` (системд: `programs.keyboard-center.daemon.preset = "my-viz"`)

### ОГРАНИЧЕНИЯ / ЧЕСТНОСТЬ
- **Только USB**: у 2.4G/Ajazz нет аудио-монитора — там `open` вернёт ошибку / честный отказ (как раньше).
- **Phase 2 (cava) — РЕАЛИЗОВАН** (см. 10.100 ниже): добавлены `AudioStyle::Bars`, точная геометрия F75/F99, выбор `--style row|bars` в CLI, переключатель в GUI и встроенный пресет `cava`.
- **Скорость ↔ сглаживание**: `smoothing_for_speed(60) = 0.6` — текущий дефолт сохранён. Выше скорость → быстрее реакция (attack 1.0, decay = smoothing).
- **Обратная совместимость**: старые TOML-пресеты без `[audio]` или без `style` работают; `audio-viz` остаётся в режиме `row`, дефолт — `row`.

### ОСТАЛОСЬ (будущее)
- Калибровка порогов `ACTIVATION` / `RMS_FLOOR` / `DEFAULT_SENSITIVITY` на живом железе под разные жанры музыки.
- Визуальная проверка `cava` на живых F75/F99.


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

## 10.100 АУДИО-РЕАКТИВ PHASE 2: CAVA-СТИЛЬ (BARS) (сессия 2026-09-25)

> Реализация `style = "bars"` (cava-стиль) для аудио-реактива. Запрос владельца: «сделай столбики как в cava». Геометрия LED сверена с `SinowealthKeyboard10cDevices.cpp` и Keyboard Layout Manager из OpenRGB.

### РЕШЕНИЯ ВЛАДЕЛЬЦА
1. **Геометрия:** использовать реальные раскладки OpenRGB для F75 и F99, а не сопоставление LED-индекса с номером слота.
2. **Стиль:** добавить `AudioStyle::Bars` в `AudioParams.style`; дефолт `Row` сохраняет обратную совместимость.
3. **CLI/GUI:** `--style row|bars` в CLI и переключатель стиля в GUI; TOML хранит `style = "row" | "bars"`.
4. **Встроенный пресет:** сохранить `audio-viz` как `row`, добавить отдельный встроенный `cava` со стилем `bars`.

### ГЕОМЕТРИЯ
- **F75:** 90 LED, 80 известных позиций, 15 колонок; `LED_GEOMETRY` содержит 90 элементов, неизвестные позиции — `(0xFF, 0xFF)`.
- **F99:** 113 LED, 99 известных позиций, 19 колонок; `LED_GEOMETRY` содержит 113 элементов. Известные позиции NumPad также участвуют в столбиках. LED индикаторов, лого и боковой подсветки без позиции остаются базовым цветом.
- В OpenRGB `0` — реальный LED ESC, а не маркер неизвестной позиции.

### ЧТО СДЕЛАНО
- **`src/devices/aula_f75.rs` / `aula_f99.rs`**: добавлены точные `LED_GEOMETRY` из OpenRGB.
- **`src/devices/mod.rs`**: добавлена `audio_led_geometry(led_count)` и тесты размеров/уникальности геометрии.
- **`src/presets.rs`**: добавлены `AudioStyle::{Row, Bars}`, сериализация стиля и встроенный `cava`; `audio-viz` остаётся совместимым `row`-пресетом.
- **`src/audio.rs`**: `audio_frame` принимает стиль и геометрию; `audio_frame_row` сохраняет верхний ряд; `audio_frame_bars` группирует все известные позиции по колонкам, зажигает их снизу вверх и интерполирует цвет от `color_low` к `color_high`. При пустой или состоящей только из sentinel-координат геометрии Bars безопасно возвращается в Row.
- **`src/cli.rs`**: добавлены `--style row|bars` для `rgb`, `daemon` и сохранения/применения аудио-пресетов; неверный стиль отклоняется с понятной ошибкой.
- **`src/gui/rgb_studio.rs`**: добавлен выбор «Верхний ряд»/«Столбики (как cava)», синхронизация с пресетами и передача геометрии в аудио-поток.
- **Тесты:** добавлены проверки Bars, fallback без геометрии, сериализации стиля и встроенного `cava`; `cargo test --lib` — **31/31 passed**.

### CLI ПРИМЕРЫ
```bash
kbc rgb 258A:010C --mode audio --style bars --brightness 200
kbc daemon --preset audio-viz
kbc daemon --preset cava
kbc preset save my-bars --mode audio --style bars --brightness 200
```

### ОГРАНИЧЕНИЯ / ЧЕСТНОСТЬ
- Аудио-реактив работает только по USB; на 2.4G/BT — честный отказ.
- `audio-viz` и `cava` зашиты в бинарник, поэтому доступны любой установленной сборке без внешних TOML-файлов; одноимённый пользовательский TOML перекрывает встроенный.
- В Bars участвуют все известные позиции F75/F99, включая NumPad у F99; LED без известной позиции сохраняют базовый цвет.
- F99 keymap write остаётся `Unsupported`; визуализация Bars от этого не зависит.
- Визуальная проверка на живом F75/F99 и калибровка порогов остаются отдельными задачами.


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

## 10.102 ПОДГОТОВКА ВЕРСИИ 0.4.0 (сессия 2026-09-25)

- Версия пакета повышена с `0.3.0` до `0.4.0` из-за новой пользовательской функциональности: аудио-стили `row`/`bars`, встроенный пресет `cava`, выбор стиля в CLI/GUI/TOML и геометрия F75/F99.
- Версия синхронизирована в `Cargo.toml`, `Cargo.lock`, `flake.nix` и бейдже README.
- В README добавлен раздел «Что нового в 0.4.0» с описанием аудио-реактива, встроенных пресетов и обновлённой документации.
- Сборка и проверки выполнены через `nix develop .`: `cargo check --all-targets` — 0 ошибок, `cargo test --lib` — 31/31, `cargo build --release` — успешно; пакет `nix build .#keyboard-center` — успешно.
- `target/release/kbc --version` и `result/bin/kbc --version` подтверждают: `kbc 0.4.0`. В GUI версия берётся из `crate::VERSION`.
- Коммит не создавался.


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

## 10.103 ДОСТУП К HIDRAW / UDEV-ПРАВИЛА (сессия 2026-09-28)

> Жалоба владельца: «при сканировании устройства ошибка hid — отказано в доступе» (на живом F75, USB, PID `258A:010C`). Диагностика велась на хосте `darkside_pc` (вне репозитория — `~/nixosniri`, работа только с явного разрешения владельца). Решение владельца: «подключим в nixosniri на pc хосте».

### ДИАГНОСТИКА (что было на самом деле)

- `/dev/hidraw0` и `/dev/hidraw1` — `crw------- root root` (режим 0600), оба интерфейса F75 (`HID_PHYS=usb-0000:07:00.3-2.2`).
- Ошибка приложения: `hidapi error: Failed to open a device with path '/dev/hidraw1': Permission denied` (то же для `info`, `battery`, `rgb`). Сканирование (`kbc list`) при этом работает — hidapi перечисляет устройства без открытия.
- Группы `plugdev` в системе **нет**; пользователь `darkside` в `users/wheel/disk/video/networkmanager/libvirtd/kvm` — не в `plugdev`.
- `99-keyboard-center.rules` в `/etc/udev/rules.d` **отсутствует** → модуль проекта в системном флейке не включён (временные правки `~/nixosniri` из 10.2 к этому моменту уже были откачены).

### ГЛАВНАЯ НАХОДКА: ПРАВИЛА БЫЛИ СЛОМАНЫ (`ATTR{}` вместо `ATTRS{}`)

- В `99-keyboard-center.rules` использовался матч `ATTR{idVendor}`/`ATTR{idProduct}` при `SUBSYSTEM=="hidraw"`. У hidraw-устройства **в собственном sysfs-каталоге атрибута `idVendor` нет**: `/sys/class/hidraw/hidraw0/` содержит только `dev, device, power, subsystem, uevent`, а каталог устройства (`.../0003:258A:010C.0006/`) — `country, hidraw, input, modalias, uevent, report_descriptor`. `idVendor` лежит на 4+ уровня выше, в USB-родителе: `…/usb3/3-2/idVendor`.
- Следствие: `ATTR{}` на hidraw **не срабатывает никогда**, правило молча игнорируется, `MODE`/`GROUP`/`TAG` не применяются — даже если файл установлен. То есть просто скопировать правила в систему было бы бессмысленно.
- Подтверждение из живой системы: реальные рабочие правила для hidraw используют `ATTRS` — `/etc/udev/rules.d/60-steam-input.rules:8` (`KERNEL=="hidraw*", ATTRS{idVendor}=="28de", MODE="0660", TAG+="uaccess"`), а также `60-persistent-hidraw.rules:13` (`ATTRS{bInterfaceNumber}`). Плюс проверка `udevadm verify 99-keyboard-center.rules` (systemd 260) — синтаксис чистый, единственное замечание `Failed to resolve group 'plugdev'` (группы ещё нет).
- **Фикс:** `ATTR{` → `ATTRS{` во всех 15 правилах файла; шапка файла переписана — объяснение, почему именно `ATTRS` (со ссылкой на steam-правила), отдельная инструкция для обычных Linux-дистрибутивов (`cp` + `udevadm reload` + `usermod -aG plugdev` + перелогин) и примечание про NixOS. Мёртвые строки-заглушки `GROUP="plugdev"` / `MODE="0664"` в начале файла (udev их не подставляет) закомментированы. Коммит локальный: `c424502` — **без пуша** (решение владельца), GitLab-репозиторий остался на `1ea1d62`.

### NIXOS: РУЧНАЯ УСТАНОВКА ПРАВИЛ НЕВОЗМОЖНА

- `/etc/udev/rules.d` — **симлинк** на `/etc/static/udev/rules.d`, а `/etc/static` — read-only bind-mount стора: запись даёт `EROFS` («Файловая система доступна только для чтения»), проверено. Поэтому инструкция `sudo cp … /etc/udev/rules.d/` из шапки правил на NixOS нерабочая.
- `/etc/group` — обычный (не read-only) файл, но он генерируется заново при каждой загрузке → `groupadd`/`usermod` не переживают ребут. `/etc/sudoers.d` на NixOS вообще отсутствует.
- Вывод: единственный постоянный путь — конфигурация: `services.udev.packages` (наш `nixos-module.nix` это уже делает) либо `services.udev.extraRules`.

### ЧТО СДЕЛАНО В `~/nixosniri` (PC-хост, по решению владельца)

- `flake.nix`: input `keyboard-center = { url = "gitlab:dark_siders/keyboard-control"; inputs.nixpkgs.follows = "nixpkgs"; }` (репозиторий публичный), добавлен в деструктуризацию `outputs` и в список модулей `keyboard-center.nixosModules.default`. `nix flake update keyboard-center` → lock обновлён, input зафиксирован на `1ea1d62`.
- `hosts/darkside_pc/default.nix`: `programs.keyboard-center.enable = true;`, `programs.keyboard-center.enableUdevRules = false;` и **страховка** `services.udev.extraRules` — 4 правила с `ATTRS{idVendor}` (`258a`, `3554`, `0c45`, `0328`) → `MODE="0664", GROUP="plugdev", TAG+="uaccess", TAG+="udev-acl"`. Страховка нужна потому, что вход идёт с GitLab, а фикс `ATTRS` не запушен; правила из пакета не ставятся, дублирования нет. Комментарий в файле объясняет, что после пуша фикса строки можно удалить, а `enableUdevRules` вернуть в `true`. Файл собирается в `etc/udev/rules.d/99-local.rules` (номер 99 → грузится после системных правил, поэтому перебивает их).
- Строку `users.users.darkside.extraGroups = [ "plugdev" ]` в хосте **убрали как дубль** — группа уже прописана глобально в `modules/core/userset.nix:18` (просто текущий конфиг на машине давно не пересобирался, поэтому группа и не существовала). Побочно: `plugdev` в обоих хостах, модуль пока включён только на PC.
- Сборка `nix build .#nixosConfigurations.darkside_pc.config.system.build.toplevel` — успешно; в closure системы пакет `keyboard-center-0.4.0`; `programs.keyboard-center.enable = true`; `extraRules` на месте.
- **Не сделано (нужен sudo):** `sudo nixos-rebuild switch --flake .#darkside_pc` — владелец выполняет сам, затем перелогин (вступление в `plugdev`) и проверка `getent group plugdev` / `ls -l /dev/hidraw0` / `kbc info 258A:010C` без sudo.

### ПРО `./target/` И GC (ответ на вопрос владельца «как запускать через ./target/… после перезагрузки»)

- Отдельная проблема от прав: `nix-gc.timer` в `~/nixosniri` — еженедельно (понедельник 00:00), а `target/` не является GC-корнем → библиотеки стора удаляются, бинарь перестаёт запускаться (ошибка загрузки ELF).
- Проверено: все 14 store-путей, нужных `target/release/kbc` (`ldd`: pulseaudio, systemd-minimal-libs, libgcc, glibc, dbus-1-lib, flac/vorbis/opus/libsndfile/mpg123 и др.), входят в requisites новой системы → после `switch` бинари из `target/` переживут и ребут, и GC. Отдельный `nix build .#keyboard-center` не потребовался.
- Альтернатива без всяких пересборок: `sudo`-запуск или разовое `sudo chmod 664 /dev/hidraw0 /dev/hidraw1` после ребута (удобно для отладки, но не переживает переподключение).


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

## 10.104 ИКОНКА ПРИЛОЖЕНИЯ В МЕНЮ (сессия 2026-09-28)

> Жалоба владельца: модуль подключён в `~/nixosniri` (NixOS-модуль, `environment.systemPackages`), приложение есть в списке, но «само название без иконки». Разрешение владельца на правку `flake.nix` получено явно.

### ДИАГНОСТИКА (живая система, read-only)

- `.desktop` находится лаунчером через `XDG_DATA_DIRS` → `/run/current-system/sw/share/applications/`, где лежит симлинк `kgswh2nnv321cbn30683a05wd3sq29r4-keyboard-center.desktop` → store-путь пакета. Содержимое файла валидно (`Icon=keyboard-center`), имя — нет.
- Иконка в системе: `/run/current-system/sw/share/icons/hicolor/scalable/apps/if0rp850pqn0vdppw9m56kgq23dgrada-keyboard-center.svg` — имя с префиксом-хэшем, поэтому `Icon=keyboard-center` не резолвится ни в одном лаунчере (noctalia/rofi/GTK).
- Udev-правило установилось ПРАВИЛЬНО (`lib/udev/rules.d/99-keyboard-center.rules`) — там в `cp` destination был указан явно, basename не использовался.

### ПРИЧИНА

`postInstall` в `flake.nix` делал `cp ${./data/keyboard-center.desktop} $out/share/applications/`. При интерполяции пути файла из дерева флейка Nix 2.34 создаёт store-путь `<хэш>-keyboard-center.desktop` (баснейм = полное store-имя), а `cp` с destination-каталогом копирует файл под баснеймом источника → в `$out` файл получался `kgswh2nnv321cbn30683a05wd3sq29r4-keyboard-center.desktop`. Лаунчер имя `.desktop` не учитывает (приложение видно), но `Icon=keyboard-center` резолвится по ИМЕНИ файла иконки — и не находится.

### ФИКС (`flake.nix`, `postInstall`)

Все три файла ставятся через `install -Dm444` с явными именами приёмника (независимо от того, как Nix назовёт store-путь исходника):

- `install -Dm444 <desktop> $out/share/applications/keyboard-center.desktop`
- `install -Dm444 <svg> $out/share/icons/hicolor/scalable/apps/keyboard-center.svg`
- `install -Dm444 <rules> $out/lib/udev/rules.d/99-keyboard-center.rules`

В коде оставлен комментарий-предупреждение, почему `cp` в каталог использовать нельзя. Общее правило для nixpkgs: **никогда не `cp <исходник-из-флейка> $out/share/<каталог>/`** — только `install -D`/`installShellFiles` с явным именем.

### ПРОВЕРКА

- `nix build .#keyboard-center` — успешно; `result/share/applications/keyboard-center.desktop` (518 б) и `result/share/icons/hicolor/scalable/apps/keyboard-center.svg` (2597 б) — имена правильные, размеры совпадают с исходниками в `data/`; `result/bin/kbc --version` = `0.4.0`.

### ОСТАЛОСЬ (вне репозитория, за владельцем)

- Вход в `~/nixosniri` идёт с GitLab (`gitlab:dark_siders/keyboard-control`, зафиксирован на `1ea1d62`), фикс пока не запушен → до коммита/пуша в системе останется старая версия пакета. Пути: запушить фикс → `nix flake update keyboard-center` → `sudo nixos-rebuild switch --flake .#darkside_pc`; либо временно указать в `~/nixosniri` локальный путь `path:/home/darkside/keyboard-control`.
- Коммит/пуш — только по явной просьбе владельца.


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

## 10.105 УДАЛЕНИЕ НА FEDORA / БЕЗ NIX (сессия 2026-09-28)

> Запрос владельца: «может напишешь скрипт аналогичный установке, только под удаление» (контекст: на Fedora стояла старая версия, `make release` падал из-за прав на `target/`, `fedora_install.sh --uninstall` сносит только «своё» в одном режиме). Решение владельца по объёму: сносить **и системную, и пользовательскую** установки, данные (`~/.config/keyboard-center`) — только под флагом.

### НОВЫЙ СКРИПТ `fedora_uninstall.sh`

- Удаляет всегда: `/usr/local/bin/{kbc,keyboard-center}`, `/usr/share/applications/keyboard-center.desktop`, `/usr/share/icons/hicolor/scalable/apps/keyboard-center.svg`, `~/.local/bin/{kbc,keyboard-center}`, `~/.local/share/applications/keyboard-center.desktop`, `~/.local/share/icons/hicolor/scalable/apps/keyboard-center.svg`.
- Udev-правило `/etc/udev/rules.d/99-keyboard-center.rules` + `udevadm control --reload-rules && udevadm trigger` (если не `--keep-rules`).
- Флаги: `--purge` — дополнительно `~/.config/keyboard-center` (пресеты) и `~/.cache/icons`, `~/.cache/gtk-4.0`; `--keep-rules`; `--dry-run` (план действий, ничего не меняя).
- Перед удалением скрипт проверяет наличие системных файлов: если они есть, а `uid != 0` — понятная ошибка с подсказкой `sudo ./fedora_uninstall.sh`; если ничего не найдено — сообщает и всё равно выполняет `rm -f` (идемпотентно).
- Стиль как у `fedora_install.sh`: `set -euo pipefail`, функция `run` с поддержкой `--dry-run`, русские комментарии, `--help` через `sed -n '2,19p'` (диапазон подобран так, чтобы не печатать shebang — в `fedora_install.sh` тот же баг исправлен, было `sed -n '1,16p'`).

### ЧТО ОБНОВЛЕНО ВМЕСТЕ

- `Makefile`: `UNINSTALLER ?= ./fedora_uninstall.sh`, `UNINSTALL_FLAGS ?=`, цель `uninstall` → `$(UNINSTALLER) $(UNINSTALL_FLAGS)`, две строки в `help`. Установочные таргеты не тронуты.
- `fedora_install.sh`: в шапке — блок про `fedora_uninstall.sh` (+ пример), в выводе блока `--uninstall` — подсказка «для полного удаления: sudo ./fedora_uninstall.sh», диапазон `--help` исправлен (`1,16p` → `2,21p`).
- `README.md`: заголовок «Вариант 5» дополнен, новый «Шаг 6. Удаление» (dry-run/sudo/purge), таблица make-таргетов (`make uninstall`, `make uninstall UNINSTALL_FLAGS=--purge`), отдельная строка с параметрами `fedora_uninstall.sh`, дерево проекта, пункт Roadmap.
- `.gitlab-ci.yml`: в джобе `fedora-build` добавлены две строки `bash -n` для установочных и диагностических скриптов (ловит опечатки до merge). ВНИМАНИЕ: dnf-строка с пакетами остаётся одной строкой без `\`-переносов (10.58).
- `AGENTS.md`: сводка Fedora + запись 10.105 (этот блок).

### ПРОВЕРКИ

- `bash -n` для всех шести скриптов (`fedora_install.sh`, `fedora_uninstall.sh`, `scripts/{collect-data,ajazz-usb-diag,f75-usb-rev,f99-usb-rev}.sh`) — OK.
- `./fedora_uninstall.sh --help`, `--dry-run`, `--dry-run --purge` — OK; `make help`, `make -n uninstall`, `make -n uninstall UNINSTALL_FLAGS=--purge` — OK.
- Живой прогон удаления на Fedora делает владелец (на NixOS-хосте файлов нет — скрипт корректно отвечает «ничего не найдено»).
- ВНИМАНИЕ (не связано с этой правкой): на Fedora `make release` падал с `Permission denied` на `target/release/libkeyboard_center.d` — каталог `target/` принадлежит root (сборка шла через `sudo`). Решение: `sudo chown -R $USER:$USER target` или `sudo rm -rf target`. Рекомендация владельцу: собирать от пользователя, sudo — только на `fedora_install.sh --system/--udev-only`.

### ОСТАЛОСЬ

- **Коммит и пуш — делает владелец сам** (незапушены: `c424502` udev `ATTRS` 10.103, фикс иконки 10.104, удалялка 10.105).
- Для NixOS-хоста после пуша: `nix flake update keyboard-center` + `sudo nixos-rebuild switch --flake .#darkside_pc` (иначе в системе останется пакет с битым именем иконки, 10.104).


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

## 10.106 ЖИВАЯ РАЗВЕДКА AJAZZ AK820 PRO (сессия 2026-09-28)

> Контекст: до 2026-09-28 поддержка Ajazz держалась **только на ресёрче** (wsclx/`AjazzAk820ProUnofficialWebdriver`), живого железа у владельца не было. Здесь железо появилось — Ajazz на Fedora-машине. Цель сессии: найти рабочий HID-транспорт и подтвердить его, не трогая production-код.

### КАК НАЙТИ УЗЕЛ (главный практический вывод)

`/dev/hidrawN` **хардкодить нельзя**: нумерация сдвигается при каждом переподключении (в сессии узел успело сменить номер с `hidraw14` на `hidraw13`). Искать по `report_descriptor` — наличие `06 13 FF` (usage page `0xFF13`):

```bash
for d in /sys/class/hidraw/hidraw*; do
  python3 -c "import sys;print(open(sys.argv[1],'rb').read().hex().upper())" \
    "$d/device/report_descriptor" | grep -q '0613FF' && echo "${d##*/}"
done
```

У Ajazz 4 HID-интерфейса (`hidraw10`…`hidraw13` на момент сессии), из них `0xFF13` — последний.

### ДЕСКРИПТОР `0xFF13`

```
0613ff0901a101150026ff007508954009028102090391020a00ffb102c0
```

Usage Page `FF13`, Usage `0001`, **Input 64 б** (`81 02`), **Output 64 б** (`91 02`), **Feature 64 б** (`B1 02`). Отчёты **unnumbered** — Report ID в дескрипторе отсутствует, но HIDAPI всё равно трактует `buf[0]` как report ID, поэтому в буфер кладётся `0x00` + 64 байта (итого 65 байт = 130 hex-символов).

### ЧТО ПОДТВЕРЖДЕНО ЖИВЬЁМ

- `feature-send` 64 байт на `0xFF13` **уходит без EPIPE** — транспорт принимает. У `0xFF68` Feature report **нет вовсе** → EPIPE; в этом и была причина старого впечатления, что «FF13 не поддерживает Feature» — команды просто уходили в FF68-узел.
- `output-send` на `0xFF68` — **таймаут**. Вероятная причина: дескриптор `0668ff0961a1010962150026ff009540750881020963150026ff0096001075089102c0` объявляет **Output Report Count = `96 00` = 4096 байт**, тогда как firmware общается 64-байтными пакетами.

### ПРОТОКОЛ `0xFF13` (источник: MemerGamer `AjazzAk820ProUnofficialWebdriver`)

Три шага: `RESET` → `CONFIGURE <команда>` → data-пакет.

- Преамбула: `p[0]=0x04`, `p[1]=<команда>`, `p[8]=0x01` (для sleep дополнительно `p[2]=0x01`).
- Команды: `0x18` RESET, `0x13` MODE (подсветка), `0x28` TIME (часы), `0x17` SLEEP, `0x02` SAVE.
- Data-пакет подсветки: `p[0]=<эффект>`, `p[1..3]=RGB`, `p[8]=rainbow`, `p[9]=яркость`, `p[10]=скорость`, `p[11]=direction`, `p[14]=0x55`, `p[15]=0xAA`.
- Data-пакеты времени/sleep: `p[0..1]`, маркер `p[62]=0xAA`, `p[63]=0x55`.
- Эффекты `0x00`…`0x13` (20 шт.): `00` Off, `01` Static, `02` Single On, `03` Single Off, `04` Glittering, `05` Falling, `06` Colourful, `07` Breath, `08` Spectrum, `09` Outward, `0a` Scrolling, `0b` Rolling, `0c` Rotating, `0d` Explode, `0e` Launch, `0f` Ripples, `10` Flowing, `11` Pulsating, `12` Tilt, `13` Shuttle.
- **Яркость и скорость — диапазон 0..5, а НЕ 0..255** (`MAX_BRIGHTNESS = 5`).

### ПОЧЕМУ ПЕРВЫЕ ПОПЫТКИ НЕ ЗАСВЕТИЛИ (разбор собственной ошибки)

Пакет был собран неверно по трём пунктам, и каждая ошибка маскировала следующую:

1. **Команда была в `p[0]` вместо `p[1]`.** Поле `p[0]` — это код эффекта. То есть фактически отправлялся эффект `0x13` (`Shuttle`), у которого первым байтом цвета стоял `R=0x01` — **≈0,4% яркости, визуально чёрный**.
2. **Не отправлялась преамбула `0x04 <команда>`.** Без неё data-пакет молча игнорируется — канал подсветки не открыт.
3. **Яркость ставилась `0xFF`.** По протоколу максимум — `0x05`; `0xFF` уходит за диапазон и даёт ноль.

Побочно: первая же отправка с `p[0]=0x13` дала заведомо ложное впечатление «FF13 не работает», хотя транспорт был исправен с самого начала.

### СОСТОЯНИЕ НА КОНЕЦ СЕССИИ

- Транспорт `0xFF13` Feature — **подтверждён на живом железе**.
- Корректная последовательность `04 18` → `04 13` → `01 FF 00 00 …` **ещё не отправлена** — ждём визуального результата (следующий шаг владельца).

### ЧИСТО ИНФОРМАЦИОННО (не ошибка)

- Разбор `/tmp/opencode/ak820.pcap`: старый захват (220 с) содержал только энумерацию, LED SET_REPORT и обычные 8-байтовые клавиатурные Input — **ни одного 64-байтового report от `kbc`**, потому что команды шли не в то окно (переподключение).
- Новый захват показал `0C45:FDFD` «Microdia 2.4G Dongle» — устройство с 5 HID-интерфейсами, **все interrupt-IN, ноль Output/Feature, ни `FF13`, ни `FF68`**. Это **не клавиатура**; оно присутствует на шине постоянно (в обоих захватах) и при разборе usbmon легко спутывается с Ajazz по общему VID `0c45`.
- `kbc` на Fedora-машине владельца (`/usr/local/bin/kbc`) поддерживает `--path` и подходит для этих тестов.
- Формат pcap: linktype 220, сырой 64-байтовый usbmon-заголовок; существующий парсер `usbmon_ajazz.py` принимает текстовый usbmon, а не бинарный pcap.

### ЧТО НЕ ТРОГАЛИ

Production-код не менялся. Все действия — read-only диагностика плюс экспериментальные записи в канал подсветки (без `SAVE 0x02`, то есть без записи в энергонезависимую память).


10.111 — 2026-10-01 — сворачивание GUI в системный трей (SNI). GTK4/Libadwaita трая не имеют; реализация на `ksni 0.3` (feature `blocking`): org.kde.StatusNotifierItem + com.canonical.dbusmenu, чистый D-Bus поверх zbus, работает под Wayland/Niri. Решение владельца: крестик = в трей (но с настройкой), только сворачивание (без заряда/иконки батареи); пункты меню «Скрыть/Выход» даже без трея — НЕ делать; отключение флагом `KEYBOARD_CENTER_NO_TRAY` — ок.
- **`src/gui/tray.rs`** (новый): `TrayItem` (impl `ksni::Tray`): `id="keyboard-center"`, `icon_name="keyboard-center"` (hicolor), `category=ApplicationStatus`; клик по иконке/пункт «Показать/Скрыть окно» → `TrayCmd::ToggleWindow`, пункт «Выход» → `RealQuit`; `watcher_offline=true` (сервис живёт, если хост пропал). Связь с главным GTK-потоком — через `std::sync::mpsc`, спавн в фоновом потоке. **Активность трея определяется результатом `spawn()`** (не `watcher_online` — он на старте не вызывается!): `Ok` → `Available`, `Err` → `Unavailable`.
- **`src/gui/app.rs`**: `build_ui` — канал `(tray_tx, tray_rx)`; если `KEYBOARD_CENTER_NO_TRAY` не задан — `tray::spawn(tx)` + `glib::timeout_add_local(200 мс)` опрос `tray_rx` в главном цикле (ToggleWindow → is_visible? hide : present; RealQuit → `real_quit=true` + `win.close()`; Available/Unavailable → флаг `tray_online`). Явное `gio::SimpleAction "quit"` (header-меню ссылается на `app.quit`; встроенного в GTK4 нет) → `real_quit=true` + `win.close()`. `setup_close_autosave` получил флаги `tray_online`/`real_quit`: при активном трее и НЕ выходе close-request скрывает окно (`set_visible(false)`), возвращая `Propagation::Stop` (запись в память НЕ выполняется — эффекты идут host-рендерингом); реальный выход (трей/меню) = автосейв как раньше. Deprecated `hide()` (4.10) → `set_visible(false)`.
- **Cargo.toml**: `ksni = { version = "0.3", features = ["blocking"] }`. Системных зависимостей не добавлено (zbus уже в дереве через транзитивы? нет — ksni тянет zbus сам).
- Проверки: `cargo check --all-targets` — 0 ошибок (2 предсуществующих `unused_mut` в keymap_editor.rs), `cargo test --lib` — 31/31, `cargo build --release` — OK.
- Проверить у владельца на NixOS+Noctalia: крестик сворачивает, клик по иконке/меню возвращает окно, «Выход» закрывает с автосейвом; при отсутствии SNI-хоста — прежнее поведение.

10.112 — 2026-10-11 — **живая проверка 0xFF13 framing B на 0C45:8009.** По результатам прогона `scripts/ajazz-usb-diag.sh`+доработок `ajazz-usb-rev.sh` (framing a/b, финал `0xF0`, десятичное время, graceful SIGPIPE) подтверждено: 0xFF13 — unnumbered feature 64-б, нужен **65-б буфер (report id `0x00` + 64)**. T2 `b:0xF0` зажёг Static, T4 время подтвердилось. Причина бага драйвера: `hidapi::send_feature_report` трактует `buf[0]` как номер report-а → пакет с `buf[0]=0x04` уходил как report id 4 и игнорировался. Исправлено в драйвере: `send_ctl` формирует `ctl_frame` `[0x00]++pkt` (65 б); `sync_system_time` и `set_sleep_timer` переведены на `self::send_ctl()`. Добавлен юнит-тест `ctl_frame_prepends_report_id_zero`. Тесты lib — 32/32, `cargo build --release` OK. Часть дефектов (`kbc rgb --color` через `render_frame` → мёртвый 0xFF68, `is_connected` через `get_device_info`) оставлена вне этого fix.

---

## 10.107 ЕДИНЫЙ FEDORA.SH (сессия 2026-09-28)

> Запрос владельца: «вместо двух скриптов должен быть один который делает три действия» — из одного скрипта и установка, и удаление, и обновление до последнего бинарника. Интерактивно: «открываешь скрипт, а он спрашивает, что делать». Также откачена вчерашняя идея логотипа в GUI (см. ниже). Разрешения владельца получены на оба изменения; **коммит делает владелец сам**.

### ЕДИНЫЙ `fedora.sh` (вместо `fedora_install.sh` + `fedora_uninstall.sh`)

- **Интерактивный режим по умолчанию** (без аргументов): меню «Что сделать?» — `1` Установить с нуля, `2` Обновить до последней версии, `3` Полностью удалить, `0` Выход. Для установки — подвопрос куда (1 = user, 2 = system); для удаления — подвопрос про `--purge`.
- **Неинтерактивные режимы** (для make/автоматизации):
  - `install [--user|--system|--udev-only] [--install-deps] [--dry-run]` — сборка + установка (user = ~/.local без sudo; system = /usr/local + udev, сам перезапускается через `sudo` при необходимости).
  - `update [--auto|--user|--system]` — **пересборка `cargo build --release` из текущих исходников + переустановка**. `--auto`/без указания определяет место по наличию бинарников (system/user; если оба — интерактивно, без TTY — system). Самая важная для владельца фича: «чтобы пользователю прилетели изменения в программах».
  - `uninstall [--purge] [--keep-rules] [--dry-run]` — сносит и системную, и пользовательскую установки (как старая `fedora_uninstall.sh`).
  - `deps` — системные зависимости через dnf (`rust cargo gcc pkgconf gtk4-devel libadwaita-devel glib2-devel gdk-pixbuf2-devel cairo-devel pango-devel hidapi-devel libudev-devel pulseaudio-libs-devel`).
- **Стиль** — как у старых скриптов: `set -euo pipefail`, `run()` с поддержкой `--dry-run`, русские комментарии, `--help` через `sed -n` по шапке. Функции: `need_root` (перезапуск `sudo $0 install --<target>`), `detect_target` (позиция для update), `build_release`, `install_files`, `install_udev_rules`, `install_deps`, `do_uninstall`, `interactive_menu`.
- **Памятка по доступу** (plugdev/udev) выводится после user-установки.
- **Пересборка кэша иконок/`.desktop`** (`refresh_icon_cache`): после `install`/`update`/`uninstall` для соответствующей позиции вызываются `gtk-update-icon-cache -f <hicolor>` + `update-desktop-database <applications>` (если утилиты есть; каталог отсутствует — пропускается; системный без root — пропуск с сообщением). Без этого на Fedora/GNOME иконка не появляется/не исчезает в меню, т.к. читается `icon-theme.cache`, а не файл `.svg` напрямую (10.107b).
- `git rm` удалены оба старых скрипта.

### ОБНОВЛЕННЫЕ ФАЙЛЫ

- **`Makefile`**: `INSTALLER`/`UNINSTALLER` → единый `FEDORA ?= ./fedora.sh`; цели — `install` → `$(FEDORA) install --system`, `install-user` → `install --user`, `install-udev` → `install --udev-only`, **`update` → `update --auto`** (новый таргет), `install-deps` → `deps`, `uninstall` → `uninstall $(UNINSTALL_FLAGS)`; `help` и `.PHONY` обновлены.
- **`.gitlab-ci.yml`**: в джобе `fedora-build` `bash -n fedora.sh` вместо `fedora_install.sh fedora_uninstall.sh`. dnf-строка остаётся одной строкой без `\` (10.58).
- **`README.md`**: раздел «Вариант 5» переписан под единый скрипт (Шаг 2 `sudo ./fedora.sh deps`, Шаг 3 `./fedora.sh` / `install --user/--system/--udev-only`, новый Шаг 4 «Обновление» = `./fedora.sh update`, Шаг 6 `uninstall`), таблица make-таргетов (+ `make update`), режимы/флаги одной строкой, дерево проекта (две строки → одна `fedora.sh`), Roadmap.
- **`docs/collect-data.md`**: команды `fedora_install.sh …` → `fedora.sh deps` / `fedora.sh install --user` / `fedora.sh install --udev-only`.
- **`AGENTS.md`**: сводка Fedora + индекс + этот блок.

### ОТКАТ ЛОГОТИПА GUI (перенесено из черновика 10.106-черновик)

- Владелец сначала попросил логотип под названием в сайдбаре GUI (`src/gui/app.rs`, `build_sidebar`): сгенерирован `data/keyboard-center.png` 48×48 из SVG (rsvg-convert из nix store, librsvg в флейке нет → для rasterizашних SVG потребовался бы внешний пакет), в `Cargo.toml` добавлена прямая зависимость `gdk-pixbuf = "0.20"` (в Cargo.lock был транзитивно), декодирование PNG через `PixbufLoader` + `gtk::gdk::Texture::for_pixbuf` + `gtk::Image::from_paintable` (`Image::from_pixbuf` deprecated с 4.12). Собран и запущен — логотип появился, но владельцу показался «капец мелким» (48×48); увеличен до 128×128.
- **На этой сессии владелец отказался от идеи полностью.** Откат: `git checkout -- src/gui/app.rs Cargo.toml Cargo.lock`, `rm data/keyboard-center.png`. Ничего из изменений логотипа в git не закоммичено (чистая история).

### ПРОВЕРКИ

- `bash -n fedora.sh` — OK.
- `./fedora.sh --help` — печатает шапку без shebang (sed `2,30p`).
- `./fedora.sh uninstall --dry-run`, `deps --dry-run`, `update --auto --dry-run`, `install --system --dry-run`, `install --udev-only --dry-run` — вывод корректен, ничего не меняет.
- `make -n install`, `make -n update`, `make -n uninstall UNINSTALL_FLAGS=--purge` — команды на новый скрипт.

### ДОРАБОТКА 10.107b — иконка в меню (2026-09-28)

- Владелец: «удалил скриптом и установил заново на Fedora, опять пропала картинка в списке программ».
- Причина: на Fedora/GNOME иконка читается из кэша `icon-theme.cache`, а скрипт после установки его не пересобирал — `.svg` ложился на место, но в меню не появлялся (и не исчезал при удалении). Это и есть причина «опять», а не регрессия фикса 10.104 (там была другая проблема — имена с хэшем в Nix-пакете).
- Фикс: в `fedora.sh` функция `refresh_icon_cache` (user|system) вызывает `gtk-update-icon-cache -f …` и `update-desktop-database …`; подключена после `install`/`update`/`uninstall`. Специальных случаев: нет утилиты → сообщение; нет каталога (`~/.local/share/icons/hicolor` до первой установки) → пропуск; системный кэш без root → пропуск с сообщением.
- Проверено `--dry-run` для всех веток, `bash -n` OK.

### ОСТАЛОСЬ

- **Коммит/пуш — делает владелец сам** (незапушены: `c424502` udev `ATTRS` 10.103, фикс иконки 10.104, удалялка в составе этой сессии, единый `fedora.sh` 10.107).
- На NixOS-хосте изменений Fedora-пути не требуется; логотип GUI откачен без следов.
- Прогонить живой `fedora.sh update` на Fedora (владелец): должен пересобрать release и переустановить по месту.
