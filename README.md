<p align="center">
  <img src="data/keyboard-center.svg" width="120" alt="Keyboard-Center">
</p>

<h1 align="center">⌨️ Keyboard-Center ⌨️</h1>

<p align="center">
  <img src="https://img.shields.io/badge/Version-0.6.1-blue?style=flat-square" alt="Version">
  <img src="https://img.shields.io/badge/OS-Linux-blue?logo=linux&logoColor=white&style=flat-square" alt="OS">
  <img src="https://img.shields.io/badge/GUI-GTK4_%2B_Libadwaita-orange?style=flat-square" alt="GUI">
  <img src="https://img.shields.io/badge/Lang-Rust-red?logo=rust&logoColor=white&style=flat-square" alt="Rust">
  <img src="https://img.shields.io/badge/License-MIT-yellow?style=flat-square" alt="License">
  <a href="https://altlinux.space/dark-siders/keyboard-control"><img src="https://img.shields.io/badge/Forgejo-altlinux.space-fabd2f?style=flat-square&logo=gitea&logoColor=white" alt="Репозиторий"></a>
</p>

---

## Описание

**Keyboard-Center** — приложение для управления механическими клавиатурами **AULA** и **Ajazz**: RGB-подсветка, живые эффекты, сохранение настроек в память клавиатуры, батарея (2.4G) и диагностика протокола.

Состоит из **двух бинарников**:
- `keyboard-center` — GUI на GTK4 + Libadwaita (RGB-студия, скрин-менеджер для Ajazz, управление питанием);
- `kbc` — CLI для скриптов и быстрых операций.

Разрабатывалась и тестируется на **NixOS**. Для сборки используется **Nix Flake** (`crane`), но собрать и использовать можно и без флейка (`nix-build` / cargo / бинарник).

## Что нового в 0.6.1

- **Фикс однотонного цвета на 2.4G-ресивере AULA** — раньше «применить статику» на ресивере
  включало перелив по палитре: коды эффектов были перепутаны. Теперь `Static` → `0x01`
  (Solid — однотонный цвет из палитры), `SpectrumCycle` → `0x03` (перелив), а палитра
  заливается выбранным цветом по всем 7 слотам каждого блока. Перед любой записью конфига
  проверяется подпись `5A A5` — защита от записи усечённого/нулевого блока. **Проверено на
  живом железе (F75, 2.4G).**

### Ранее в 0.6.0

- **Ajazz AK820 Pro — 2.4G-донгл `0C45:FDFD`** (канал `0xFF60`, MI_03) через протокол ресивера
  (кадр `[order, len, chunk#, data≤28, checksum]`, эхо первых 3 байт): подсветка (order `0x05`),
  аппаратные эффекты `0x00..0x13`, включение/выключение света, синхронизация времени (`0x0C`) и
  батарея (`0x20`). **Проверено на живом железе.**
- **Редизайн интерфейса** — понятные русские вкладки, автопоиск устройств, вкладка «Подсветка» с
  выпадающим списком 56 цветов (круглый образец + цветные кружки), автоприменение без кнопок и
  автосохранение в память клавиатуры, отдельная вкладка пресетов и отдельная вкладка «Таймаут
  режима сна».
- **Защита 2.4G-ресивера AULA** от потока записей: для 2.4G задержка автоприменения поднята до
  1.5 с, плюс не чаще одной записи в 2 с (лишние — отложенный повтор) — запись на ресивер
  двухфазная («flash and revert»), а частые записи дёргали подсветку.
- **Честные ограничения на 2.4G**: батарея на текущей ревизии донгла отдаёт 0 (прошивка не
  заполняет); **сон по 2.4G не применяется** (order `0x07` — эхо есть, эффекта нет; сон работает
  по USB); per-key / keymap / макросы / TFT по донглу — `Unsupported` (донгл не релеит данные
  ответов, а запись без чтения небезопасна).

### Ранее в 0.5.0

- **Ajazz AK820 Pro** — живая поддержка (ревизия `0C45:8009`, USB): подсветка (20 режимов),
  синхронизация времени и сон работают через control-канал `0xFF13`; per-key подсветка и заряд
  АКБ реализованы по реверсу официального ПО (живой проверки ещё ждут); заводская base-раскладка
  захардкожена из официального установщика (ассет `data/ajazz/rgb-keyboard.xml`).
- **Системный трей** — крестик сворачивает окно в трей (SNI); отключается `KEYBOARD_CENTER_NO_TRAY`.
- **Исправления**: udev-правила теперь используют `ATTRS{}` (без этого доступ к hidraw не выдавался);
  иконка приложения корректно ставится в меню.
