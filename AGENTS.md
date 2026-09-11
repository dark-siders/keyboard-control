# AGENTS.md — Keyboard-Center

> **Язык агента:** ТОЛЬКО русский. Никакого английского в ответах пользователю.
> **Границы:** Работа ТОЛЬКО внутри репозитория `/home/darkside/keyboard-control/`. Ни в коем случае не выходить за пределы проекта.
> **Запрет на установку:** НИКОГДА не использовать `nix profile install`, `nix profile remove`, `nix profile list`, `nix-env -i`, `nix-env -e`, `nix-env -u` или любые другие команды управления профилями/окружениями nix. Софт ставится ТОЛЬКО через конфигурацию (flake, home-manager). Исключение — только по явной команде владельца.
> **Разрешение на изменения:** Любые правки в код вносить ТОЛЬКО после явного разрешения владельца. Сначала уточнять, потом делать.
> **Stable ONLY:** Ничего не ставить и не подтягивать из unstable/nixpkgs-unstable.

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
| AULA F99 | 96% | NumPad, ролик громкости/подсветки, индикаторы |
| AULA F75 | 75% | Ролик громкости, **БЕЗ Numpad** (75%: 84 клавиши) |

Все устройства поддерживаются в 3 режимах: USB / 2.4 GHz Receiver / Bluetooth.

> **Что есть физически у владельца (2026-09-11):** только **AULA F75** (USB, ревизия «BY Tech Gaming Keyboard» PID `0x010C`, подтверждён на ПК). **Ajazz AK820 Pro и AULA F99 физически ОТСУТСТВУЮТ** — их поддержка проектируется «на будущее» (по ресёрчу/аналогам), но проверить протоколы на реальном железе пока нельзя. Вся живая разведка/дампы/протокол — только на F75.

### VID/PID (известные)

**Ajazz AK820 Pro:**
- USB: VID `0x0328` PID `0x8207`
- Dongle: VID `0x0328` PID `0x8208`
- BT: VID `0x0328` PID `0x8209`

**AULA F99:**
- USB: VID `0x258A` PID `0x004E`
- Dongle: VID `0x258A` PID `0x004F`
- BT: VID `0x258A` PID `0x0050`

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
    │   └── settings_panel.rs # System & Power
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

---

# 10. Журнал работ (не забыть!)

## 10.1 Сессия 2026-09-11 — разведка протокола AULA F75

### Что сделано
1. **`cargo check --all-targets` → 0 ошибок, 0 предупреждений**, `cargo test --lib` → 2/2 (тесты RGB565), `cargo build` → линкуется.
2. Добавлен модуль `src/devices/hidshell.rs` — raw-доступ для разведки протокола: открытие конкретного интерфейса по пути, `send_feature`/`get_feature`/`send_output`/`read_input`, скан feature-репортов, `hex_dump`.
3. Новые CLI-команды в `src/cli.rs`:
   - `kbc probe <vid:pid>` — список интерфейсов + скан feature-репортов
   - `kbc raw <vid:pid> [--path /dev/hidrawN] feature-get <id> <len>`
   - `kbc raw <vid:pid> [--path ...] feature-send <hex>`
   - `kbc raw <vid:pid> [--path ...] output-send <hex>`
   - `kbc raw <vid:pid> [--path ...] input-read <len> <timeout_ms>`
4. В реестр `DEVICES` (mod.rs), сканер (scanner.rs), HidBridge (hid.rs) и `99-keyboard-center.rules` добавлен **AULA F75 PID `0x010C`** (ревизия «BY Tech Gaming Keyboard»). `kbc list` на ПК владельца видит `258A:010C` (2 интерфейса).
5. **flake.nix (keyboard-control): `nixosModules`/`homeManagerModules` вынесены из `flake-utils.eachDefaultSystem` на верхний уровень outputs.** Раньше flake-utils раскладывал их по системам (`nixosModules.x86_64-linux.default`), из-за чего `inputs.keyboard-center.nixosModules.default` был недоступен из внешних флейков. Пакет берётся через `self.packages."x86_64-linux".keyboard-center`.

