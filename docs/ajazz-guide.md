# Ajazz AK820 Pro: руководство по поддержке + read-only диагностика

> **Кому:** владельцу **Ajazz AK820 Pro** (ревизии USB `0C45:8009` / `0C45:800A` / `0C45:8099`), у которого есть реальное железо.
>
> **Зачем:** у разработчика проекта эта клавиатура физически ОТСУТСТВУЕТ. Драйвер `src/devices/ajazz_ak820.rs` написан по открытому ресёрчу (wsclx / ak820pro-modder, протокол fw 1.07) и **на живом железе ещё не проверялся** (журнал 10.53/10.54/10.60/10.63). Ваши read-only снимки позволят подтвердить или поправить драйвер.
>
> **ВАЖНО:** этот документ и скрипт — **ТОЛЬКО ЧТЕНИЕ**. Ни одной write-команды в клавиатуру не отправляется: сбор делает лишь GET-запросы и системные команды (`lsusb`). Запись (TFT-картинки, макросы, keymap, sleep, синк времени) в этом гайде не запускается.

---

## A. Что уже есть в драйвере (по ресёрчу, НЕ проверено на железе)

Ajazz AK820 Pro — Sonix SN32F299, протокол wsclx (транспорт `0xAA`/`0x55`). Реализовано в `src/devices/ajazz_ak820.rs`:

- **Транспорт** — 64-байтные output-репорты, ответ `0x55` (см. раздел C);
- **Подсветка** — аппаратные режимы (Static/Breath/Spectrum/…) + per-key custom-LED буфер 128×4 (512 Б);
- **Keymap** — 128 слотов × 4 Б, базовый слой + Fn-слой + заводские матрицы (read-only);
- **Макросы** — index 400 Б + data-блоки (чтение/запись);
- **TFT 128×128** — загрузка кадров/GIF (RGB565 LE, заголовок 256 Б), синк времени;
- **Батарея** — из device-info (байт `[17]`);
- **Сон** — 4 ступени через control-канал `0xFF13`.

Три HID-интерфейса (usage pages): **control `0xFF13`**, **data/командный `0xFF68`**, **TFT `0xFF67`**. Драйвер сам выбирает нужный интерфейс по usage page.

### A2. Открытые вопросы (что нужно подтвердить)

1. Совпадают ли **usage pages** и число интерфейсов с ожидаемыми (control `0xFF13`, data `0xFF68`, TFT `0xFF67`)?
2. Отвечает ли транспорт `0xAA`/`0x55` на GET-команды (device-info, LED, keymap, макросы)?
3. Где именно лежит **батарея** в device-info (предположительно `[17]`)?
4. Читаются ли **keymap Default/FN1** и заводские матрицы (`0x1F`/`0x1C`)?
5. Формат **LED-эффекта** (16-байтный payload) — совпадает ли с SET-форматом?
6. Макросы: формат index/data на реальной прошивке.

---

## B. Быстрый способ: скрипт `scripts/ajazz-usb-diag.sh` (read-only)

Скрипт выполняет только чтение и упаковывает результат:

```bash
# 1. один раз собрать kbc:
nix develop . --command cargo build --release      # NixOS
# либо: make release                                # Fedora/дистрибутив без nix

# 2. запуск (--sg — если группа plugdev ещё не применилась после перелогина):
./scripts/ajazz-usb-diag.sh
./scripts/ajazz-usb-diag.sh --sg
./scripts/ajazz-usb-diag.sh --device 0C45:8009     # если автодетект не сработал
```

Результат: папка **`~/kbc-diag-ajazz-ГГГГММДД-ЧЧММСС/`** и одноимённый **`~/kbc-diag-ajazz-….tar.gz`**. Отправьте именно `.tar.gz`.

Что внутри:

