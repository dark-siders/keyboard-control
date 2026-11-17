# CLI `kbc`, демон и пресеты

Помимо GUI `keyboard-center`, проект поставляет консольную утилиту `kbc` — для скриптов, быстрых операций и диагностики протокола. Установка — см. [install.md](install.md).

## Примеры

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
kbc time 0C45:8009                          # синхронизация времени (Ajazz)
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
# TFT: control 0xFF13 + data 0xFF68, ACK на каждый чанк (протокол Hardware-verified на 0C45:800A, PR #2).
kbc screen 0C45:8009 --image anim.gif         # картинка или GIF до 20 кадров (анимация уходит целиком)
kbc screen 0C45:8009 --brightness 200         # яркость TFT-экрана (можно вместе с --image)
kbc macro 0C45:8009 --set 0 --keys 04,05,06   # прошить макрос №0: «A», «B», «C» (press+release)
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

## Справочник команд

Полный перечень сабкоманд CLI (контракт, стабилен для применения; флаги отмечены `*` — experimental/для 2.4G-ресивера):

| Команда | Назначение | Ключевые флаги |
|---|---|---|
| `kbc list` | список подключённых устройств | — |
| `kbc info <VID:PID>` | детали устройства (тип, модель) | — |
| `kbc rgb <VID:PID>` | подсветка: host-эффект (USB) или запись в память ресивера (2.4G) | `--mode` (вкл. `audio` — спектр системного звука, USB-only), `--style row\|bars` (аудио), `--color RRGGBB`, `--brightness`, `--speed`, `--hold ms`, `--light-on`, `--light-off` (2.4G), `--hw-effect HEX`*, `--color-low RRGGBB`, `--color-high RRGGBB`, `--sensitivity 0.1..2.0` (аудио)* |
| `kbc screen <VID:PID>` | TFT-экран Ajazz: кадр/GIF-анимация и/или яркость | `--image <path>` (картинка или GIF до 20 кадров), `--brightness 0..255` |
| `kbc time <VID:PID>` | синхронизация времени ПК с экраном (Ajazz) | — |
| `kbc battery <VID:PID>` | заряд 0..100% (только 2.4G; USB/BT — инфо-сообщение) | — |
| `kbc save <VID:PID>` | эффект/конфиг в память клавиатуры (держится без приложения) | `--effect HEX`, `--brightness`, `--speed` |
| `kbc remap <VID:PID>` | переназначение клавиш (F75, USB; верифицировано) | `--list`, `--key slot`, `--to usage`, `--media usage`, `--modifier 0..7`, `--layer 0\|1` |
| `kbc reset <VID:PID>` | сброс раскладки к заводской (Default + FN1) — как «Сбросить всё» в GUI | — |
| `kbc macro <VID:PID>` | прошивка/запись макроса (Ajazz AK820 Pro, USB) | `--set N` (0..99) + `--keys 04,05,06` (HID usage в hex), `--record N` (1..12), `--stop` |
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

`kbc macro --set` (только Ajazz по USB) шьёт макрос через рабочий канал `0xFF13` по рамке официального драйвера: каждая клавиша даёт press+release, минимальная задержка между событиями 10 мс, слоты `0..99`, лимит одного макроса — 320 байт. Прошивка **не отдаёт макросы на чтение**, поэтому драйвер держит их кэш только в памяти процесса: при записи одного слота пересобирается вся таблица, и слоты, не записанные в этом же запуске `kbc`, будут очищены. Запись на железе пока не проверена — делайте бэкап существующих макросов (например, через официальный драйвер) перед прошивкой.

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

## Формат TOML-пресета

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