### Разведка report-descriptor AULA F75 (PID 0x010C, USB)
Устройство имеет 2 HID-интерфейса:
- `hidraw5` (input0) — классическая клавиатура 6KRO/boot (modifiers + 6 keys + LED output).
- `hidraw6` (input1) — всё остальное, report IDs:
  - `0x01` — System Control (input)
  - `0x02` — Consumer Control (input)
  - `0x03` — вендорский input (3 байта)
  - `0x04` — NKRO-матрица клавиш (120 клавиш, input)
  - `0x05` — вендорский **output** (5 байт)
  - `0x06` — вендорский **input 7 байт + output 519 байт** ← **главный канал команд**
  - `0x07` — Mouse (input)

**Ключевой вывод (ПОДТВЕРЖДЁН на железе, см. 10.3):** у AULA есть **feature report ID `0x06`** (520 байт: report id + 519 payload, тип Feature согласно дескриптору `85 06 ... 96 07 02 ... b1 02`). `probe_features` ищет обычные репорты — «классический» скан feature не покажет ничего полезного, это нормально.

**Подтверждённый протокол (по блогу Xevrion + живой проверке на клавиатуре владельца):**
- Заголовок: `06 CMD A0 A1 A2 A3 L0 L1 <data...>`; `CMD`: bit 7 = read; `0x04`/`0x84` = write/read config, `0x0a`/`0x8a` = write/read per-key color table, `0x88/0x89/0x9d/0x9e/0x9f` = read прошивки (машинный код 8051 SinoWealth).
- Адрес config region `00 00 01 00`, длина little-endian: config `0x0080`, color table `0x0200`.
- **Чтение:** сначала SET_FEATURE (read-запрос `06 84 00 00 01 00 80 00` + нулевой пад до 520 байт), затем GET_FEATURE report 6 возвращает данные (голый GET — ничего).
- Эффект-таблица — пары `(a,b)`: `a` = яркость (`0x09` полная, `0x01` тусклая), `b` = `(speed << 4) | color` (speed=старшая тетрада, color=младшая). Ripple — offset 78–79 (в data-области). Маркер конца `5A A5`.
- **Скорость света/эффектов зашита в прошивке** (чип 8051 SinoWealth): speed зажимается на 4 (0x47). Недостижимо перебором значений. Путь к «быстрой ряби» — host-rendering: статичный/DIY-режим + стриминг цветовых кадров.
- Per-key color table: 512 байт = 4 байта/клавиша (`RR GG BB 00`) × 128 клавиш. Запись кладёт цвета, но **LED не обновляются без неизвестного commit/latch-пакета** — главный нераскрытый кусок протокола.

### Статус доступа к железу (ПК владельца, host `darkside_pc`)
- NixOS, NixOS-модуль keyboard-center временно подключён в `~/nixosniri` (см. 10.2).
- udev-правила установлены: `/etc/udev/rules.d/99-keyboard-center.rules` есть, узлы перетриггерены → `/dev/hidraw5`, `/dev/hidraw6` стали `root:plugdev` mode `0664`.
- **Проблема доступа:** пользователь `darkside` **есть в группе `plugdev` по базе** (`id darkside`, `getent group plugdev` подтверждают — группа добавлена в userset от рождения), **но текущая сессия группы не видит** (`id -nG` без plugdev). `TAG+="uaccess"` ACL не добавил (getfacl не показывает uaccess). `kbc raw` даёт `Permission denied`.
  - **Рабочее решение:** обернуть команды в `sg plugdev -c "..."` — группу применить к подпроцессу без перелогина. Подтверждено на железе.
  - Вариант А (быстро, безопасно): `sudo setfacl -m u:darkside:rw /dev/hidraw5 /dev/hidraw6`; откат `sudo setfacl -x u:darkside /dev/hidraw5 /dev/hidraw6`.
  - Вариант Б (по-настоящему): временно `users.users.darkside.extraGroups = [ "plugdev" ]` в nixosniri + rebuild + **перелогин** (иначе группа не применится к текущей сессии).
  - Вариант В (через nixosniri после отката): разобраться, почему `uaccess` не сработал (возможно, нужно `ENV{ID_SEAT}="seat0"` или другое правило) и починить.

