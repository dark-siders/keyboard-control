#!/usr/bin/env bash
# -*- coding: utf-8 -*-
# Сбор дампа слотов keymap AULA F99 (USB 258A:010C) для GUI click-редактора.
# Аналог того, что делали для F75 (журнал 10.72).
# Запуск: ./scripts/f99-slot-dump.sh [--sg]

set -euo pipefail

PID_USB="258A:010C"
USE_SG=false
KBC_BIN="kbc"

for arg in "$@"; do
    case "$arg" in
        --sg) USE_SG=true ;;
        --kbc=*) KBC_BIN="${arg#--kbc=}" ;;
        *) echo "Неизвестный аргумент: $arg"; exit 1 ;;
    esac
done

run_kbc() {
    if $USE_SG; then
        local cmd
        cmd="$(printf '%q ' "$KBC_BIN" "$@")"
        sg plugdev -c "$cmd"
    else
        "$KBC_BIN" "$@"
    fi
}

echo "=== F99 Keymap Slot Dump ==="
echo "PID: $PID_USB"
echo "Бин: $KBC_BIN"
echo "sg: $USE_SG"
echo

# 1. Проверка наличия устройства
echo "1. Поиск устройства..."
if ! run_kbc list | grep -q "258A:010C"; then
    echo "❌ Устройство 258A:010C не найдено"
    exit 1
fi
echo "✓ Устройство найдено"

# 2. Чтение keymap Default (layer 0)
echo
echo "2. Чтение keymap Default (layer 0)..."
run_kbc raw "$PID_USB" feature-send "06 83 00 00 01 00 00 02 $(for ((i=0;i<512;i++)); do printf ' 00'; done)" >/dev/null 2>&1
DEFAULT_RAW=$(run_kbc raw "$PID_USB" feature-get 06 520 | head -1)
echo "$DEFAULT_RAW" > /tmp/f99_keymap_default_raw.txt
echo "✓ Default сохранён в /tmp/f99_keymap_default_raw.txt"

# 3. Чтение keymap FN1 (layer 1)
echo
echo "3. Чтение keymap FN1 (layer 1)..."
run_kbc raw "$PID_USB" feature-send "06 83 00 01 01 00 00 02 $(for ((i=0;i<512;i++)); do printf ' 00'; done)" >/dev/null 2>&1
FN1_RAW=$(run_kbc raw "$PID_USB" feature-get 06 520 | head -1)
echo "$FN1_RAW" > /tmp/f99_keymap_fn1_raw.txt
echo "✓ FN1 сохранён в /tmp/f99_keymap_fn1_raw.txt"

# 4. Парсинг слотов (128 слотов × 4 байта = 512 байт)
echo
echo "4. Парсинг слотов..."
parse_slots() {
    local raw="$1"
    local layer_name="$2"
    local out_file="$3"

    # raw = "06 83 00 00 01 00 00 02 XX XX XX ... (520 байт)"
    # данные начинаются с 9-го поля (индекс 8 в 0-based), 512 байт = 512 полей
    local fields=($raw)
    if [[ ${#fields[@]} -lt 520 ]]; then
        echo "⚠ Неполный ответ: ${#fields[@]} байт (нужно 520)"
        return 1
    fi

    echo "# $layer_name keymap slots (128 slots × 4 bytes)" > "$out_file"
    echo "# Format: slot_index | page,0,0,usage | decoded" >> "$out_file"
    echo "" >> "$out_file"

    for ((slot=0; slot<128; slot++)); do
        local off=$((8 + slot * 4))
        local page="${fields[$off]}"
        local b1="${fields[$((off+1))]}"
        local b2="${fields[$((off+2))]}"
        local usage="${fields[$((off+3))]}"

        local decoded=""
        case "$page" in
            0x00)
                if [[ "$usage" == "0x00" || "$usage" == "00" ]]; then
                    decoded="UNBIND"
                else
                    decoded="Press(0x$usage)"
                fi
                ;;
            0x02)
                decoded="Media(0x$usage)"
                ;;
            0x0D|0x0E|0x0F) # служебные page (Fn и др.)
                decoded="Service(page=$page, usage=0x$usage)"
                ;;
            *)
                decoded="Unknown(page=$page, usage=0x$usage)"
                ;;
        esac

        printf "slot %3d | %02X %02X %02X %02X | %s\n" "$slot" \
            "0x$page" "0x$b1" "0x$b2" "0x$usage" "$decoded" >> "$out_file"
    done
}

parse_slots "$DEFAULT_RAW" "Default" /tmp/f99_slots_default.txt
parse_slots "$FN1_RAW" "FN1" /tmp/f99_slots_fn1.txt

echo "✓ Разбор слотов: /tmp/f99_slots_default.txt, /tmp/f99_slots_fn1.txt"

# 5. Поиск известных клавиш (F1-F12, Fn, спец-клавиши)
echo
echo "5. Поиск известных паттернов..."
echo "=== F1-F12 (ожидаемые слоты по F75: 12,18,24,30,36,42,48,54,60,66,72,78) ===" | tee -a /tmp/f99_slots_default.txt
for s in 12 18 24 30 36 42 48 54 60 66 72 78; do
    line=$(sed -n "$((s+4))p" /tmp/f99_slots_default.txt 2>/dev/null || echo "slot $s: НЕТ ДАННЫХ")
    echo "$line" | tee -a /tmp/f99_slots_default.txt
done

echo
echo "=== Fn key (ожидаемый слот 53 по F75) ===" | tee -a /tmp/f99_slots_default.txt
line=$(sed -n "57p" /tmp/f99_slots_default.txt 2>/dev/null || echo "slot 53: НЕТ ДАННЫХ")
echo "$line" | tee -a /tmp/f99_slots_default.txt

# 6. Сборка итогового архива
OUT_DIR="$HOME/f99-slot-dump-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$OUT_DIR"
cp /tmp/f99_keymap_default_raw.txt "$OUT_DIR/01-keymap-default-raw.txt"
cp /tmp/f99_keymap_fn1_raw.txt "$OUT_DIR/02-keymap-fn1-raw.txt"
cp /tmp/f99_slots_default.txt "$OUT_DIR/03-slots-default.txt"
cp /tmp/f99_slots_fn1.txt "$OUT_DIR/04-slots-fn1.txt"

# Добавим info
run_kbc info "$PID_USB" > "$OUT_DIR/00-info.txt" 2>&1 || true
run_kbc list > "$OUT_DIR/00-list.txt" 2>&1 || true

tar -czf "$OUT_DIR.tar.gz" -C "$(dirname "$OUT_DIR")" "$(basename "$OUT_DIR")" 2>/dev/null
echo
echo "=== ГОТОВО ==="
echo "Каталог: $OUT_DIR"
echo "Архив:   $OUT_DIR.tar.gz"
echo
echo "Пришлите архив владельцу Keyboard-Center для добавления F99_SLOT_LABELS."