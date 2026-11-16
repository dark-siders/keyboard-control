#!/usr/bin/env bash
# ─── Авто-диагностика Ajazz AK820 Pro (USB/2.4G-донгл, ТОЛЬКО ЧТЕНИЕ) ───
# Владельцам реального железа: запустить и прислать архив из ~/kbc-diag-*/
#
# Скрипт НИЧЕГО не пишет в клавиатуру. Он выполняет только GET-команды:
#   kbc list / info / probe / battery
#   kbc ajazz-diag   — read-only дамп протокола (device-info, LED-эффект,
#                      game-mode, custom-LED буфер, keymap Default/FN1 +
#                      заводские матрицы, макросы; команды 16/17/19/20/21/22/28/31)
#   kbc remap --list — читаемая карта keymap (GET_KEY/GET_FN_KEY)
# Записи (kbc save / rgb --light-* / set-* / time / screen / remap без --list)
# скрипт НЕ выполняет.
#
# Параметры:
#   --device VID:PID   устройство явно (по умолчанию — автодетект по kbc list)
#   --sg               запускать kbc через `sg plugdev -c` (если нет перелогина)
#   -h|--help          эта справка
#
# Примеры:
#   ./scripts/ajazz-usb-diag.sh
#   ./scripts/ajazz-usb-diag.sh --sg
#   ./scripts/ajazz-usb-diag.sh --device 0C45:8009
#
# Результат: папка $HOME/kbc-diag-ГГГГММДД-ЧЧММСС/ + одноимённый tar.gz в $HOME.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KBC=""

DEVICE=""   # автодетект по умолчанию
USE_SG=0

# --- поиск бинарника kbc -------------------------------------------------
# ВАЖНО: `type -P`, а НЕ `command -v` — ниже объявлена функция `kbc`,
# поэтому `command -v kbc` вернёт имя функции (рекурсия → SIGSEGV).
find_kbc() {
    if [[ -x "$ROOT/target/release/kbc" ]]; then
        KBC="$ROOT/target/release/kbc"
        echo "[i] использую локальную сборку: $KBC" >&2
    elif type -P kbc >/dev/null 2>&1; then
        KBC="$(type -P kbc)"
        echo "[i] использую системный kbc: $KBC" >&2
    else
        echo "не найден бинарник kbc. Соберите его (один раз) и повторите:" >&2
        echo "  make release" >&2
        echo "  # либо: nix develop . --command cargo build --release" >&2
        exit 1
    fi
}

# --- вызов kbc (с учётом --sg) -------------------------------------------
kbc() {
    if [[ "$USE_SG" -eq 1 ]]; then
        local cmd
        cmd="$(printf '%q ' "$KBC" "$@")"
        sg plugdev -c "$cmd"
    else
        "$KBC" "$@"
    fi
}

# --- автодетект Ajazz по kbc list ----------------------------------------
detect_device() {
    local out pid
    echo "kbc list — автодетект Ajazz..." >&2
    out="$(kbc list 2>&1 || true)"
    if pid="$(printf '%s\n' "$out" | grep -oE '0C45:[0-9A-Fa-f]{4}' | head -n1)" && [[ -n "$pid" ]]; then
        DEVICE="$pid"
    else
        echo "не удалось автоматически найти Ajazz (ожидается VID 0C45)." >&2
        echo "Запустите с явным указанием: $0 --device 0C45:8009" >&2
        echo "--- вывод kbc list ---" >&2
        echo "$out" >&2
        exit 1
    fi
    echo "Определено устройство: Ajazz AK820 Pro ($DEVICE)" >&2
}