## 10.2 ВРЕМЕННЫЕ правки в `~/nixosniri` — ОБЯЗАТЕЛЬНО ОТКАТИТЬ!

Владелец разрешил временно подключить Keyboard-Center в системный флейк с последующим откатом. Для этого внесены правки (не закоммичены, рабочая копия):

1. `~/nixosniri/flake.nix` — в `inputs` добавлен блок:
   ```nix
   keyboard-center = {
     url = "path:/home/darkside/keyboard-control";
     inputs.nixpkgs.follows = "nixpkgs";
   };
   ```
   Выполнялся `nix flake update keyboard-center` (важно: без него lockpath кеширует старый narHash и модуль не находится → ошибка `attribute 'default' missing`).
2. `~/nixosniri/hosts/darkside_pc/default.nix`:
   - сигнатура изменена `{ ... }:` → `{ inputs, ... }:`
   - в `imports` добавлен `inputs.keyboard-center.nixosModules.default`
   - добавлено `programs.keyboard-center.enable = true;`

**Откат:** удалить блок из flake.nix, вернуть default.nix к исходному виду (`git checkout -- flake.nix hosts/darkside_pc/default.nix` — правки не коммитились). После отката udev-правила и бинарники останутся в системе до следующего `nixos-rebuild`.

## 10.3 Что сделано дальше (сессия 2026-09-11, продолжение)

- **Фикс CLI `kbc raw`:** аргумент `hex` заменён с `Vec<u8>` (clap падал `Mismatch ... downcast to Vec<u8>`) на `String` + ручной парсинг через `parse_hex`. hex передавать ОБЯЗАТЕЛЬНО в кавычках: `feature-send "06 84 00 00 01 00 80 00"`.
- **Живой протокол подтверждён на железе (ПК владельца):**
  1. `feature-send` read-запроса `06 84 00 00 01 00 80 00` → `feature-get 06 520` вернул **136 реальных байт** (report id 6 + header `84 00 00 01 00 80 00` + 128 байт config + `5A A5`). Без 520-байтового пада GET не отвечает (пусто/Protocol error).
  2. Снимок конфига сохранён: `/tmp/f75_config_dump.txt` (эталон для restore).
  3. Раскладка config (data-область, 128 байт): первые ~56 байт — общие настройки; с offset 58 начинается эффект-таблица пар `(a,b)`; Ripple на 78–79 (у нас `09 37` = яркость полная, speed 3, color 7); конец `5A A5`.
  4. Пассивный `input-read 7` и `feature-get` без SET — пусто (подтверждает схему SET→GET).
  5. Клавиатура ждёт именно 520-байтовые feature-пакеты (`feature-send` коротким пакетом SET проходит, но ответа нет).
- **Ресёрч open-source:** статья Xevrion https://xevrion.dev/blogs/aula-f75-linux-reverse-engineering (обновл. 2026-09-06) — тот же `258a:010c`, протокол расписан. Драйвер-референс: https://www.webkkk.net/VitalyArt/Aula-F75-Max-Driver (macOS/Linux, hidapi+GTK4).

## 10.4 Что ещё нужно будет (план следующих шагов)

