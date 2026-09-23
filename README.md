<p align="center">
  <img src="data/keyboard-center.svg" width="120" alt="Keyboard-Center">
</p>

<h1 align="center">⌨️ Keyboard-Center ⌨️</h1>

<p align="center">
  <img src="https://img.shields.io/badge/Version-0.3.0-blue?style=flat-square" alt="Version">
  <img src="https://img.shields.io/badge/OS-Linux-blue?logo=linux&logoColor=white&style=flat-square" alt="OS">
  <img src="https://img.shields.io/badge/GUI-GTK4_%2B_Libadwaita-orange?style=flat-square" alt="GUI">
  <img src="https://img.shields.io/badge/Lang-Rust-red?logo=rust&logoColor=white&style=flat-square" alt="Rust">
  <img src="https://img.shields.io/badge/License-MIT-yellow?style=flat-square" alt="License">
  <img src="https://img.shields.io/gitlab/last-commit/dark_siders/keyboard-control?style=flat-square&color=fabd2f&logo=git&logoColor=white" alt="Last Commit">
  <a href="README_en.md"><img src="https://img.shields.io/badge/%F0%9F%87%AC%F0%9F%87%A7_English-blue?style=flat-square" alt="English"></a>
</p>

---

## Описание

**Keyboard-Center** — приложение для управления механическими клавиатурами **AULA** и **Ajazz**: RGB-подсветка, живые эффекты, сохранение настроек в память клавиатуры, батарея (2.4G) и диагностика протокола.

Состоит из **двух бинарников**:
- `keyboard-center` — GUI на GTK4 + Libadwaita (RGB-студия, скрин-менеджер для Ajazz, управление питанием);
- `kbc` — CLI для скриптов и быстрых операций.

Разрабатывалась и тестируется на **NixOS**. Для сборки используется **Nix Flake** (`crane`), но собрать и использовать можно и без флейка (`nix-build` / cargo / бинарник).

| | |
|---|---|
| **Язык** | Rust (edition 2021) |
| **GUI** | GTK4 + Libadwaita (`gtk4-rs`, `libadwaita-rs`) |
| **HID** | `hidapi` (через `libudev`) |
| **Сборка** | Nix Flake + `crane` |
| **Целевая ОС** | Linux (фокус — NixOS) |
| **Лицензия** | MIT |