# --- главная сборка -------------------------------------------------------
collect() {
    local dir="$1"
    echo "Сборка данных в $dir ..."

    {
        echo "Keyboard-Center диагностика Ajazz AK820 Pro (USB) — $(date '+%Y-%m-%d %H:%M:%S %Z')"
        echo "Устройство: $DEVICE"
        echo "Утилита:    $KBC"
        echo "ОС:         $(uname -a)"
        echo
        echo "Режим: ТОЛЬКО ЧТЕНИЕ (ни одной write-команды не отправлено)."
    } > "$dir/00-обзор.txt"

    echo "[1] kbc list"
    kbc list > "$dir/01-kbc-list.txt" 2>&1 || true

    echo "[2] kbc info ($DEVICE)"
    kbc info "$DEVICE" > "$dir/02-info.txt" 2>&1 || true

    echo "[3] kbc probe ($DEVICE) — интерфейсы/usage pages"
    kbc probe "$DEVICE" > "$dir/03-probe.txt" 2>&1 || true

    echo "[4] kbc battery ($DEVICE)"
    kbc battery "$DEVICE" > "$dir/04-battery.txt" 2>&1 || true

    echo "[5] kbc ajazz-diag — read-only дамп протокола (главный шаг)"
    if kbc ajazz-diag "$DEVICE" > "$dir/05-ajazz-diag.txt" 2>&1; then
        echo "  ок: $(wc -l < "$dir/05-ajazz-diag.txt") строк"
    else
        echo "  [!] kbc ajazz-diag завершился с ошибкой — вывод сохранён как есть" >&2
    fi

    echo "[6] kbc remap --list — читаемая карта keymap (Default+FN1)"
    kbc remap "$DEVICE" --list > "$dir/06-keymap-list.txt" 2>&1 \
        || echo "# kbc remap --list: не удалось (keymap-чтение не поддержано)" > "$dir/06-keymap-list.txt"

    echo "[7] система (lsusb, uevent)"
    lsusb > "$dir/07-lsusb.txt" 2>&1 || true
    : > "$dir/08-uevent.txt"
    for h in /sys/class/hidraw/hidraw*/device/uevent; do
        [[ -f "$h" ]] || continue
        {
            echo "=== $h ==="
            grep -E 'HID_ID|HID_NAME|DRIVER|PRODUCT' "$h" || cat "$h"
        } >> "$dir/08-uevent.txt"
    done

    # Сводка — что куда легло
    {
        echo "Собранные файлы:"
        ls -1 "$dir"
        echo
        echo "Что проверять:"
        echo "  * 01-kbc-list.txt   — видит ли сканер клавиатуру (PID ревизии 8009/800A/8099)"
        echo "  * 02-info.txt       — модель, подключение, батарея"
        echo "  * 03-probe.txt      — интерфейсы/usage pages (ожидание: control 0xFF13, data 0xFF68, TFT 0xFF67)"
        echo "  * 04-battery.txt    — заряд (у Ajazz USB читается, device-info[17])"
        echo "  * 05-ajazz-diag.txt — ГЛАВНОЕ: device_info/led_effect/game_mode/custom_led/"
        echo "                        keymap_*/macros_* (ключ=hex; *_error=… — команда не ответила)"
        echo "  * 06-keymap-list.txt — читаемая карта keymap Default+FN1 (GET_KEY/GET_FN_KEY)"
        echo "  * 07-lsusb.txt      — дерево USB (0C45:…)"
        echo "  * 08-uevent.txt     — usage pages hidraw"
        echo
        echo "Подсказки по разбору:"
        echo "  * батарея:   строка battery= в 05-ajazz-diag.txt (0..100)"
        echo "  * эффект:    led_effect_mode=0xNN (0x00..0x13 — аппаратные, 0x80 — Custom)"
        echo "  * если все секции *_error=Protocol — прошивка отвечает не на cmd 0xAA/0x55:"
        echo "    пришлите 03-probe.txt и 08-uevent.txt, это тоже важный результат."
    } > "$dir/09-сводка.txt"

    echo "Готово. Данные в: $dir"
}

# --- основной ход ---------------------------------------------------------
while [[ $# -gt 0 ]]; do
    case "$1" in
        --device) DEVICE="$2"; shift 2 ;;
        --sg)     USE_SG=1; shift ;;
        -h|--help)
            sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) echo "неизвестный аргумент: $1 (см. './scripts/ajazz-usb-diag.sh --help')" >&2; exit 2 ;;
    esac
done

find_kbc

if [[ -z "$DEVICE" ]]; then
    detect_device
fi
if [[ ! "$DEVICE" =~ ^0C45:[0-9A-Fa-f]{4}$ ]]; then
    echo "предупреждение: $DEVICE не похож на Ajazz (ожидается 0C45:XXXX), продолжаю как указано" >&2
fi

STAMP="$(date '+%Y%m%d-%H%M%S')"
DIR="$HOME/kbc-diag-ajazz-${STAMP}"
mkdir -p "$DIR"

trap 'echo "данные частично сохранены в $DIR" >&2' ERR

collect "$DIR"

ARCHIVE="$HOME/kbc-diag-ajazz-${STAMP}.tar.gz"
if command -v tar >/dev/null 2>&1; then
    tar -C "$HOME" -czf "$ARCHIVE" "kbc-diag-ajazz-${STAMP}"
    echo "Архив для отправки: $ARCHIVE"
else
    echo "tar не найден — отправьте папку как есть: $DIR"
fi

cat <<EOF

Что дальше:
  1. Отправьте владельцу проекта архив: $ARCHIVE
  2. (или папку $DIR целиком)

ВАЖНО: этот скрипт ничего не записывал в клавиатуру — только чтение.
EOF