1. **Доступ к hidraw6 без `sg plugdev`** — вариант Б (extraGroups+rebuild+перелогин) или разобраться с `uaccess` (вариант В).
2. **Охота за commit/latch-пакетом** (главный нераскрытый кусок): записать цветовую таблицу (`06 0a ...`, 512 байт `RR GG BB 00` ×128), затем искать короткий пакет, заставляющий LED обновиться (подбор, strace AULA-софта под Wine, dampp прошивки). До этого — всегда сохранять снимок конфига через `feature-get 06 520`.
3. **Дампить прошивку**: read-команды `0x88/0x89/0x9d/0x9e/0x9f` (машинный код 8051 SinoWealth) — бэкап и реверс (там может быть зашит и commit).
4. **Заполнить драйверы** `src/devices/aula_f75.rs` и `aula_f99.rs` реальными командами (сейчас `cmd()` методы — заглушки с выдуманными ID): RGB (mode/color/brightness/speed), polling rate, debounce, sleep timer, battery.
5. **Ключевые поля для матрицы RGB:** у владельца **F75 — 84 клавиши, БЕЗ Numpad** (см. `KEY_COUNT: u16 = 84`; 75% раскладка). F99 (96%) — с NumPad, но его физически нет — раскладку матрицы по нему проверить пока нельзя (только по официальному софту/ресёрчу). Проверяемая живая цель сейчас — именно 84 клавиши без нумпада.
6. **После завершения проверки** — откатить правки 10.2 и (по желанию владельца) решить, оставлять ли Keyboard-Center в nixosniri постоянно.
7. По желанию: `cargo build --release`, публикация в GitLab.
8. **Отображение процента заряда АКБ для всех трёх видов подключения: USB / 2.4G-ресивер / Bluetooth.** Сейчас батарея не показывается ни в CLI, ни в GUI (метод `get_battery_level` — ВЫМЫСЕЛ). Нужно: выяснить команду/канал чтения заряда на каждом из трёх транспортных режимов и вывести процент в GUI (см. 10.12, п.3).

## 10.5 Попытки дампа прошивки и чтения color table (сессия 2026-09-11)

Проверялось на живой F75 (hidraw6, через `sg plugdev`). **Итог: только чтение config работает надёжно. Формат адреса для прошивки и color table НЕ выяснен.**

- **Дамп прошивки** (CMD `0x88/0x89/0x9d/0x9e/0x9f`, идея из блога Xevrion — это read-команды машинного кода 8051 SinoWealth):
  - `0x88` и `0x9e` с адресом `00 00 00 00`, длиной `0x0080` — SET проходит, но GET → `Protocol error` (пустой ответ).
  - `0x89/0x9d/0x9f`, а также `0x88` len=`0x0100`/`0x0200` — `Protocol error` на SET.
  - **Осторожно:** ряд комбинаций вызывал **самопереподключение клавиатуры** — `/dev/hidraw6` на 1–2 сек исчезал («No such file or directory»), потом появлялся снова: `0x88` адрес `00 00 01 00`, `0x88` len `0x0200`, `0x9d`, `0x9f`. Клавиатура пережила без последствий (узлы и функция вернулись), но вслепую перебирать адреса дальше НЕ стал — риск подвешивания.
- **Чтение color table** (`0x8a`, 512 байт = `RR GG BB 00` ×128):
  - Адрес `00 00 01 00` (как у config), len `0x0200` — GET → `Protocol error`.
  - Адреса `00 00 02 00` и `00 00 00 01` — клавиатура переподключилась.
  - Вывод: формат адреса color table другой либо colour-область не читается через `0x8a`. Нужен правильный адрес/регион.
- **Что дальше по этим двум задачам:** брать адрес/формат из референс-драйверов (VitalyArt/Aula-F75-Max-Driver, OpenRGB-тикет на `258a:010c`), а не слепым перебором. Приоритет — нахождение commit/latch-команды через **straсе АУЛА-софта под Wine** (см. блог Xevrion) или реверс кода прошивки после получения правильного дампа.
- **ВАЖНО про VitalyArt/Aula-F75-Max-Driver:** это драйвер для **Epomaker x Aula F75 Max** — ДРУГОЕ железо: `0c45:800a` (wired) / `05ac:024f` (2.4G receiver), usage pages `ff13/ff68/ff59/ff60`. Это НЕ наша `258a:010c` (SinoWealth 8051). Протоколы там могут отличаться — использовать как референс архитектуры (hidapi+hidraw, GUI), но команды/адреса НЕ переносить напрямую без проверки.

## 10.6 Текущий статус (сессия 2026-09-11, итог)