| Файл | Что это |
|---|---|
| `00-обзор.txt` | окружение (дата, ОС, путь к `kbc`) |
| `01-kbc-list.txt` | вывод `kbc list` — видит ли сканер клавиатуру и её PID |
| `02-info.txt` | `kbc info` — модель, подключение, батарея |
| `03-probe.txt` | `kbc probe` — интерфейсы hidraw и usage pages |
| `04-battery.txt` | `kbc battery` — заряд |
| `05-ajazz-diag.txt` | **ГЛАВНОЕ**: `kbc ajazz-diag` — read-only дамп протокола |
| `06-keymap-list.txt` | `kbc remap --list` — читаемая карта keymap Default+FN1 |
| `07-lsusb.txt` | `lsusb` |
| `08-uevent.txt` | uevent hidraw (usage pages) |
| `09-сводка.txt` | что где лежит и на что смотреть |

> Тот же дамп добавляется в общий сбор `scripts/collect-data.sh --device ajazz` (файл `28-ajazz-diag.txt`).

---

## C. Команда `kbc ajazz-diag` и формат дампа

`kbc ajazz-diag <VID:PID>` открывает командный интерфейс (`0xFF68`) и последовательно выполняет read-only GET-команды. Формат вывода — по строке на секцию, `ключ=hex`; если команда не ответила — `ключ_error=…`, дамп продолжается.

| Ключ | Команда | Размер | Смысл |
|---|---|---|---|
| `device_info` | `GET_DEVICE_INFO` (16) | 48 Б | info-структура; `battery=` — байт `[17]` |
| `led_effect` | `GET_LED_EFFECT` (19) | 16 Б | текущий глобальный эффект; `led_effect_mode=` — байт `[0]` |
| `game_mode` | `GET_GAME_MODE` (17) | 56 Б | game-mode-блок (задел) |
| `custom_led` | `GET_CUSTOM_LED_DATA` (20) | 512 Б | per-key буфер 128 × RGBA |
| `keymap_base` | `GET_KEY` (18) | 512 Б | текущий базовый слой |
| `keymap_fn` | `GET_FN_KEY` (22) | 512 Б | текущий Fn-слой |
| `keymap_default_base` | `GET_DEFAULT_KEY_MATRIX` (31) | 512 Б | заводской базовый слой |
| `keymap_default_fn` | `GET_DEFAULT_FN_KEY_MATRIX` (28) | 512 Б | заводской Fn-слой |
| `macros_index` | `GET_MACRO` (21) | 400 Б | index макросов (100 слотов × 4 Б LE) |
| `macros_count`, `macro_<id>` | `GET_MACRO` (21) | — | число и декодированные события макросов |

Декодирование, которое делает команда (только достоверное):
- `battery=` — `device_info[17]` (0..100);
- `led_effect_mode=0xNN` — первый байт LED-эффекта (`0x00..0x13` аппаратные, `0x80` Custom);
- `macro_<id>=delay:keycode:family:press,…` — события макроса.

> Все команды — **GET**. Команды `SET_*` (34/35/36/37/38), `SET_FACTORY_RESET` (15), TFT (80/81) и control-канал `0xFF13` этой командой **не используются**.

### Ручной запуск отдельных шагов (если нужно точечно)

```bash
KBC=target/release/kbc

$KBC list
$KBC info 0C45:8009
$KBC probe 0C45:8009          # ожидаем control 0xFF13 / data 0xFF68 / TFT 0xFF67
$KBC battery 0C45:8009        # заряд (у Ajazz по USB читается, в отличие от AULA)
$KBC ajazz-diag 0C45:8009     # полный read-only дамп
$KBC remap 0C45:8009 --list   # карта keymap Default+FN1
lsusb
```

---

## D. Протокол (для тех, кто хочет проверить вручную)

### D1. Транспорт

64-байтные **output-репорты**, `REPORT_ID = 0`. Кадр запроса:

```
[0] = 0xAA        (MAGIC_OUTGOING)
[1] = cmd         (команда)
[2] = len         (длина полезной нагрузки)
[3] = addr_lo     (адрес, младший байт)
[4] = addr_hi     (адрес, старший байт)
[5] = 0
[6] = last        (1 — последний чанк)
[7] = 0
[8..64) = payload (до 56 байт)
```

Ответ приходит как input-репорт, начинается с `0x55` (`MAGIC_INCOMING`), команда — в байте `[1]`, данные — с байта `[8]`.

### D2. Команды (GET — read-only; SET — сюда НЕ входят)

