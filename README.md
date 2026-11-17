<p align="center">
  <img src="data/keyboard-center.svg" width="120" alt="Keyboard-Center">
</p>

<h1 align="center">⌨️ Keyboard-Center ⌨️</h1>

<p align="center">
  <img src="https://img.shields.io/badge/Version-0.6.3-blue?style=flat-square" alt="Version">
  <img src="https://img.shields.io/badge/OS-Linux-blue?logo=linux&logoColor=white&style=flat-square" alt="OS">
  <img src="https://img.shields.io/badge/GUI-GTK4_%2B_Libadwaita-orange?style=flat-square" alt="GUI">
  <img src="https://img.shields.io/badge/Lang-Rust-red?logo=rust&logoColor=white&style=flat-square" alt="Rust">
  <img src="https://img.shields.io/badge/License-MIT-yellow?style=flat-square" alt="License">
  <a href="https://altlinux.space/dark-siders/keyboard-control"><img src="https://img.shields.io/badge/Forgejo-altlinux.space-fabd2f?style=flat-square&logo=gitea&logoColor=white" alt="Репозиторий"></a>
</p>

---

## Что это

**Keyboard-Center** — приложение для управления механическими клавиатурами **AULA F75 / F99** и **Ajazz AK820 Pro**: RGB-подсветка, живые эффекты, сохранение настроек в память клавиатуры, батарея (2.4G), ремап клавиш и диагностика протокола.

Два бинарника:

- `keyboard-center` — GUI на GTK4 + Libadwaita (подсветка, пресеты, экран Ajazz, раскладка, трей);
- `kbc` — CLI для скриптов и быстрых операций.

Разрабатывается и тестируется на **NixOS**, но собирается и на других дистрибутивах (Fedora — через `fedora.sh`). Код на Rust, лицензия MIT.

## Поддерживаемые клавиатуры

| Клавиатура | USB | 2.4G | Комментарий |
|---|---|---|---|
| **AULA F75** | ✅ | ✅ | есть железо, полный рабочий сценарий |
| **AULA F99** | ✅ | ✅ | проверено на чужом железе; ремап/индикаторы/poll/debounce/sleep — открыто |
| **Ajazz AK820 Pro** | ✅ подсветка/время/сон; 🟡 TFT-загрузка | ✅ донгл `0C45:FDFD`: подсветка/эффекты/время/батарея | per-key и заряд ждут живой проверки; TFT портирован по hardware-verified протоколу (`0C45:800A`), на `0C45:8009` ждёт live-теста |

Частые идентификаторы: `258A:010C` (F75/F99 USB), `3554:FA09` (общий 2.4G-ресивер), `0C45:8009` / `0C45:FDFD` (Ajazz USB/донгл). VID/PID зависят от ревизии — полная таблица и статус в [docs/devices.md](docs/devices.md).

> Bluetooth **не поддерживается** по решению владельца; единственный живой канал батареи — 2.4G-ресивер.

## Что умеет

- **RGB-подсветка** — цвет, яркость, скорость; host-эффекты (Static / Дыхание / Волна / Спектр / Риппл / Дождь / Реактивная) и аудио-реактив по системному звуку (верхний ряд или столбики как в cava, только USB).
- **Сохранение в память** — аппаратные эффекты прошивки и настройки ресивера держатся без запущенного приложения; свет можно выключить «в память», чтобы не загорался при старте.
- **Однотонный цвет на 2.4G** — Solid применяется ровным цветом (с 0.6.1, проверено на железе), перед записью проверяется подпись конфига `5A A5`.
- **Батарея** — процент заряда через 2.4G-ресивер; у Ajazz — через USB/донгл (реализовано, живой проверки ждёт).
- **Ремап клавиш** — F75 по USB: Default/FN1, клик-редактор в GUI и `kbc remap` (Press/Media/Modifier).
- **Экран Ajazz** — загрузка картинки/GIF на TFT (control `0xFF13` + data `0xFF68`, ACK на чанк) и синхронизация времени; протокол подтверждён на ревизии `0C45:800A`, на `0C45:8009` — без live-проверки.
- **Пресеты и демон** — TOML-пресеты, фоновый `kbc daemon` (в т.ч. systemd user unit), автозапуск аудио-визуализации без GUI.
- **Трей (SNI)** — крестик сворачивает окно, выход — из меню трея, с автосохранением. Отключается через `KEYBOARD_CENTER_NO_TRAY=1`.

Интерфейс разделён на вкладки: **Устройства**, **Подсветка**, **Расширенные настройки пресетов**, **Клавиши**, **Экран**, **Настройки**, **Таймаут режима сна**.