- **Подтверждено на железе и работает:** чтение config region (`06 84 ...` → GET 520), структура эффект-таблицы `(a,b)`, маркер `5A A5`. Снимок config: `/tmp/f75_config_dump.txt`.
- **Не работает / не выяснено:** адрес чтения прошивки, адрес чтения color table, **commit/latch-пакет** (главный оставшийся кусок протокола).
- **Ограничение железа:** только F75 физически; Ajazz AK820 Pro и F99 — на будущее, по ресёрчу.
- **Следующие шаги (по приоритету):** исследовать код VitalyArt/Aula-F75-Max-Driver + OpenRGB-тикет → определить адреса/команды; затем strace AULA-софта под Wine для commit-пакета; после — заполнить драйверы `aula_f75.rs`/`aula_f99.rs` и откатить правки nixosniri.

## 10.7 Прорыв: direct-режим через OpenRGB (сессия 2026-09-11)

Найден референс-драйвер **OpenRGB `SinowealthKeyboard10cController`** (Rodrigo Tavares, PID `010c`) — это прямой рабочий путь для отображения цветов, **без commit/latch**:

- **Direct-режим:** feature report `06 08 00 00 01 00 7A 01` + RGB-тройки (3 байта на клавишу) начиная с offset `0x08`, буфер 520 байт, `hid_send_feature_report`. Клавиатура сама отображает цвета.
- **Keepalive:** клавиатура в direct-режиме «умирает» (~1 с) без потока пакетов — OpenRGB шлёт refresh каждые ~500 мс (thread `KeepaliveThreadFunction`, `1s` порог). Для host-рендеринга нужен **постоянный стриминг**, а не один пакет.
- **Раскладка F75** в драйвере: `aula_f75_layout` (модель `0xCD`), 75%, LED-индексы до 89 (90 LED), sparse. Также есть `aula_f99_layout` (модель `0xA4`), `aula_f87_layout` (`0x0B`), LEOBOG Hi75C Pro (`0xA3`), Redragon K686 Eisa Pro (`0x20`/`0x05`).
- **Проверено на железе (подтверждение владельца 2026-09-11):** пакет direct-режима (все LED красным) отправлен через `kbc raw feature-send` серией кадров — клавиатура перешла в отображение цветов (подсветка загорелась красным). После остановки потока подсветка погасла — это штатное поведение keepalive, не поломка.
- **Прочие находки из ресёрча:**
  - Документированный протокол F87 (тот же чип): `docs/PROTOCOL.md` у `marcoslor/Aula-F87-Controller` — 20-байтовые output-фрагменты (report ID `0x13`), READ `0x44`/WRITE `0x04`/COLOR `0x09`/PERKEY `0x02`/SAVE `0x0A`/AUDIO `0x88`, checksum = sum[0:19]&0xFF, эффект 21 = Self_define, byte 14 config должен быть `0x00` при записи (известная загвоздка). Альтернативный канал к тому же устройству.
  - `sinowealth-kb-tool` (carlossless) — ISP-бэкап/restore прошивки SinoWealth.
  - OpenRGB контроллеры: `SinowealthKeyboard10cController` (наш PID 010C), `SinowealthKeyboard16Controller` и др.
  - Xevrion commit-пакет: его не нужно, если использовать direct-режим; «commit» был нужен только для обычной (не direct) записи color table через `0x0a`.

## 10.8 ДАМП ПРОШИВКИ выполнен (сессия 2026-09-11)