- **Fedora**: единый скрипт `fedora.sh` — установка / обновление / удаление.

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
> **AULA F99** — USB и 2.4G проверены на отдельном реальном устройстве (orflem); драйвер написан по ресёрчу и подтверждён живой проверкой USB+2.4G.  
> **Ajazz AK820 Pro** — проверен на живом железе: по USB (ревизия `0C45:8009`) подсветка (20 режимов), синк времени и сон работают через control-канал `0xFF13`; **2.4G-донгл `0C45:FDFD` работает** (протокол ресивера `0xFF60`: подсветка, эффекты, свет, время, батарея; сон по 2.4G не применяется) — проверено живьём; per-key подсветка (`0x23`/`0x20` USB) и заряд АКБ реализованы по реверсу официального ПО, живой проверки ещё ждут; **заводская base-раскладка захардкожена** из официального установщика (ассет `data/ajazz/rgb-keyboard.xml`, 81 клавиша); **keymap и макросы, по реверсу официального ПО, тоже живут на `0xFF13`** (`0x11`/`0x27` и `0x19`/`0x15`) — команды известны, код ещё не переведён туда; TFT-интерфейс `0xFF67` на этой ревизии отсутствует (см. [статус устройств](#статус-устройств)).

---

## Возможности

### Реально работают на железе (F75, USB и 2.4G)

- **RGB-подсветка** — выбор цвета, яркости и скорости; живые host-эффекты (Static / Дыхание / Волна / Спектр / Риппл / Дождь / Реактивная) через direct-канал с keepalive.
- **Сохранение в память клавиатуры** — аппаратные эффекты из встроенной таблицы прошивки (`data[10]` / `led[10]`), включая яркость/скорость. Держатся **без запущенного приложения**, переживают перезапуск и отключение.
- **Выключить свет в память** — свет не загорится при старте (USB: обнуление яркости во всех слотах эффектов; 2.4G: совсем).
- **Однотонный цвет на 2.4G** — выбранный цвет через ресивер применяется ровным однотонным (Solid), а не радугой; перед записью проверяется подпись `5A A5`.
- **Батарея (2.4G-ресивер)** — процент заряда по каналу 0x13/0x05.
- **Автосохранение при закрытии GUI** — последний выбор эффекта/света записывается сам.
- **Выбор командного HID-интерфейса** — автоматически находит вендорский usages-page канал (hidraw), без ручного выбора.
- **Системный трей (SNI)** — окно можно свернуть в трей: крестик прячет окно (host-эффекты продолжают идти), реальный выход — из меню трея или меню «Выход» (с автосейвом). Отключить трей: `KEYBOARD_CENTER_NO_TRAY=1 keyboard-center`. Без SNI-хоста (панели) поведение прежнее.

### GUI (`keyboard-center`)

| Страница | Что делает |
|---|---|
| **Устройство** | Выбор/открытие подключённой клавиатуры, статус |
| **RGB & Light Studio** | Пресеты, режимы/яркость/скорость, «живой цвет», аудио-реактив (VU-спектр системного звука: верхний ряд или столбики как в cava, зелёный→синий, USB), аппаратные эффекты, «Записать в память», «Выключить свет» |
| **Screen Manager** | Ajazz AK820 Pro: загрузка GIF/картинки на TFT-экран, превью |
| **Keymap & Fn Layer** | Показ раскладки Default/FN1 (F75) и клик-редактор переназначения (Press/Media/Modifier, сброс); то же из CLI — `kbc remap` |
| **System & Power** | Polling rate, debounce, батарея, статус подключения |
| **Таймаут режима сна** | Ступени сна прошивки (Не засыпать / 1 / 5 / 30 мин) + «Применить» (по USB; по 2.4G — `Unsupported`) |

### CLI (`kbc`)

```bash
kbc list                                    # список найденных устройств
kbc info 258A:010C                          # детали устройства
kbc rgb 258A:010C --mode wave --brightness 180 --speed 120   # host-эффект (USB, живёт с keepalive)
kbc rgb 3554:FA09 --mode breathing --brightness 128 --speed 30
kbc save 258A:010C --effect 0B              # аппаратный эффект в память (держится без приложения)
kbc save 3554:FA09 --effect 06              # то же для ресивера 2.4G
kbc battery 3554:FA09                       # процент заряда (2.4G; клавиатура не должна спать)
kbc rgb 0C45:8009 --mode static --color 00FF00   # Ajazz: аппаратно через 0xFF13 (держится без приложения)
kbc battery 0C45:8009                       # заряд АКБ Ajazz (control-канал 0xFF13)
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
kbc reset 258A:010C                           # сбросить раскладку к заводской (Default + FN1) — как «Сбросить всё» в GUI
# --- экран Ajazz AK820 Pro (TFT) и макросы ---
# На ревизии 0C45:8009 интерфейса TFT (0xFF67) нет — screen/macro вернут Unsupported.
kbc screen 0C45:8009 --image anim.gif         # картинка или GIF (анимация уходит целиком, все кадры)
kbc screen 0C45:8009 --brightness 200         # яркость TFT-экрана (можно вместе с --image)
kbc macro 0C45:8009 --record 1                # начать запись макроса №1 (набираем клавиши на клавиатуре)
kbc macro 0C45:8009 --stop                    # остановить запись
kbc monitor                                 # горячая замена (watch)
# --- пресеты и даемон (плавные host-эффекты без GUI) ---
kbc preset save wave2 --mode wave --brightness 200 --speed 60   # сохранить пресет (TOML в ~/.config/keyboard-center/presets)
kbc preset list                            # список пресетов (в т.ч. встроенные audio-viz и cava)
kbc daemon --preset wave2                  # крутит эффект до Ctrl-C (кадр 40 мс; 2.4G/Ajazz — запись в память и выход)
kbc preset apply wave2                     # то же через делегирование
kbc preset remove wave2                    # удалить пресет («default» и встроенные удалять нельзя)
# --- аудио-реактив (USB; спектр системного звука @DEFAULT_MONITOR@) ---
# Режим row: реагирует верхний ряд (Esc + F1..F12), остальные клавиши — базовый цвет.
# Режим bars: столбики снизу вверх по всем известным позициям LED-матрицы (F75 — 80, F99 — 99, включая NumPad у F99), как в cava.
# LED без известной позиции остаются базовым цветом.
# Цвет — градиент зелёный (низы) → синий (верхи), яркость — от уровня (AGC).
# Пока музыка не играет — весь кадр статичен. Работает только по USB.
kbc rgb 258A:010C --mode audio --style row --brightness 200
kbc rgb 258A:010C --mode audio --style bars --brightness 200      # столбики как в cava
kbc daemon --preset audio-viz              # встроенный row-пресет, без GUI
kbc daemon --preset cava                  # встроенный bars-пресет, без GUI
# --- диагностика протокола ---
kbc probe 258A:010C                         # скан интерфейсов и feature-репортов
kbc ajazz-diag 0C45:8009                    # read-only дамп Ajazz AK820 Pro (все GET-команды)
kbc ajazz-diag 0C45:FDFD                    # 2.4G-донгл: эхо-пинги всех GET-команд (read-only)
kbc rgb 0C45:FDFD --mode static --color FF0000   # подсветка Ajazz через 2.4G-донгл (SET_LED_EFFECT)
kbc rgb 0C45:FDFD --hw-effect 0x0B          # аппаратный код эффекта 0x00..0x13 через донгл
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
| `kbc rgb <VID:PID>` | подсветка: host-эффект (USB) или запись в память ресивера (2.4G) | `--mode` (вкл. `audio` — спектр системного звука, USB-only), `--style row\|bars` (аудио), `--color RRGGBB`, `--brightness`, `--speed`, `--hold ms`, `--light-on`, `--light-off` (2.4G), `--hw-effect HEX`*, `--color-low RRGGBB`, `--color-high RRGGBB`, `--sensitivity 0.1..2.0` (аудио)* |
| `kbc screen <VID:PID>` | TFT-экран Ajazz: кадр/GIF-анимация и/или яркость | `--image <path>` (GIF → анимация целиком), `--brightness 0..255` |
| `kbc time <VID:PID>` | синхронизация времени ПК с экраном (Ajazz) | — |
| `kbc battery <VID:PID>` | заряд 0..100% (только 2.4G; USB/BT — инфо-сообщение) | — |
| `kbc save <VID:PID>` | эффект/конфиг в память клавиатуры (держится без приложения) | `--effect HEX`, `--brightness`, `--speed` |
| `kbc remap <VID:PID>` | переназначение клавиш (F75, USB; верифицировано) | `--list`, `--key slot`, `--to usage`, `--media usage`, `--modifier 0..7`, `--layer 0\|1` |
| `kbc reset <VID:PID>` | сброс раскладки к заводской (Default + FN1) — как «Сбросить всё» в GUI | — |
| `kbc macro <VID:PID>` | запись макроса (Ajazz AK820 Pro, device-side) | `--record N` (1..12), `--stop` |
| `kbc set-poll <VID:PID> <Hz>` | polling rate 125 \| 250 \| 500 \| 1000 | — |
| `kbc set-debounce <VID:PID> <ms>` | анти-дребезг | — |
| `kbc set-sleep <VID:PID> <sec>` | автоотключение (0 = выкл) | — |
| `kbc profile save/load <path> [--device]` | профиль в файл / из файла | `--device`, `--name` |
| `kbc profile eeprom-save <VID:PID> <slot>` | профиль во встроенную память (0..=7) | — |
| `kbc preset save/list/apply/remove <name>` | TOML-пресеты подсветки | `--mode`, `--color`, `--brightness`, `--speed`, `--frame-ms`, `--color-low`, `--color-high`, `--sensitivity`, `--style row\|bars` (аудио) |
| `kbc daemon --preset <name>` | host-движок по пресету до Ctrl-C (2.4G/Ajazz — запись в память и выход) | `--preset`, `--mode`, `--color`, `--brightness`, `--speed`, `--frame-ms`, `--color-low`, `--color-high`, `--sensitivity`, `--style row\|bars` (аудио) |
| `kbc monitor` | отслеживание горячей замены | — |
| `kbc probe <VID:PID>` | разведка HID: интерфейсы + скан feature-репортов | `--len` |
| `kbc ajazz-diag <VID:PID>` | read-only дамп Ajazz AK820 Pro: device-info/LED/game-mode/custom-LED/keymap/макросы (только GET) | — |
| `kbc raw <VID:PID> <op>` | raw-доступ (диагностика протокола) | `feature-get ID LEN`, `feature-send HEX`, `output-send HEX`, `input-read LEN TIMEOUT_MS`, `wl-read-pages CMD NEED`* |

Формат `VID:PID` — hex через двоеточие (например `258A:010C`); RGB-цвет — `RRGGBB` (hex); эффекты/слоты — hex (например `0B`, `0x14`).

---

## Режим демона (`kbc daemon`)

`kbc daemon` — это фоновый движок host-рендеринга, который крутит эффект по пресету до `Ctrl-C`. Он предназначен для работы **без GUI**, в том числе как systemd user unit.

### Основные сценарии

| Задача | Команда |
|---|---|
| Запуск пресета в фоне (до Ctrl-C) | `kbc daemon --preset wave2` |
| Встроенный аудио-пресет `row` (верхний ряд) | `kbc daemon --preset audio-viz` |
| Встроенный аудио-пресет `bars` (cava-столбики) | `kbc daemon --preset cava` |
| Свой аудио-пресет | `kbc daemon --preset my-audio` |
| Переопределение параметров без сохранения | `kbc daemon --preset wave2 --brightness 180 --speed 80` |
| Явное указание устройства | `kbc daemon --preset wave2 --device 258A:010C` |

### Все флаги `kbc daemon`

| Флаг | Описание |
|---|---|
| `--preset <name>` | Имя пресета (дефолт: `default`, создаётся автоматически) |
| `--device <VID:PID>` | Явный выбор устройства (иначе автовыбор: USB > 2.4G > BT) |
| `--mode <mode>` | Переопределить режим движка (static/breathing/wave/spectrum/ripple/rain/reactive/custom/audio) |
| `--color <RRGGBB>` | Переопределить базовый цвет |
| `--brightness <0-255>` | Переопределить яркость |
| `--speed <1-255>` | Переопределить скорость |
| `--frame-ms <1-1000>` | Переопределить период кадра (мс) |
| `--color-low <RRGGBB>` | Аудио: цвет низких частот |
| `--color-high <RRGGBB>` | Аудио: цвет высоких частот |
| `--sensitivity <0.1-2.0>` | Аудио: чувствительность |
| `--style row\|bars` | Аудио: стиль визуализации (только для `mode=audio`) |

### Особенности работы

- **Автовыбор устройства**: USB > 2.4G > Bluetooth (берётся первое найденное).
- **Аппаратная запись**: для 2.4G-ресивера и Ajazz эффект пишется во флеш **сразу** — демон завершается, приложение больше не нужно. Для USB (F75/F99) — host-рендеринг с keepalive до Ctrl-C.
- **Аудио-реактив**: работает **только по USB** (нужен доступ к `@DEFAULT_MONITOR@`). На 2.4G/BT демон завершится с ошибкой. Ленное открытие аудио-потока + авто-переоткрытие при перезапуске PipeWire/PulseAudio.
- **Пауза на старте**: 2 сек перед первым кадром (фикс «кривая вёрстка» на старте ПК).
- **Автосохранение при закрытии GUI**: последнее состояние (выбранный эффект/свет) пишется в память клавиатуры.

### Systemd user unit (автозапуск)

Для автозапуска при входе в сессию (NixOS + Home Manager):

```nix
programs.keyboard-center.daemon.enable = true;
programs.keyboard-center.daemon.preset = "audio-viz";  # или "cava", "wave2" и т.д.
```

- Юнит: `keyboard-center-daemon`
- Перезапуск без ребилда: `systemctl --user restart keyboard-center-daemon`
- **Важно**: перед изменением пресета (`kbc preset save`) останавливайте юнит:
  `systemctl --user stop keyboard-center-daemon` — иначе демон перезапишет эффект на лету.
- Аудио-пресеты `audio-viz` (row) и `cava` (bars) встроены в бинарник — не требуют пользовательских TOML-файлов.

### Примеры для аудио-реактива

```bash
# Встроенный row-пресет (верхний ряд Esc+F1..F12)
kbc daemon --preset audio-viz

# Встроенный bars-пресет (столбики по колонкам, включая NumPad у F99)
kbc daemon --preset cava

# Свой аудио-пресет
kbc preset save my-viz --mode audio --style bars --brightness 200 --color-low "#00FF00" --color-high "#0000FF" --sensitivity 1.0
kbc daemon --preset my-viz

# Systemd автозапуск (NixOS Home Manager)
# programs.keyboard-center.daemon.preset = "cava";
```

---

### Формат TOML-пресета

Один пресет = один файл `~/.config/keyboard-center/presets/<name>.toml`. Поле `version` задаёт схему файла (текущая 1); файлы без него читаются как v1; версия выше понятной — отклоняется с подсказкой.

Помимо файлов есть **два встроенных аудио-пресета**: `audio-viz` (`style = "row"`) и `cava` (`style = "bars"`). Они зашиты в бинарник, видны в `kbc preset list` и в GUI у любого пользователя без настройки, удалить их нельзя. Свой файл с таким же именем перекрывает встроенный.

```toml
version = 1          # схема формата (для миграций)
type = "wave"        # snake_case из RgbMode: static/breathing/wave/spectrum/ripple/rain/reactive/custom/audio
color = "#FF5A00"    # базовый цвет
brightness = 200     # 0..=255
speed = 60           # 1..=255
frame_ms = 40        # период кадра host-рендеринга, мс

# Секция [audio] — только для type = "audio" (у других режимов секции нет):
# [audio]
# sensitivity = 0.75   # 0.1..2.0, множитель громкости после AGC (дефолт 0.75)
# color_low = "#33FF33"  # цвет низких частот (hex), дефолт зелёный
# color_high = "#3333FF" # цвет высоких частот (hex), дефолт синий
# style = "row"        # "row" (верхний ряд) | "bars" (столбики как в cava)
```

---

## Поддерживаемые устройства

### VID/PID

| Устройство | Режим | VID:PID |
|---|---|---|
| **Ajazz AK820 Pro** | USB + 2.4G (основная ревизия) | `0C45:8009` |
| | USB + 2.4G (вторая ревизия) | `0C45:800A` |
| | USB + 2.4G (сток fw v1.14) | `0C45:8099` |
| | **2.4G-донгл** | `0C45:FDFD` |
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
| AULA F75 | ✅ Полная | ✅ Полная | 🚫 закрыто | **есть железо, работает** |
| AULA F99 | ✅ Работает | ✅ Работает | 🚫 закрыто | **драйвер готов, работает на USB+2.4G (проверено на чужом железе; keymap write / RGB flash / poll/debounce/sleep — открыто)** |
| Ajazz AK820 Pro | 🟢 подсветка/время/сон · 🟡 per-key и заряд | ✅ донгл `0C45:FDFD`: подсветка/эффекты/время/батарея через протокол ресивера `0xFF60` (сон по 2.4G — `Unsupported`) | 🚫 закрыто | **проверено на живом железе 2026-11-16: подсветка (20 режимов), время и сон через `0xFF13` (USB); через донгл — подсветка/эффекты/время (order 0x05/0x0C), сон (0x07) на ревизии донгла не применяется; per-key (`0x23`/`0x20` USB) и заряд (USB `0x20 0x01`, донгл `0x20`) реализованы по реверсу; заводская base-раскладка захардкожена из установщика (`data/ajazz/rgb-keyboard.xml`); keymap (`0x11`/`0x27`) и макросы (`0x19`/`0x15`) на `0xFF13` — команды известны, код не переведён; per-key по донглу (`0x14`) — задел; TFT `0xFF67` на ревизии 8009 отсутствует** |

> **BT (Bluetooth) закрыто** по решению владельца: подключение по Bluetooth не поддерживается, батарея по BT не читается. Единственный живой канал батареи — 2.4G-ресивер. VID/PID для BT в таблице выше оставлены как идентификаторы устройств.

> **Есть физически у владельца:** **AULA F75** (USB, ревизия «BY Tech Gaming Keyboard» PID `0x010C`) + 2.4G-ресивер `3554:FA09`; **Ajazz AK820 Pro** (`0C45:8009`, USB, на Fedora-машине) — проверен живьём по control-каналу `0xFF13`. **AULA F99** проверена на чужом железе — USB + 2.4G работают; keymap write / RGB flash / poll/debounce/sleep ждут live write-тестов. Read-only диагностика `kbc ajazz-diag` готова, но ходит в wsclx `0xFF68` — на ревизии 8009 он не отвечает, поэтому дамп вернёт `*_error=Protocol` (это ожидаемо; см. [docs/ajazz-guide.md](docs/ajazz-guide.md), раздел A).

---

## Установка

### Вариант 1 — NixOS (флейк в системном конфиге)

В `flake.nix` своего конфига добавьте вход:

```nix
inputs = {
  keyboard-center = {
    url = "git+https://altlinux.space/dark-siders/keyboard-control";
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

**Аудио-реактив в фоне, без GUI:** поставьте `programs.keyboard-center.daemon.preset = "audio-viz"` (встроенный режим верхнего ряда) или `"cava"` (встроенный режим столбиков по известной LED-матрице, включая NumPad у F99). Оба пресета зашиты в бинарник и не требуют пользовательского TOML-файла. Юнит стартует с графической сессией, сам открывает монитор `@DEFAULT_MONITOR@` и **переоткрывает поток** при перезапуске PipeWire/PulseAudio; если клавиатура или звук появятся позже старта — демон дождётся (ленивое открытие + ретраи). Нужен USB: на 2.4G/BT демон честно завершится с ошибкой «только USB».

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

### Вариант 5 — Fedora / любой дистрибутив без nix (Makefile + fedora.sh)

Один интерактивный скрипт `fedora.sh` вместо установки и удаления по отдельности.
Запустите без аргументов — скрипт спросит, что делать: **установить с нуля**,
**обновить до последней версии** (пересобрать и переустановить) или **полностью удалить**.
При установке/обновлении дополнительно выбирается место (для пользователя или системно).

**Полный пошаговый сценарий** (Fedora 40/41+, аналогично других дистрибутивах с `dnf`).

#### Шаг 1. Разрешения на доступ к клавиатуре (группа `plugdev`)

Доступ к `/dev/hidraw` без root даёт группа `plugdev` и udev-правила `99-keyboard-center.rules`.
Скрипт установки создаёт правило при системной установке, а группу нужно сделать один раз:

```bash
sudo groupadd plugdev           # ошибка «уже существует» — не страшно
sudo usermod -aG plugdev $USER
```

> [!IMPORTANT]
> После добавления в `plugdev` нужно **перелогиниться** (группа применяется только к новой сессии).
> Проверка: `id -nG` должен содержать `plugdev`. Быстрый временный способ без перелогина —
> обернуть команду: `sg plugdev -c "kbc list"`.

#### Шаг 2. Установка зависимостей и сборка

```bash
sudo ./fedora.sh deps           # системные зависимости через dnf (Rust, GTK4/hidapi dev-пакеты)
make release                    # cargo build --release
```

Или всё одной командой — скрипт поставит зависимости и сам соберёт:
`sudo ./fedora.sh install --install-deps --system`.

#### Шаг 3. Установка

Проще всего запустить интерактивно и выбрать пункт «Установить с нуля»:

```bash
./fedora.sh
```

Либо сразу указать режим:

```bash
./fedora.sh install --user                 # для пользователя: ~/.local/bin, без sudo
sudo ./fedora.sh install --system          # системно: /usr/local/bin + udev + .desktop
sudo ./fedora.sh install --udev-only       # только 99-keyboard-center.rules
```

> [!NOTE]
> При **пользовательской** установке (`--user`) udev-правило не ставится — без него hidraw
> не откроется даже с группой `plugdev`. Решите разово (нужен sudo):
> `sudo ./fedora.sh install --udev-only`.

#### Шаг 4. Обновление до последней версии

Скрипт пересобирает release-бинарники из текущих исходников и переустанавливает их туда,
где программа уже стоит (system или user — определяется автоматически):

```bash
./fedora.sh update
```

Или через Makefile: `make update`. Так «прилетают» любые изменения из последнего кода.

#### Шаг 5. Проверка

```bash
kbc list                  # видит поддерживаемые клавиатуры (после перелогина)
keyboard-center           # запуск GUI
```

#### Шаг 6. Удаление

Запустите интерактивно и выберите «Полностью удалить», либо напрямую:

```bash
./fedora.sh uninstall --dry-run     # сначала посмотреть, что будет удалено
sudo ./fedora.sh uninstall          # бинарники, .desktop, иконка, udev-правила
sudo ./fedora.sh uninstall --purge  # то же + пресеты (~/.config/keyboard-center) и кэш иконок
```

Или через Makefile: `make uninstall`, `make uninstall UNINSTALL_FLAGS=--purge`.
Скрипт требует `sudo`, если найдены системные файлы; пользовательские данные по умолчанию не трогаются.

Полезные таргеты Makefile:

| Цель | Что делает |
|---|---|
| `make build` / `make release` | debug / оптимизированная сборка |
| `make check` | `cargo check --all-targets` |
| `make test` | `cargo test --lib` |
| `make install-user` | локальная установка без sudo (`fedora.sh install --user`) |
| `make install` | системная установка (`fedora.sh install --system`) |
| `make install-udev` | только udev-правила |
| `make update` | пересборка + переустановка туда, где стоит (`fedora.sh update --auto`) |
| `make install-deps` | установка зависимостей через dnf (`fedora.sh deps`) |
| `make uninstall` | полное удаление (`fedora.sh uninstall`) |
| `make uninstall UNINSTALL_FLAGS=--purge` | удаление вместе с пресетами и кэшем иконок |

Режимы `fedora.sh`: `install` (по умолчанию `--user`, также `--system`, `--udev-only`), `update` (`--auto`/`--user`/`--system`), `uninstall`, `deps`. Флаги: `--install-deps` (поставить системные зависимости через dnf), `--purge` (пресеты `~/.config/keyboard-center` и кэш иконок), `--keep-rules` (оставить `/etc/udev/rules.d/99-keyboard-center.rules`), `--dry-run`. Без аргументов — интерактивное меню.

После установки/обновления/удаления скрипт пересобирает кэши иконок и `.desktop` (`gtk-update-icon-cache` + `update-desktop-database`) — без этого иконка в меню приложений не появится/не исчезнет на рабочих столах на базе GTK.

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
├── Cargo.toml                # Rust: gtk4, libadwaita, hidapi, image, clap, ksni...'
├── Makefile                  # сборка/установка без nix (для Fedora и др.)
├── fedora.sh                 # Fedora/без nix: установка, обновление, удаление (интерактивное меню)
├── flake.nix                 # Nix Flake: packages, nixosModules, homeManagerModules
├── default.nix               # легаси-обёртка без флейка (nix-build)
├── 99-keyboard-center.rules  # udev-правила (доступ к hidraw)
├── docs/
│   ├── collect-data.md       # инструкция владельцам F99/Ajazz (сбор данных: по умолчанию read-only)
│   ├── f99-guide.md          # руководство владельцам AULA F99 (протокол/тесты/ремап)
│   └── ajazz-guide.md        # руководство владельцам Ajazz AK820 Pro (протокол + read-only диагностика)
├── scripts/
│   ├── collect-data.sh       # автособирает диагностику в tar.gz (по умолч. только чтение; для F99 --write-tests → f99-usb-rev.sh)
│   ├── f99-usb-rev.sh        # реверс-диагностика USB-канала F99 (дампы + по желанию write-тесты)
│   └── ajazz-usb-diag.sh     # read-only диагностика Ajazz AK820 Pro (kbc ajazz-diag) → tar.gz
├── data/
│   ├── keyboard-center.desktop
│   ├── keyboard-center.svg   # иконка приложения
│   ├── firmware/
│   │   ├── f75_firmware.hex  # дамп прошивки F75 (61 КБ, без модификаций)
│   │   └── FIRMWARE.md       # паспорт дампа
│   └── ajazz/
│       ├── rgb-keyboard.xml  # заводская раскладка AK820 Pro из установщика (81 клавиша)
│       └── README.md         # паспорт ассета (провенанс, MD5)
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
    │   ├── settings_panel.rs # System & Power (батарея, poll rate...)
    │   └── tray.rs           # системный трей (SNI, ksni)
    ├── screen/converter.rs   # GIF/PNG → формат экрана TFT
    └── profiles/storage.rs   # TOML-профили + EEPROM
```

---

## Документация

| Файл | Что внутри |
|---|---|
| [docs/collect-data.md](docs/collect-data.md) | Инструкция владельцам F99/Ajazz: что и как снять с реального железа (по умолчанию только чтение; для F99 есть `--write-tests`) |
| [docs/f99-guide.md](docs/f99-guide.md) | Руководство владельцам AULA F99: протокол, тесты, ремап |
| [docs/ajazz-guide.md](docs/ajazz-guide.md) | Руководство владельцам Ajazz AK820 Pro: протокол и read-only диагностика (`kbc ajazz-diag`) |
| [data/firmware/FIRMWARE.md](data/firmware/FIRMWARE.md) | Паспорт дампа прошивки F75 (инструмент, MD5, команда) |
| [data/ajazz/README.md](data/ajazz/README.md) | Паспорт заводской раскладки Ajazz AK820 Pro (провенанс, MD5, состав XML) |
| AGENTS.md | Подробный журнал реверс-инжиниринга протокола (рус.) |

---

## Известные ограничения

- **Статические цвета в память не пишутся** — у AULA встроенная таблица прошивки содержит только радужные аппаратные эффекты; однотонная статика = host-эффект приложения и живёт, пока приложение открыто.
- **Батарея**: по USB канал заряда у AULA отсутствует (данные о заряде по проводу не передаются) — в GUI/CLI это честно объясняется. 2.4G — работает; **BT — закрыто решением владельца** (подключение по Bluetooth не поддерживается, заряд по BT не читается).
- **AULA remap** — для **F75 (USB) ремап реализован и подтверждён на железе**: протокол (512-байтная таблица, слои Default/FN1, защита Fn #53 и F1–F12, трейлер `5A A5`), карта слотов «слот ↔ физическая клавиша» (подтверждена дампом заводской таблицы), CLI `kbc remap` (клавиша→клавиша `--to`, медиа `--media`, **модификаторы `--modifier 0..7` (0xE0–0xE7)**). Нюансы: **запись ремапа применяется прошивкой сразу к обоим слоям (Default и FN1)** одним blob'ом — достаточно одного `--layer 0`; **после любого тестового ремапа (= особенно `--modifier`) обязательно проверять откат readback'ом** (`kbc remap <pid> --list | grep <слот>`), т.к. запись может «молча не примениться»; тестовые модификаторы писать только на неиспользуемые слоты (6, 23, 29, 41, 47, 65, 71, 90–126), а не на рабочие клавиши. Для **F99** канал ремапа не ревёрснут; для Ajazz — реализован в драйвере (по wsclx), но на ревизии `0C45:8009` канал `0xFF68` не отвечает, поэтому ремап там пока недоступен. По реверсу официального ПО keymap Ajazz живёт на рабочем `0xFF13` (`0x11` base / `0x27` Fn, 576 Б на слой) — это и есть путь к рабочему ремапу (этап 3); **чтения keymap нет**, поэтому перед первым remap нужен авто-бэкап. Заводская base-раскладка (81 клавиша) уже захардкожена из официального установщика (ассет `data/ajazz/rgb-keyboard.xml` → `factory_keymap_ak820()`), заводского Fn-слоя в установщике нет.
- **Ролик/энкодер не настраивается** — у F75 ролик аппаратно жёстко закреплён за громкостью (команды переназначения в протоколе нет), у **AULA F99 энкодера/ролика НЕТ** (подтверждено живой диагностикой), у Ajazz AK820 Pro он firmware-fixed (Volume/Mute). GUI больше не предлагает настройку ролика.
- **Screen Manager** — только для Ajazz AK820 Pro. На проверенной ревизии `0C45:8009` интерфейс TFT `0xFF67` **отсутствует**, поэтому загрузка картинок/GIF на этой модели невозможна (код оставлен для других ревизий).

---

## Roadmap

- [x] AULA F75 USB: direct-RGB, host-эффекты, сохранение в память, свет выкл
- [x] AULA F75 2.4G: батарея, эффекты/яркость/скорость в память ресивера
- [x] AULA F75: ремап (протокол + карта слотов подтверждены дампом; CLI `kbc remap`)
- [x] GUI RGB-студия: пресеты, слайдеры, автосохранение при закрытии
- [x] Аудио-реактив (VU-спектр системного звука): USB, верхний ряд или столбики cava, градиент зелёный→синий; `kbc rgb --mode audio`, встроенные пресеты `audio-viz`/`cava` и фоновый `kbc daemon` (без GUI)
- [x] AULA F99: базовый USB/2.4G-хост-рендеринг, эффекты и аудио-реактив подтверждены; гайд для владельцев — `docs/f99-guide.md`
- [ ] AULA F99: довести дополнительные каналы (ремап, индикаторы, polling/debounce/sleep)
- [x] Ajazz AK820 Pro: подсветка (20 режимов), синк времени и сон через control-канал `0xFF13` — подтверждено на живом железе
- [x] Ajazz AK820 Pro: per-key подсветка (таблица `0x23`, live-кадры `0x20`) и заряд АКБ — реализованы по реверсу официального ПО
- [x] Ajazz AK820 Pro: заводская base-раскладка (81 клавиша) захардкожена из официального установщика (ассет `data/ajazz/rgb-keyboard.xml` → `factory_keymap_ak820()`)
- [ ] Ajazz AK820 Pro: живая проверка per-key и заряда
- [ ] Ajazz AK820 Pro: keymap/Fn через `0xFF13` (этап 3; нужен авто-бэкап — readback'а нет)
- [ ] Ajazz AK820 Pro: макросы через `0xFF13` (`0x19`+`0x15`, этап 4; формат найден в реверсе, нужен живой тест с бэкапом) и polling/debounce (этап 7) — ждут usbmon официального ПО
- [ ] Ajazz AK820 Pro: TFT через `0xFF68` (этап 5) и запись через 2.4G-донгл `0C45:FDFD` (этап 6)
- [x] Финальная документация для сборки на Fedora (без флейка): Makefile + fedora.sh (установка/обновление/удаление)

---

## Поддержка / обратная связь

Проект персональный (автор — darkside). Проблемы можно репортить в трекер репозитория: [altlinux.space/dark-siders/keyboard-control](https://altlinux.space/dark-siders/keyboard-control).

### У вас AULA F99 или Ajazz AK820 Pro?

**Ajazz AK820 Pro** уже проверен на живом железе ревизии `0C45:8009`: подсветка (20 режимов), синк времени и сон работают через control-канал `0xFF13`; per-key подсветка (`0x23` сохранённая таблица / `0x20` живой кадр) и заряд АКБ реализованы по реверсу официального ПО и ждут живой проверки; заводская base-раскладка (81 клавиша) захардкожена из официального установщика (ассет `data/ajazz/rgb-keyboard.xml`). Keymap и макросы, по реверсу официального ПО, тоже живут на `0xFF13` (`0x11`/`0x27` и `0x19`/`0x15`) — команды известны, но код ещё не переведён туда; TFT-интерфейс `0xFF67` на этой ревизии отсутствует. Оставшиеся этапы (keymap и макросы через `0xFF13`, TFT, 2.4G-донгл) расписаны в [docs/ajazz-guide.md](docs/ajazz-guide.md), раздел A1. Железа других ревизий (`0x800A`, `0x8099`) у проекта нет. F99 проверен на отдельном реальном устройстве, но дополнительные каналы F99 всё ещё требуют подтверждения. Если у вас другая ревизия Ajazz или нужна дополнительная проверка F99, помогите её подтвердить:

```bash
git clone https://altlinux.space/dark-siders/keyboard-control && cd keyboard-control
# один раз собрать kbc (см. docs/collect-data.md, раздел 1 — NixOS или Fedora),
# затем:
./scripts/collect-data.sh
# результат: ~/kbc-diag-ГГГГММДД-ЧЧММСС.tar.gz
```

Скрипт по умолчанию собирает только **read-only** диагностику (не меняет настроек клавиатуры): `kbc list/info/probe/battery`, дампы конфиг-региона (F99), `lsusb` и uevent. Единственное исключение — флаг `--write-tests` **только для владельцев AULA F99**: включает аккуратные write-тесты T1–T3 (каждый тест восстанавливает исход; `collect-data.sh` делегирует их в `scripts/f99-usb-rev.sh`). Подробности — в **[docs/collect-data.md](docs/collect-data.md)**. Пришлите архив — это займёт 5 минут.

**Для владельцев AULA F99** есть отдельное руководство по поддержке клавиатуры — **[docs/f99-guide.md](docs/f99-guide.md)**: что уже работает, включая USB-аудио в режимах `row`/`bars`, какие команды ещё прогнать для индикаторов и ремапа, и подробный разбор формата передачи подсветки (для интерпретатора GIF/анимаций).

**Для владельцев Ajazz AK820 Pro** — **[docs/ajazz-guide.md](docs/ajazz-guide.md)** и отдельный read-only скрипт `./scripts/ajazz-usb-diag.sh` (только GET-команды + `kbc ajazz-diag`, архив `~/kbc-diag-ajazz-*.tar.gz`). Драйвер Ajazz проверен на живом железе ревизии `0C45:8009`; неподтверждённые каналы (TFT, 2.4G-запись, а также keymap/макросы — команды известны на `0xFF13`, но код ещё не переведён) честно отдают `Unsupported`.

---

Maintained by darkside 🐾