| Код | Имя | Код | Имя |
|---|---|---|---|
| `16` (0x10) | GET_DEVICE_INFO | `34` (0x22) | SET_KEY |
| `17` (0x11) | GET_GAME_MODE | `35` (0x23) | SET_LED_EFFECT |
| `18` (0x12) | GET_KEY | `36` (0x24) | SET_CUSTOM_LED_DATA |
| `19` (0x13) | GET_LED_EFFECT | `37` (0x25) | SET_MACRO |
| `20` (0x14) | GET_CUSTOM_LED_DATA | `38` (0x26) | SET_FN_KEY |
| `21` (0x15) | GET_MACRO | `15` (0x0F) | SET_FACTORY_RESET |
| `22` (0x16) | GET_FN_KEY | `80` (0x50) | SET_TFT_USER_ANIMATION |
| `28` (0x1C) | GET_DEFAULT_FN_KEY_MATRIX | `81` (0x51) | SET_TFT_BUILT_IN_INDEX |
| `31` (0x1F) | GET_DEFAULT_KEY_MATRIX | | |

### D3. Многочанковые блоки (512 Б keymap, 512 Б custom-LED, макросы)

Большие блоки читаются пачками по **56 байт**: для каждого чанка отправляется кадр с `addr = база + i*56`, `len = размер чанка`, `last = 1` только на последнем. Драйвер ждёт ответ с той же командой и склеивает payload’ы (`get_many`/`get_many_at`).

### D4. Keymap-слот (4 байта)

| Байт 0 (page) | Смысл | Декодирование |
|---|---|---|
| `0` | Default/пусто | `None` |
| `2` | клавиша | `Press(bytes[2])` |
| `3` | media/consumer | `Media(bytes[1] | bytes[2]<<8)` |
| `6` | макрос | `Macro(bytes[1])` |
| `10` | Fn-слой | `FnLayer(bytes[1])` |

Модификатор кодируется как page `2` + `0xE0+m` (поэтому декодируется обратно как `Press(0xE1…)`, а не `Modifier` — особенность формата wsclx).

---

## E. Что искать в присланном дампе

- **`03-probe.txt`**: есть ли интерфейсы с usage pages `0xFF13`, `0xFF68`, `0xFF67`. Расхождение — важный сигнал о другой ревизии/прошивке.
- **`05-ajazz-diag.txt`**:
  - `battery=` — правдоподобные 0..100?
  - `device_info=…` — первые байты (прошивка/версия, если видно);
  - `led_effect_mode=0xNN` — совпадает с текущим визуальным эффектом?
  - `keymap_base` / `keymap_fn` — не нули ли (нули = канал не читается);
  - `macros_count` и `macro_*`;
  - строки `*_error=Protocol` — какая именно команда не ответила.
- **`06-keymap-list.txt`**: ненулевые назначения слотов (или «все Default»).
- **`08-uevent.txt`**: соответствие hidraw ↔ usage page.

Если какая-то команда вернула `Protocol error` — **это тоже результат** (канал отличается от ожидаемого). Приложите вывод как есть, не «чистите».

---

## F. Участие в проекте

Проект MIT, вклад приветствуется. Присылайте архив `~/kbc-diag-ajazz-*.tar.gz` (issue/комментарий в репозитории) — по нему правятся драйвер и сканер. Позже, после подтверждения read-only канала, можно будет отдельно договориться о **write-тестах** (TFT, макросы, keymap) — но только с явного согласия и с откатом, как это сделано для AULA F99 (`scripts/f99-usb-rev.sh`).

---

## Шпаргалка по командам

| Задача | Команда |
|---|---|
| Свежая сборка | `nix develop . --command cargo build --release` |
| Полный read-only сбор | `./scripts/ajazz-usb-diag.sh --sg` |
| Дамп протокола | `target/release/kbc ajazz-diag 0C45:8009` |
| Карта клавиш | `target/release/kbc remap 0C45:8009 --list` |
| Разведка интерфейсов | `target/release/kbc probe 0C45:8009` |
| Батарея | `target/release/kbc battery 0C45:8009` |
| Общий сбор (F99/Ajazz) | `./scripts/collect-data.sh --device ajazz --sg` |