- **Инструмент:** `sinowisp` (carlossless, бывш. sinowealth-kb-tool) v2.1.0, собран из исходников в `/tmp/opencode/aula/sinowealth-kb-tool` (nix develop, release). Не устанавливался — только временная сборка.
- **Профиль устройства:** `DEVICE_AULA_F75` = vendor `0x258a` product `0x010c`, платформа **SH68F90** (BYK916), isp_iface_num=`1`, isp_report_id=`5`, firmware_size 61440, bootloader 4096, reboot=true (по умолчанию). ISP-канал: report ID `0x05` — команды, `0x06` — обмен (CMD_ENABLE_FIRMWARE `0x55`, INIT_READ `0x52`, INIT_WRITE `0x57`, ERASE `0x45`, REBOOT `0x5a`, XFER_READ_PAGE `0x72`/WRITE_PAGE `0x77`).
- **Команда:** `sg plugdev -c "target/release/sinowisp read -d aula-f75 --reboot false /tmp/f75_firmware.hex"`.
- **Результат:** прошивка снята без перезагрузки устройства (`--reboot false`) — **61440 байт**, MD5 `564524ecd3aed509c3bba432f0398aca` (данные прошивки), файл `/tmp/f75_firmware.hex` (Intel HEX, 3841 строк). Вектор сброса и код на месте.
- **Клан по дампу:** файл ПОЛОЖЕН в репозиторий по решению владельца — `data/firmware/f75_firmware.hex` (директория `data/firmware/` создана под дампы, md5 файла `e2b5c9ad159962302d994a501ce4bb63`). Оригинал остаётся в `/tmp/f75_firmware.hex`.
- **Примечание по безопасности чтения:** sinowisp-read в конце выставляет LJMP-опкод на `firmware_size-5` (обычно уже есть) — рисков не заметил; запись/стирание флеша на живом устройстве НЕ выполнялись.

## 10.9 Вывод по «commit-пакету» (сессия 2026-09-11)

- **Для HOST-RENDERING commit не нужен** — прямой канал OpenRGB (`06 08 00 00 01 00 7A 01` + RGB, 520 б) уже сам показывает цвета на LED, постоянно пересылая кадры (keepalive ~0.5–1с). Это и есть «commit», если требуется живой стриминг.
- **20-байтовый протокол F87** (report ID `0x13`, READ `0x44`/WRITE `0x04`/COLOR `0x09`/PERKEY `0x02`/SAVE `0x0A`) — это ДРУГОЙ канал (output reports), у нашего F75 в in-use дескрипторе report `0x13` НЕТ. Применим для wireless-F87/F99, не для проводного F75 (0x06). Команды оттуда напрямую не переносить.
- **dаудиторский нюанс (dnviti/aula-x99-pro, wireless):** writes persist immediately, отдельного save нет; при записи RGB+LED — LED писать ПОСЛЕДНИМ (иначе RGB-запись инвалидирует LED); `write_cmd = read_cmd & 0x0F`.
- **Открыт остаётся лишь вопрос постоянного сохранения настроек в память клавиатуры** (чтобы эффект/цвета держались без ПК). Пути: DSP через `sinowisp` (рискованно), либо поиск в ходе strace OEM-софта. Для базового стриминга не требуется.

## 10.10 Реальный протокол в драйвере + GUI live-цвет (сессия 2026-09-11, вечер)

- **AulaF75 стал рабочим по 2 направлениям (подтверждено на железе):**
  - `render_frame(&[Color])` — direct-канал (главный канал 0x06, заголовок `08 00 00 01 00 7A 01`, 3 байта × 90 LED, пад до 520). `RGB_LED_COUNT = 90` (матрица по OpenRGB, модель 0xCD). Проверено: клавиатура загорелась зелёным и удержала цвет 3 с потоком кадров.
  - `read_config()` — приватное чтение конфига (SET `84 00 00 01 00 80 00` → GET 06 520), `get_rgb_preset` теперь читает эффект-таблицу (offset 58: a=яркость, b=(speed<<4)|color) вместо фейка.
  - Остальные методы (set_rgb_mode/color/brightness/speed, remap_key, polling rate, debounce, sleep, battery, EEPROM) — помечены комментарием «ВЫМЫСЕЛ (не проверено)»: cmd-id выдуманы, до реверса не трогать.
  - **НЕ работает (подтверждено на GUI):** режим подсветки, яркость, скорость — аппаратные эффекты прошивки; переключаются только через запись конфига (SET `04 …`, пока не проверена на железе). Преcеты в GUI — это пока просто статичный direct-кадр цветом, а не эффекты.