## Быстрый старт

Основной путь — NixOS-модуль:

```nix
# flake.nix
inputs.keyboard-center = {
  url = "git+https://altlinux.space/dark-siders/keyboard-control";
  inputs.nixpkgs.follows = "nixpkgs";
};

# конфигурация хоста
{
  imports = [ inputs.keyboard-center.nixosModules.default ];
  programs.keyboard-center.enable = true;
  users.users.ваше_имя.extraGroups = [ "plugdev" ];  # доступ к /dev/hidraw
}
```

```bash
sudo nixos-rebuild switch --flake .#имя_хоста
kbc list            # видит клавиатуры после перелогина (группа plugdev)
keyboard-center     # GUI
```

Home Manager, сборку без флейка, dev-shell и Fedora — см. [docs/install.md](docs/install.md). Проверить доступ без перелогина: `sg plugdev -c "kbc list"`.

## Примеры команд

```bash
kbc info 258A:010C                                      # модель/ревизия (F75 и F99 делят PID)
kbc rgb 258A:010C --mode wave --brightness 180 --speed 120   # host-эффект по USB
kbc save 258A:010C --effect 0B                          # аппаратный эффект в память
kbc battery 3554:FA09                                   # заряд через 2.4G-ресивер
kbc rgb 3554:FA09 --mode static --color FF5A00          # однотонный цвет на ресивере
kbc rgb 0C45:FDFD --hw-effect 0x0B                      # эффект Ajazz через 2.4G-донгл
kbc remap 258A:010C --list                              # раскладка Default/FN1
kbc daemon --preset cava                                # аудио-столбики без GUI
```

Полный справочник CLI, флаги демона и формат TOML-пресета — в [docs/cli.md](docs/cli.md).

## Ограничения

- **USB-батареи у AULA нет** — канал заряда работает только через 2.4G; BT закрыт.
- **Статика по USB** — host-эффект (живёт, пока запущено приложение); в память пишутся аппаратные эффекты. На 2.4G Solid теперь пишется в память.
- **Ajazz** — per-key и заряд реализованы по реверсу, но живой проверки ждут; макросы записываются через `0xFF13` (`kbc macro --set`, пока не проверено на железе); keymap не переведён с SET-канала на `0xFF13`; TFT (`0xFF13` + `0xFF68`) проверен на `0C45:800A`, на ревизии `0C45:8009` живой записи ещё не было; сон по 2.4G не применяется (`Unsupported`).
- **F99** — ремап и дополнительные каналы (poll/debounce/sleep) пока не доведены.
- **Энкодер** — у F75 и Ajazz firmware-fixed, у F99 ролика нет; GUI не предлагает его настройку.

Подробности — в [docs/devices.md](docs/devices.md).

## Поддержка

Проект персональный (автор — darkside), баг-репорты — в [трекере](https://altlinux.space/dark-siders/keyboard-control). Если у вас AULA F99 или Ajazz другой ревизии — лучший вклад это read-only диагностика: соберите `kbc` (см. [docs/collect-data.md](docs/collect-data.md), раздел 1) и запустите `./scripts/collect-data.sh` (для Ajazz — `./scripts/ajazz-usb-diag.sh`). Скрипт ничего не записывает в клавиатуру и складывает архив в `~/kbc-diag-*.tar.gz`.

## Структура проекта

```
keyboard-control/
├── src/                      # драйверы устройств, GUI, CLI, аудио, пресеты
├── docs/                     # установка, CLI, устройства, гайды по железу
├── nix/                      # NixOS- и Home Manager-модули
├── flake.nix / default.nix   # сборка через Nix
├── Makefile / fedora.sh      # сборка и установка без nix
├── 99-keyboard-center.rules  # доступ к hidraw
└── data/                     # иконка, .desktop, прошивка, раскладка Ajazz
```

## Документация

| Файл | Что внутри |
|---|---|
| [docs/install.md](docs/install.md) | все варианты установки (NixOS, Home Manager, nix-build, Fedora) |
| [docs/cli.md](docs/cli.md) | справочник `kbc`, демон, TOML-пресеты |
| [docs/devices.md](docs/devices.md) | VID/PID, статус устройств, ограничения |
| [docs/f99-guide.md](docs/f99-guide.md) | протокол и тесты AULA F99 |
| [docs/ajazz-guide.md](docs/ajazz-guide.md) | протокол и диагностика Ajazz AK820 Pro |
| [docs/collect-data.md](docs/collect-data.md) | сбор диагностики с реального железа (read-only) |
| [AGENTS.md](AGENTS.md) | подробный журнал реверс-инжиниринга протоколов |

---

Maintained by darkside 🐾