> [!NOTE] Статус поддержки
> **AULA F75** — полностью рабочий сценарий на реальном железе (USB + 2.4G).  
> **AULA F99** и **Ajazz AK820 Pro** — драйверы написаны по ресёрчу (протокол общий с F75 / из wsclx-проектов), но физического железа для живой проверки пока нет (см. [статус устройств](#статус-устройств)).

---

## Возможности

### Реально работают на железе (F75, USB и 2.4G)

- **RGB-подсветка** — выбор цвета, яркости и скорости; живые host-эффекты (Static / Дыхание / Волна / Спектр / Риппл / Дождь / Реактивная) через direct-канал с keepalive.
- **Сохранение в память клавиатуры** — аппаратные эффекты из встроенной таблицы прошивки (`data[10]` / `led[10]`), включая яркость/скорость. Держатся **без запущенного приложения**, переживают перезапуск и отключение.
- **Выключить свет в память** — свет не загорится при старте (USB: обнуление яркости во всех слотах эффектов; 2.4G: совсем).
- **Батарея (2.4G-ресивер)** — процент заряда по каналу 0x13/0x05.
- **Автосохранение при закрытии GUI** — последний выбор эффекта/света записывается сам.
- **Выбор командного HID-интерфейса** — автоматически находит вендорский usages-page канал (hidraw), без ручного выбора.

### GUI (`keyboard-center`)

| Страница | Что делает |
|---|---|
| **Устройство** | Выбор/открытие подключённой клавиатуры, статус |
| **RGB & Light Studio** | Пресеты, режимы/яркость/скорость, «живой цвет», аппаратные эффекты, «Записать в память», «Выключить свет» |
| **Screen Manager** | Ajazz AK820 Pro: загрузка GIF/картинки на TFT-экран, превью |
| **Keymap & Fn Layer** | Показ раскладки Default/FN1 (F75); переназначение — через CLI `kbc remap` (GUI-клик-редактор — TODO) |
| **System & Power** | Polling rate, debounce, sleep timer, батарея, статус подключения |

### CLI (`kbc`)

```bash
kbc list                                    # список найденных устройств
kbc info 258A:010C                          # детали устройства
kbc rgb 258A:010C --mode wave --brightness 180 --speed 120   # host-эффект (USB, живёт с keepalive)
kbc rgb 3554:FA09 --mode breathing --brightness 128 --speed 30
kbc save 258A:010C --effect 0B              # аппаратный эффект в память (держится без приложения)
kbc save 3554:FA09 --effect 06              # то же для ресивера 2.4G
kbc battery 3554:FA09                       # процент заряда (2.4G; клавиатура не должна спать)
kbc time 258A:010C                          # синхронизация времени (Ajazz)
kbc set-poll 258A:010C 1000                 # polling rate
kbc set-debounce 258A:010C 4                # анти-дребезг
kbc set-sleep 258A:010C 600                 # автоотключение, c
kbc profile save ./pro.toml --device 258A:010C --name "Pro"
# --- переназначение клавиш (AULA F75, USB) ---
kbc remap 258A:010C --list                    # показать раскладку Default и FN1 (слоты + физические клавиши)
kbc remap 258A:010C --key 44 --to 2C          # слот 0x2C физически «Space» → печатает «A» (usage 0x2C)
kbc remap 258A:010C --key 68 --media E2       # физическая «[» → пауза/плэй (media usage 0xE2)
kbc remap 258A:010C --key 68 --to 2E --layer 1   # правим FN1-слой (внимание: запись применяется к ОБОИМ слоям)
kbc remap 258A:010C --key 73 --modifier 1     # ВНИМАНИЕ: тест только на неиспользованных слотах! После теста — readback (--list)
kbc monitor                                 # горячая замена (watch)
# --- пресеты и даемон (плавные host-эффекты без GUI) ---
kbc preset save wave2 --mode wave --brightness 200 --speed 60   # сохранить пресет (TOML в ~/.config/keyboard-center/presets)
kbc preset list                            # список пресетов
kbc daemon --preset wave2                  # крутит эффект до Ctrl-C (кадр 40 мс; 2.4G/Ajazz — запись в память и выход)
kbc preset apply wave2                     # то же через делегирование
kbc preset remove wave2                    # удалить пресет («default» удалять нельзя)
# --- диагностика протокола ---
kbc probe 258A:010C                         # скан интерфейсов и feature-репортов
kbc raw 258A:010C feature-get 6 520
kbc raw 258A:010C feature-send "06 84 00 00 01 00 80 00"
kbc raw 3554:FA09 output-send "13 05 00 ..."
```

#### Справочник команд

Полный перечень сабкоманд CLI (контракт, стабилен для применения; флаги отмечены `*` — experimental/для 2.4G-ресивера):

| Команда | Назначение | Ключевые флаги |
|---|---|---|
| `kbc list` | список подключённых устройств | — |
| `kbc info <VID:PID>` | детали устройства (тип, модель) | — |
| `kbc rgb <VID:PID>` | подсветка: host-эффект (USB) или запись в память ресивера (2.4G) | `--mode`, `--color RRGGBB`, `--brightness`, `--speed`, `--hold ms`, `--light-on`, `--light-off` (2.4G), `--hw-effect HEX`* |
| `kbc screen <VID:PID> --image <path>` | загрузка кадра/GIF на TFT (Ajazz AK820 Pro) | `--image` |
| `kbc time <VID:PID>` | синхронизация времени ПК с экраном (Ajazz) | — |
| `kbc battery <VID:PID>` | заряд 0..100% (только 2.4G; USB/BT — инфо-сообщение) | — |
| `kbc save <VID:PID>` | эффект/конфиг в память клавиатуры (держится без приложения) | `--effect HEX`, `--brightness`, `--speed` |
| `kbc remap <VID:PID>` | переназначение клавиш (F75, USB; верифицировано) | `--list`, `--key slot`, `--to usage`, `--media usage`, `--modifier 0..7`, `--layer 0\|1` |
| `kbc set-poll <VID:PID> <Hz>` | polling rate 125 \| 250 \| 500 \| 1000 | — |
| `kbc set-debounce <VID:PID> <ms>` | анти-дребезг | — |
| `kbc set-sleep <VID:PID> <sec>` | автоотключение (0 = выкл) | — |
| `kbc profile save/load <path> [--device]` | профиль в файл / из файла | `--device`, `--name` |
| `kbc profile eeprom-save <VID:PID> <slot>` | профиль во встроенную память (0..=7) | — |
| `kbc preset save/list/apply/remove <name>` | TOML-пресеты подсветки | `--mode`, `--color`, `--brightness`, `--speed`, `--frame-ms` |
| `kbc daemon --preset <name>` | host-движок по пресету до Ctrl-C (2.4G/Ajazz — запись в память и выход) | `--preset`, `--mode`, `--color`, `--brightness`, `--speed`, `--frame-ms` |
| `kbc monitor` | отслеживание горячей замены | — |
| `kbc probe <VID:PID>` | разведка HID: интерфейсы + скан feature-репортов | `--len` |
| `kbc raw <VID:PID> <op>` | raw-доступ (диагностика протокола) | `feature-get ID LEN`, `feature-send HEX`, `output-send HEX`, `input-read LEN TIMEOUT_MS`, `wl-read-pages CMD NEED`* |

Формат `VID:PID` — hex через двоеточие (например `258A:010C`); RGB-цвет — `RRGGBB` (hex); эффекты/слоты — hex (например `0B`, `0x14`).

#### Формат TOML-пресета

Один пресет = один файл `~/.config/keyboard-center/presets/<name>.toml`. Поле `version` задаёт схему файла (текущая 1); файлы без него читаются как v1; версия выше понятной — отклоняется с подсказкой.

```toml
version = 1          # схема формата (для миграций)
type = "wave"        # snake_case из RgbMode
color = "#FF5A00"    # базовый цвет
brightness = 200     # 0..=255
speed = 60           # 1..=255
frame_ms = 40        # период кадра host-рендеринга, мс
```

---

## Поддерживаемые устройства

### VID/PID

| Устройство | Режим | VID:PID |
|---|---|---|
| **Ajazz AK820 Pro** | USB + 2.4G (основная ревизия) | `0C45:8009` |
| | USB + 2.4G (вторая ревизия) | `0C45:800A` |
| | USB + 2.4G (сток fw v1.14) | `0C45:8099` |
| | BT | `0C45:FEFE` |
| **AULA F75** | USB | `258A:004B` |
| | 2.4G | `258A:004C` |
| | BT | `258A:004D` |
| | USB (ревизия «BY Tech») | `258A:010C` |
| **AULA F75 (2.4 GHz)** | 2.4G-ресивер Compx | `3554:FA09` |
| **AULA F99** | USB | `258A:004E` |
| | 2.4G | `258A:004F` |
| | BT | `258A:0050` |

> **F99 и F75 делят один 2.4G-ресивер** `3554:FA09`. VID/PID зависят от ревизии — реальные значения смотрите через `lsusb` / `udevadm monitor`.

### Статус устройств

| Устройство | USB | 2.4G | BT | Статус |
|---|---|---|---|---|
| AULA F75 | ✅ | ✅ | 🚫 закрыто | **есть железо, работает** |
| AULA F99 | 🟡 драйвер готов | частично (общий ресивер) | 🚫 закрыто | железа нет, протокол общий с F75 |
| Ajazz AK820 Pro | 🟡 драйвер готов | ⬜ | 🚫 закрыто | железа нет, по ресёрчу (Sonix SN32F299) |

> **BT (Bluetooth) закрыто** по решению владельца: подключение по Bluetooth не поддерживается, батарея по BT не читается. Единственный живой канал батареи — 2.4G-ресивер. VID/PID для BT в таблице выше оставлены как идентификаторы устройств.

> **Есть физически у владельца:** только **AULA F75** (USB, ревизия «BY Tech Gaming Keyboard» PID `0x010C`) + 2.4G-ресивер `3554:FA09`. Ajazz AK820 Pro и F99 физически отсутствуют — их драйверы заполняются по ресёрчу и протокольным аналогам без выдуманных команд.

---

## Установка

### Вариант 1 — NixOS (флейк в системном конфиге)

В `flake.nix` своего конфига добавьте вход:

```nix
inputs = {
  keyboard-center = {
    url = "gitlab:dark_siders/keyboard-control";
    inputs.nixpkgs.follows = "nixpkgs";   # или "flake-utils.crane.nixpkgs..."
  };
};
```

Подключите модуль и включите:

```nix
{
  imports = [
    inputs.keyboard-center.nixosModules.default
  ];

  programs.keyboard-center.enable = true;

  # Добавить пользователя в группу plugdev (доступ к hidraw без sudo)
  programs.keyboard-center.enableUserGroup = true;   # по умолчанию true
  programs.keyboard-center.enableUdevRules = true;   # по умолчанию true

  # Пользователь в группе (иначе hidraw без root не откроется):
  users.users.ваше_имя.extraGroups = [ "plugdev" ];
}
```

Собрать:

```bash
sudo nixos-rebuild switch --flake .#имя_хоста
```

Модуль ставит: бинарники `keyboard-center`/`kbc` в `environment.systemPackages`, udev-правила через `services.udev.packages`, группу `plugdev`, `.desktop`-файл и иконку.

> [!NOTE] Доступ к `/dev/hidraw`
> Если вы никогда не перелогинивались после добавления в `plugdev` — группа не применится к текущей сессии. Список активных групп: `id -nG`. Быстрый способ без перелогина: обернуть команду в `sg plugdev -c "kbc list"`.

### Вариант 2 — Home Manager (пользовательский уровень)

```nix
{ inputs, ... }:
{
  home-manager.users.ваше_имя = { ... }: {
    imports = [
      inputs.keyboard-center.homeManagerModules.default
    ];
    programs.keyboard-center.enable = true;
    # systemd user unit: держит host-эффект пресета при входе в сессию
    programs.keyboard-center.daemon.enable = true;
    programs.keyboard-center.daemon.preset = "wave2";   # имя пресета (kbc preset save …)
  };
}
```

Ставит бинарники и `.desktop`-файл на уровне пользователя. **Udev-правила при этом должен дать NixOS-модуль** (права на hidraw — системное дело).

Даемон перезапускается без ребилда: `systemctl --user restart keyboard-center-daemon`. До пересоздания пресета останавливайте юнит (`systemctl --user stop keyboard-center-daemon`), иначе `kbc preset save` перезапишет эффект на лету.

### Вариант 3 — без флейка (NixOS / чистая сборка)

```bash
# собрать из репозитория
nix-build -E 'import (builtins.getFlake (toString ./.)) { }' -A packages.x86_64-linux.default

# либо через обёртку default.nix
nix-build
```

Результат — в `result/`, при желании установить в окружение:

```bash
# udev-правила
sudo cp 99-keyboard-center.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger

# бинарники (путь по образцу)
cp result/bin/* ~/.local/bin/

# .desktop + иконка
mkdir -p ~/.local/share/applications ~/.local/share/icons/hicolor/scalable/apps
cp result/share/applications/keyboard-center.desktop ~/.local/share/applications/
cp result/share/icons/hicolor/scalable/apps/keyboard-center.svg ~/.local/share/icons/hicolor/scalable/apps/
```

> [!NOTE] Доступ к hidraw без флейка
> Группа `plugdev` должна существовать и пользователь в неё входить (`sudo groupadd plugdev`, `sudo usermod -aG plugdev $USER`, перелогин). Правила выше выставляют `GROUP="plugdev" MODE="0664"`.

### Вариант 4 — разработка (cargo / dev-shell)

```bash
nix develop . --command cargo build --release
nix develop . --command cargo check --all-targets
nix develop . --command cargo test --lib
# без nix (если есть системные gtk4/libadwaita/udev)
cargo build --release
```

Готовые бинарники: `target/release/kbc`, `target/release/keyboard-center`.

### Вариант 5 — Fedora / любой дистрибутив без nix (Makefile + fedora_install.sh)

**Полный пошаговый сценарий** (Fedora 40/41+, аналогично других дистрибутивах с `dnf`).

#### Шаг 1. Разрешения на доступ к клавиатуре (группа `plugdev`)

Доступ к `/dev/hidraw` без root даёт группа `plugdev` и udev-правила `99-keyboard-center.rules`.
Скрипт установки создаёт правило через флаг `--system`, а группу нужно сделать один раз:

```bash
sudo groupadd plugdev           # ошибка «уже существует» — не страшно
sudo usermod -aG plugdev $USER
```

> [!IMPORTANT]
> После добавления в `plugdev` нужно **перелогиниться** (группа применяется только к новой сессии).
> Проверка: `id -nG` должен содержать `plugdev`. Быстрый временный способ без перелогина —
> обернуть команду: `sg plugdev -c "kbc list"`.

#### Шаг 2. Установка зависимостей

Либо автоматически, флагом скрипта (ставит Rust, GTK4/Libadwaita dev-пакеты и hidapi):

```bash
sudo ./fedora_install.sh --install-deps
```

Либо вручную, чтобы контролировать состав:

```bash
sudo dnf install gcc gtk4-devel libadwaita-devel hidapi-devel glib2-devel \
                 cairo-devel pango-devel gdk-pixbuf2-devel libudev-devel
```

(`rust`/`cargo`, если ещё не стоят: `sudo dnf install rust cargo` — их тоже ставит `--install-deps`.)

#### Шаг 3. Сборка

```bash
make release            # = cargo build --release
```

Готовые бинарники: `target/release/kbc`, `target/release/keyboard-center`.

#### Шаг 4а. Установка для своего пользователя (без sudo)

```bash
./fedora_install.sh --user
```

Ставит `kbc`/`keyboard-center` в `~/.local/bin`, `.desktop` и иконку в `~/.local/share`.
**Udev-правило так не ставится** — без него hidraw не откроется даже с группой `plugdev`.
Решите разово (нужен sudo):

```bash
sudo ./fedora_install.sh --udev-only   # только 99-keyboard-center.rules + reload/trigger
```

#### Шаг 4б. Системная установка (все сразу, нужен sudo)

```bash
sudo ./fedora_install.sh --system
```

Ставит бинарники в `/usr/local/bin`, глобальный `.desktop`+иконку и udev-правила
(с перезагрузкой правил и триггером). Сочетается с шагом 2:

```bash
sudo ./fedora_install.sh --install-deps --system   # всё в одной команде
```

#### Шаг 5. Проверка

```bash
kbc list                  # видит поддерживаемые клавиатуры (после перелогина)
keyboard-center           # запуск GUI
```

Полезные таргеты Makefile:

| Цель | Что делает |
|---|---|
| `make build` / `make release` | debug / оптимизированная сборка |
| `make check` | `cargo check --all-targets` |
| `make test` | `cargo test --lib` |
| `make install-user` | локальная установка без sudo |
| `make install` | системная установка (`fedora_install.sh --system`) |
| `make install-udev` | только udev-правила |
| `make install-deps` | установка зависимостей через dnf (`fedora_install.sh --install-deps`) |
| `make uninstall` | удаление установленного |

Параметры `fedora_install.sh`: `--user` (по умолчанию), `--system`, `--udev-only`, `--install-deps` (поставить системные зависимости через dnf), `--uninstall`, `--dry-run` (показать действия без изменений).

---

## Работа с несколькими моделями

Сканер (`kbc list`) находит все поддерживаемые устройства сразу. Владелец держит подключёнными одновременно:

```
AULA F75 (2.4 GHz) 3554:FA09   # 2.4G-ресивер (hidraw0)
AULA F75            258A:010C   # USB, «BY Tech Gaming Keyboard»
```

> Ресивер `3554:FA09` отвечает на команды только если клавиатура **не спит на линке** — перед `kbc battery` / `kbc rgb` нажмите клавишу. Пока клавиатура спит, ресивер отвечает константным блоком на любой read.

---

## Структура проекта

```
keyboard-control/
├── AGENTS.md                 # внутренняя документация/журнал (для агента)
├── Cargo.toml                # Rust: gtk4, libadwaita, hidapi, image, clap...
├── Makefile                  # сборка/установка без nix (для Fedora и др.)
├── fedora_install.sh         # установщик Fedora: зависимости (dnf) + бинарники + udev + .desktop
├── flake.nix                 # Nix Flake: packages, nixosModules, homeManagerModules
├── default.nix               # легаси-обёртка без флейка (nix-build)
├── 99-keyboard-center.rules  # udev-правила (доступ к hidraw)
├── docs/
│   ├── collect-data.md       # инструкция владельцам F99/Ajazz (сбор данных: по умолчанию read-only)
│   └── f99-guide.md          # руководство владельцам AULA F99 (протокол/тесты/ремап)
├── scripts/
│   ├── collect-data.sh       # автособирает диагностику в tar.gz (по умолч. только чтение; для F99 --write-tests → f99-usb-rev.sh)
│   └── f99-usb-rev.sh        # реверс-диагностика USB-канала F99 (дампы + по желанию write-тесты)
├── data/
│   ├── keyboard-center.desktop
│   ├── keyboard-center.svg   # иконка приложения
│   └── firmware/
│       ├── f75_firmware.hex  # дамп прошивки F75 (61 КБ, без модификаций)
│       └── FIRMWARE.md       # паспорт дампа
├── nix/
│   ├── nixos-module.nix      # NixOS-модуль (программы/udev/группа)
│   └── home-module.nix       # Home-Manager-модуль
└── src/
    ├── cli.rs                # CLI-бинарник `kbc`
    ├── main.rs               # GUI-бинарник `keyboard-center`
    ├── types.rs              # Color, KeyId, RgbMode, Keymap...
    ├── devices/              # trait KeyboardDevice + драйверы
    │   ├── mod.rs            # реестр VID/PID, trait, build_device
    │   ├── hid.rs            # HidBridge поверх hidapi (+выбор командного канала)
    │   ├── hidshell.rs       # raw-доступ и разведка (probe/raw)
    │   ├── scanner.rs        # сканер /dev/hidraw + hot-plug
    │   ├── ajazz_ak820.rs    # Ajazz AK820 Pro
    │   ├── aula_f75.rs       # AULA F75 (USB + 2.4G-ресивер): RGB/свет/эффекты/батарея
    │   └── aula_f99.rs       # AULA F99 (по аналогу F75, 113 LED)
    ├── gui/                  # GTK4/Libadwaita
    │   ├── app.rs            # главное окно AdwApplication
    │   ├── device_panel.rs   # выбор устройства
    │   ├── keyboard_layout.rs# визуальная клавиатура
    │   ├── rgb_studio.rs     # RGB & Light Studio
    │   ├── screen_manager.rs # Screen Manager (Ajazz)
    │   ├── keymap_editor.rs  # Keymap & Fn Layer
    │   └── settings_panel.rs # System & Power (батарея, poll rate...)
    ├── screen/converter.rs   # GIF/PNG → формат экрана TFT
    └── profiles/storage.rs   # TOML-профили + EEPROM
```

---

## Документация

| Файл | Что внутри |
|---|---|
| [docs/collect-data.md](docs/collect-data.md) | Инструкция владельцам F99/Ajazz: что и как снять с реального железа (по умолчанию только чтение; для F99 есть `--write-tests`) |
| [data/firmware/FIRMWARE.md](data/firmware/FIRMWARE.md) | Паспорт дампа прошивки F75 (инструмент, MD5, команда) |
| AGENTS.md | Подробный журнал реверс-инжиниринга протокола (рус.) |

---

## Известные ограничения

- **Статические цвета в память не пишутся** — у AULA встроенная таблица прошивки содержит только радужные аппаратные эффекты; однотонная статика = host-эффект приложения и живёт, пока приложение открыто.
- **Батарея**: по USB канал заряда у AULA отсутствует (данные о заряде по проводу не передаются) — в GUI/CLI это честно объясняется. 2.4G — работает; **BT — закрыто решением владельца** (подключение по Bluetooth не поддерживается, заряд по BT не читается).
- **AULA remap** — для **F75 (USB) ремап реализован и подтверждён на железе**: протокол (512-байтная таблица, слои Default/FN1, защита Fn #53 и F1–F12, трейлер `5A A5`), карта слотов «слот ↔ физическая клавиша» (подтверждена дампом заводской таблицы), CLI `kbc remap` (клавиша→клавиша `--to`, медиа `--media`, **модификаторы `--modifier 0..7` (0xE0–0xE7)**). Нюансы: **запись ремапа применяется прошивкой сразу к обоим слоям (Default и FN1)** одним blob'ом — достаточно одного `--layer 0`; **после любого тестового ремапа (= особенно `--modifier`) обязательно проверять откат readback'ом** (`kbc remap <pid> --list | grep <слот>`), т.к. запись может «молча не примениться»; тестовые модификаторы писать только на неиспользуемые слоты (6, 23, 29, 41, 47, 65, 71, 90–126), а не на рабочие клавиши. Для **F99** канал ремапа не ревёрснут; для Ajazz — реализован в драйвере (по wsclx, на железе не проверен).
- **Ролик/энкодер не настраивается** — у F75 ролик аппаратно жёстко закреплён за громкостью (команды переназначения в протоколе нет), у **AULA F99 энкодера/ролика НЕТ** (подтверждено живой диагностикой), у Ajazz AK820 Pro он firmware-fixed (Volume/Mute). GUI больше не предлагает настройку ролика.
- **Screen Manager** — только для Ajazz AK820 Pro, железа нет — код по ресёрчу.

---

## Roadmap

- [x] AULA F75 USB: direct-RGB, host-эффекты, сохранение в память, свет выкл
- [x] AULA F75 2.4G: батарея, эффекты/яркость/скорость в память ресивера
- [x] AULA F75: ремап (протокол + карта слотов подтверждены дампом; CLI `kbc remap`)
- [x] GUI RGB-студия: пресеты, слайдеры, автосохранение при закрытии
- [ ] AULA F99 (подтверждение драйвера на железе; гайд для владельцев — `docs/f99-guide.md`)
- [ ] Ajazz AK820 Pro: RGB, Screen Manager (TFT), энкодер, статика в память
- [x] Финальная документация для сборки на Fedora (без флейка): Makefile + fedora_install.sh

---

## Поддержка / обратная связь

Проект персональный (автор — darkside). Проблемы можно репортить в GitLab-репозиторий: [gitlab.com/dark_siders/keyboard-control](https://gitlab.com/dark_siders/keyboard-control).

### У вас AULA F99 или Ajazz AK820 Pro?

У проекта **физически нет этих клавиатур** — драйверы под них написаны по ресёрчу и ещё не проверялись на реальном железе. Если у вас есть такая клавиатура, помогите её подтвердить:

```bash
git clone https://gitlab.com/dark_siders/keyboard-control && cd keyboard-control
# один раз собрать kbc (см. docs/collect-data.md, раздел 1 — NixOS или Fedora),
# затем:
./scripts/collect-data.sh
# результат: ~/kbc-diag-ГГГГММДД-ЧЧММСС.tar.gz
```

Скрипт по умолчанию собирает только **read-only** диагностику (не меняет настроек клавиатуры): `kbc list/info/probe/battery`, дампы конфиг-региона (F99), `lsusb` и uevent. Единственное исключение — флаг `--write-tests` **только для владельцев AULA F99**: включает аккуратные write-тесты T1–T3 (каждый тест восстанавливает исход; `collect-data.sh` делегирует их в `scripts/f99-usb-rev.sh`). Подробности — в **[docs/collect-data.md](docs/collect-data.md)**. Пришлите архив — это займёт 5 минут.

**Для владельцев AULA F99** есть отдельное руководство по полной поддержке клавиатуры — **[docs/f99-guide.md](docs/f99-guide.md)**: что уже работает, какие команды ещё прогнать для индикаторов и записи ремапа, и подробный разбор формата передачи подсветки (для интерпретатора GIF/анимаций).

---

Maintained by darkside 🐾