- **`HidBridge::open` больше не открывает первый интерфейс:** для AULA (F75/F99) ищет интерфейс с вендорским usage page (≥0xFF00) — это командный канал hidraw6; если не найден, fallback на первый. Без этого `send_feature` уходил на клавиатурный hidraw5 → `Broken pipe`.
- **`app.rs`: PID `0x010C` привязан к AulaF75** (было `_ => "неподдерживаемое устройство"`). GUI может открыть наше железо.
- **CLI `kbc rgb --color RRGGBB [--hold мс]`:** `--color` теперь шлёт полный direct-кадр на все 90 LED (не заглушку на пробел); `--hold` удерживает поток keepalive (400 мс) заданное время. Fallback для устройств без direct — per-key пример.
- **GUI (RGB-студия):** добавлен блок «Живой цвет (host-rendering)»: выбор цвета + «Включить цвет» (стартует keepalive-таймер 400 мс) + «Выключить свет» (чёрный кадр + стоп). Таймер через `gtk::glib::timeout_add_local`, хранится в `Rc<RefCell<Option<SourceId>>>`.
- Проверка GUI вживую — ожидается (запуск `target/debug/keyboard-center` на ПК владельца).

## 10.11 Живой визуальный тест LED-канала (сессия 2026-09-12, ночь)

- **Цель:** повторить визуальный тест «тускло→ярко» через запись эффект-таблицы конфига (SET `06 04 …`, яркости 09↔01) — владелец в прошлый раз отошёл от ПК и не видел результат.
- **Проблема с узлами:** после записи конфига клавиатура заново создаёт hidraw-узлы (дважды за сессию: были 5+6, потом 586 и т.д.) — командный канал «переезжает». Это само по себе уже говорит, что write проходит через `кик`/реинициализацию.
- **Надёжный способ найти командный узел:** не `--path` наугад, а определять интерфейс по sysfs — командный = hidraw `258a:010c` с **usage page ≥ 0xFF00** (vendor), среди узлов это тот, который НЕ keyboard-интерфейс. В конце сессии командный = `hidraw6` (иногда 7).
- **Результат подтверждён на железе владельцем:** серия из трёх **direct-кадров** (RED→GREEN→BLUE) через `kbc raw` `06 08 …` — **была видна как вспышки цветов на клавиатуре**. Канал жив, keepalive/commit-пакет для показа direct-кадра НЕ нужен (подтверждено ещё раз).
- **Запись эффект-таблицы конфига работает в плане «данные приняты»** (readback показывает `01` после записи `01`), но **визуально НЕ применяется на лету** — клавиатура не перечитывает таблицу, пока её не перезагрузишь. Значит **commit/latch-пакет после записи конфига всё ещё не найден и необходим** для live-переключения режимов/яркости/скорости через прошивку. Host-rendering (constant direct-кадр) для этого не годится — он не управляет аппаратными эффектами.
- **Итоговое состояние:** live-цвет через host-rendering — ОК; аппаратные пресеты (эффект/яркость/скорость) — запись есть, latch нет. Приоритет следующей сессии: найти latch.
- Пакеты/снимки: `/tmp/f75test/` (RED/GREEN/BLUE direct, dim/bright конфиг-записи). В репозиторий НЕ добавлялись.

## 10.12 Известные проблемы, которые надо решить (запись владельца 2026-09-12)

1. **Не сканируется клавиатура при подключении по 2.4G-ресиверу** — `kbc list`/сканер не находят устройство в режиме ресивера (обнаружение/канал в этом режиме не работает). Нужно разобраться, почему сканер не видит клаву на ресивере.
2. **Bluetooth-подключение пока НЕ проверял** — владелец не тестировал клаву по Bluetooth (ни сканер, ни протокол, ни заряд).
3. **Процент заряда АКБ не отображается ни в одном виде подключения (USB / 2.4G / BT)** — запланировано: показывать процент в GUI для всех трёх каналов (см. план 10.4, пункт 8).
