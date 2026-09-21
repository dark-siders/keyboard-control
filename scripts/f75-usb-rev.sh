#!/usr/bin/env bash
# Скрипт реверс-диагностики USB-канала AULA F75 (258A:010C).
#
# Назначение: записать серию read-only замеров и (по флагу) интерактивных
# write-тестов T1–T5, чтобы закрыть открытые вопросы протокола F75:
#   * T1 — аппаратная запись конфига 06 04 (round-trip селектора data[10]) + latch
#   * T2 — охота за селектором self-define (режим per-key): полный перебор data[10] ∈ 0x01..0x1F
#   * T3 — индикаторы Caps/Num/Scroll через boot output-report 0x02
#   * T4 — direct-канал: простой кадр 06 08 vs enable-seq (0x39/0x3c/0x39) + disable
#   * T5 — поиск начала эффект-таблицы (offset 58 vs 64): дифф снимков при Fn+\ и Fn+Вниз
#
# Параметры:
#   --device f75        устройство (поддерживается только f75)
#   --sg                оборачивать команды через "sg plugdev -c ..."
#   --write-tests       выполнять интерактивные write-тесты T1–T5 (каждый
#                       восстанавливает исходные данные, маркер 5A A5 НЕ трогается)
#   --auto              без интерактивных вопросов (write-тесты пропускаются)
#   -h|--help           эта справка
#
# Результат: ~/kbc-diag-<дата-время>/ + одноимённый .tar.gz.
# Это РЕВЕРС-ИНСТРУМЕНТ: не запускайте без понимания, что происходит.
#
# Форматы пакетов (из collect-data.sh / f99-usb-rev.sh):
#   чтение конфига:   SET 06 84 00 00 01 00 80 00 (+пад до 520) -> GET 06 520
#   чтение цветов:    SET 06 8A 00 00 01 00 00 02 (512 б, ровно 520) -> GET 06 520
#   чтение keymap:    SET 06 83 00 <слой> 01 00 00 02 (+пад до 520) -> GET 06 520
#   запись конфига:   SET 06 04 00 00 01 00 80 00 + 128 б data + пад (384) = 520
#   запись цветов:    SET 06 0A 00 00 01 00 00 02 + 512 б data = 520 (без пада)
#   data[i] читается в строке ответа с поля 9+i (поле 1 = report id 06,
#   поля 2..8 = эхо cmd/addr/len, 9..136 = data[0..127], 137..138 = 5A A5).
#
# ВАЖНО: маркер 5A A5 (data[126..127]) НИКОГДА не трогаем: его затирание
# откатывает клавиатуру к заводскому конфигу (журнал 10.36).
# Запись конфига кикает клавиатуру (пересоздание hidraw) — после неё пауза ~5 с.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="$HOME/kbc-diag-$STAMP"

DEVICE="${DEVICE:-f75}"
USE_SG=0
WRITE_TESTS=0
AUTO=0
PID_USB=""
BOOT_HIDRAW=""

# ---------- утилиты вывода ----------
GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'
ok()   { printf "${GREEN}OK${NC}   %s\n" "$*"; }
warn() { printf "${YELLOW}WARN${NC} %s\n" "$*"; }
fail() { printf "${RED}FAIL${NC} %s\n" "$*"; exit 1; }
hdr()  { printf '\n=== %s ===\n' "$*"; }
die()  { printf "${RED}ОШИБКА:${NC} %s\n" "$*"; exit 1; }

# ask_yn: [Y/n] - вопрос; 0 = да, 1 = нет. В --auto всегда "нет".
ask_yn() {
    local a
    if [ "$AUTO" = "1" ]; then
        return 1
    fi
    printf '\n%s [Y/n]: ' "$*"
    read -r -p "" a </dev/tty
    case "${a,,}" in
        ""|y|yes|да|д) return 0 ;;
        *) return 1 ;;
    esac
}

# ---------- поиск kbc ----------
find_kbc() {
    if command -v kbc >/dev/null 2>&1; then
        KBC_BIN="$(command -v kbc)"
    elif [ -x "$ROOT/target/release/kbc" ]; then
        KBC_BIN="$ROOT/target/release/kbc"
    else
        die "kbc не найден. Соберите: make release (или nix develop . --command cargo build --release)"
    fi
}

run_kbc() { # обёртка вокруг kbc (с sg plugdev при необходимости)
    if [ "$USE_SG" = "1" ]; then
        sg plugdev -c "$KBC_BIN $*"
    else
        "$KBC_BIN" "$@"
    fi
}

# ---------- детект устройства по USB ----------
detect_device() {
    hdr "Шаг 1: поиск AULA F75 по USB"
    local list
    mkdir -p "$OUT_DIR"
    list="$(run_kbc list 2>&1 || true)"
    printf '%s\n' "$list" > "$OUT_DIR/01-kbc-list.txt"

    # F75 и F99 делят USB PID 258A:010C — берём USB-строку и проверяем модель по PSD
    if ! printf '%s\n' "$list" | grep -q '258A:010C.*USB'; then
        warn "  Видно было:"
        printf '%s\n' "$list" | sed 's/^/    /'
        die "USB-подключение F75 (258A:010C) не найдено"
    fi
    PID_USB="258A:010C"

    local out
    out="$(run_kbc info "$PID_USB" 2>&1 || true)"
    printf '%s\n' "$out" > "$OUT_DIR/02-info.txt"
    if printf '%s\n' "$out" | grep -q 'Модель:\s\+AULA F75'; then
        ok "модель F75 подтверждена по PSD"
    elif printf '%s\n' "$out" | grep -q 'Модель:\s\+AULA F99'; then
        die "обнаружена F99 (тот же PID) — для неё используйте scripts/f99-usb-rev.sh"
    else
        die "продолжить нельзя: kbc info не подтверждает модель F75 (см. 02-info.txt). Write-тесты небезопасны на неизвестном устройстве"
    fi

    # boot-интерфейс (клавиатурный HID) — для индикаторов
    run_kbc probe "$PID_USB" > "$OUT_DIR/03-probe.txt" 2>&1 || true
    BOOT_HIDRAW="$(sed -n 's/.*\(\/dev\/hidraw[0-9]*\)  usage_page=0x0001 usage=0x0006.*/\1/p' "$OUT_DIR/03-probe.txt" | head -1)"
    if [ -n "$BOOT_HIDRAW" ]; then
        ok "boot-интерфейс (HID keyboard): $BOOT_HIDRAW"
    else
        warn "boot-интерфейс (usage 0x0006) в probe не найден — индикаторы тестировать нечем"
    fi
}

# ---------- пакетные хелперы ----------
packet_read() { # $1=cmd hex, $2=длина LE (Пример: 0x84 0x0080)
    local cmd="$1" len="$2"
    local lo hi
    lo=$(( len & 0xFF )); hi=$(( (len >> 8) & 0xFF ))
    printf '06 %02X 00 00 01 00 %02X %02X ' "$((cmd))" "$lo" "$hi"
    yes 00 | head -512 | tr '\n' ' '
}

packet_read_keymap() { # $1=слой (0=Default, 1=FN1)
    local layer="$1"
    printf '06 83 00 %02X 01 00 00 02 ' "$(( layer ))"
    yes 00 | head -512 | tr '\n' ' '
}

# полный пакет записи конфига: cmd 0x04, 128 б data (поля $9..NF), пад 384
packet_write_config() { # $1=строка ответа конфига (data[0..127] в полях 9..NF)
    printf '06 04 00 00 01 00 80 00 %s' \
        "$(printf '%s' "$1" | awk '{ s=$9; for (i=10; i<=NF; i++) s=s " " $i; print s }')"
    yes 00 | head -384 | tr '\n' ' '
}

# полный пакет записи color table: cmd 0x0A, 512 б данных = 520 (без пада)
packet_write_colors() { # $1=строка ответа color table (data в полях 9..520)
    printf '06 0A 00 00 01 00 00 02 %s' \
        "$(printf '%s' "$1" | awk '{ s=$9; for (i=10; i<=NF; i++) s=s " " $i; print s }')"
}

feature_get_line() { # последняя строка вывода feature-get (сам hex-ответ)
    sed -n '/^0[0-9A-Fa-f]/p' | tail -1
}

snapshot_config() { # вернуть строку ответа GET конфига (без CR)
    run_kbc raw "$PID_USB" feature-send "$(packet_read 0x84 0x0080)" >/dev/null 2>&1 || true
    run_kbc raw "$PID_USB" feature-get 06 520 | feature_get_line | tr -d '\r' || true
}

snapshot_colors() { # вернуть строку ответа GET color table
    run_kbc raw "$PID_USB" feature-send "$(packet_read 0x8A 0x0200)" >/dev/null 2>&1 || true
    run_kbc raw "$PID_USB" feature-get 06 520 | feature_get_line | tr -d '\r' || true
}

# ---------- сбор read-only данных ----------
collect_readonly() {
    mkdir -p "$OUT_DIR"

    hdr "Шаг 2: чтение конфиг-региона (06 84, 520 б)"
    printf '# feature-send: чтение конфига (полный 520-байтный пакет)\n' > "$OUT_DIR/04-config-before.txt"
    run_kbc raw "$PID_USB" feature-send "$(packet_read 0x84 0x0080)" >> "$OUT_DIR/04-config-before.txt" || true
    printf '# feature-get 06 520\n' >> "$OUT_DIR/04-config-before.txt"
    run_kbc raw "$PID_USB" feature-get 06 520 >> "$OUT_DIR/04-config-before.txt" || true
    ok "конфиг сохранён → $OUT_DIR/04-config-before.txt"

    hdr "Шаг 3: чтение color table (06 8A, 512 б)"
    printf '# feature-send: чтение color table (проверка канала 06 8A)\n' > "$OUT_DIR/05-color-table.txt"
    run_kbc raw "$PID_USB" feature-send "$(packet_read 0x8A 0x0200)" >> "$OUT_DIR/05-color-table.txt" || true
    printf '# feature-get 06 520\n' >> "$OUT_DIR/05-color-table.txt"
    run_kbc raw "$PID_USB" feature-get 06 520 >> "$OUT_DIR/05-color-table.txt" || true
    ok "color table сохранена → $OUT_DIR/05-color-table.txt (если пусто — канал 0x8A не отвечает)"

    hdr "Шаг 4: чтение keymap Default и FN1 (06 83, read-only)"
    for layer in 0 1; do
        local fname key
        [ "$layer" = "0" ] && fname="06-keymap-default.txt" && key="Default" || fname="07-keymap-fn1.txt" && key="FN1"
        printf '# feature-send: чтение keymap %s (write 0x03 не используется)\n' "$key" > "$OUT_DIR/$fname"
        run_kbc raw "$PID_USB" feature-send "$(packet_read_keymap "$layer")" >> "$OUT_DIR/$fname" || true
        printf '# feature-get 06 520\n' >> "$OUT_DIR/$fname"
        run_kbc raw "$PID_USB" feature-get 06 520 >> "$OUT_DIR/$fname" || true
        ok "keymap $key сохранён → $OUT_DIR/$fname"
    done

    hdr "Шаг 5: системная информация"
    lsusb > "$OUT_DIR/08-lsusb.txt" 2>&1 || true
    for h in /sys/class/hidraw/hidraw*/device/uevent; do
        [ -r "$h" ] && { echo "--- $h ---"; cat "$h"; }
    done > "$OUT_DIR/09-uevent.txt" 2>&1 || true
    ok "lsusb/uevent сохранены"
}

# ---------- write-тесты (только --write-tests) ----------

# T1: аппаратная запись конфига 06 04 (round-trip селектора data[10])
t1_config_roundtrip() {
    hdr "T1: запись конфига 06 04 (round-trip селектора data[10])"
    printf '\nT1: запись конфига 06 04 (round-trip селектора data[10])\n\n' > "$OUT_DIR/T1-config-write.txt"

    local old_data sel_old sel_new new_data
    old_data="$(snapshot_config)"
    [ -n "$old_data" ] || { warn "конфиг не прочитался — тест невозможен"; return 0; }

    sel_old="$(printf '%s' "$old_data" | awk '{print $19}')"   # data[10]
    sel_new="03"

    if ! ask_yn "Сменить селектор эффекта data[10] с 0x$sel_old на 0x$sel_new и восстановить? (тест записи)"; then
        echo "T1 пропущен" >> "$OUT_DIR/T1-config-write.txt"
        return 0
    fi

    new_data="$(printf '%s' "$old_data" | awk -v s="$sel_new" '{ $19=s; print }')"

    echo "— записываю селектор 0x$sel_new (пакет 520 б)…" >> "$OUT_DIR/T1-config-write.txt"
    run_kbc raw "$PID_USB" feature-send "$(packet_write_config "$new_data")" >> "$OUT_DIR/T1-config-write.txt" 2>&1 || true
    echo "— кик: жду пересоздания узлов (5 с)…" >> "$OUT_DIR/T1-config-write.txt"
    sleep 5

    local readback sel_rb
    readback="$(snapshot_config)"
    sel_rb="$(printf '%s' "$readback" | awk '{print $19}')"
    echo "— readback: data[10]=0x$sel_rb (ожидалось 0x$sel_new)" >> "$OUT_DIR/T1-config-write.txt"
    if [ "x$sel_rb" = "x$sel_new" ]; then
        ok "ROUND-TRIP РАБОТАЕТ: data[10] = 0x$sel_rb"
    else
        warn "readback = 0x$sel_rb (ожидалось 0x$sel_new) — возможно, запись не применилась"
    fi
    if ask_yn "Визуально эффект изменился после записи?"; then
        echo "— визуальное изменение подтверждено (latch не требуется)" >> "$OUT_DIR/T1-config-write.txt"
        ok "эффект применился на лету"
    else
        warn "визуально эффект не изменился (нужен latch/кик)"
        echo "— визуального изменения нет (нужен latch?)" >> "$OUT_DIR/T1-config-write.txt"
    fi

    echo "— восстанавливаю оригинальный селектор 0x$sel_old…" >> "$OUT_DIR/T1-config-write.txt"
    run_kbc raw "$PID_USB" feature-send "$(packet_write_config "$old_data")" >> "$OUT_DIR/T1-config-write.txt" 2>&1 || true
    sleep 5
    readback="$(snapshot_config)"
    sel_rb="$(printf '%s' "$readback" | awk '{print $19}')"
    if [ "x$sel_rb" = "x$sel_old" ]; then
        ok "восстановлено: data[10] = 0x$sel_old"
    else
        warn "ВОССТАНОВЛЕНИЕ НЕ СОВПАЛО: data[10] = 0x$sel_rb (ожидалось 0x$sel_old)"
    fi
}

# T2: охота за селектором self-define (per-key цвета через 06 0A, полный перебор data[10] ∈ 0x01..0x1F)
# Идея из cloudglides probeSelfDefineSlots (VERIFIED на 258A:010C), журнал 10.59/10.67.
t2_selfdefine_hunt() {
    hdr "T2: поиск self-define — per-key цвета + полный перебор data[10] ∈ 0x01..0x1F"
    printf '\nT2: поиск self-define (запись красного на клавиши 0..2, полный перебор data[10] 0x01..0x1F)\n\n' > "$OUT_DIR/T2-selfdefine.txt"

    local cfg_before ct_before
    cfg_before="$(snapshot_config)"
    ct_before="$(snapshot_colors)"
    if [ -z "$cfg_before" ] || [ -z "$ct_before" ]; then
        warn "конфиг или color table не прочитались — тест невозможен (06 8A может не отвечать на F75)"
        return 0
    fi

    if ! ask_yn "Записать красный на клавиши 0..2 и перебирать data[10] ∈ 0x01..0x1F? (каждый шаг = запись конфига + кик)"; then
        echo "T2 пропущен" >> "$OUT_DIR/T2-selfdefine.txt"
        return 0
    fi

    # маркерные цвета: красный FF 00 00 на клавиши 0,1,2 (записи 0..2 = поля 9..20)
    local pat
    pat="$(printf '%s' "$ct_before" | awk '{
        $9="FF";  $10="00"; $11="00"; $12="00"
        $13="FF"; $14="00"; $15="00"; $16="00"
        $17="FF"; $18="00"; $19="00"; $20="00"
        print }')"

    echo "— записываю красный на клавиши 0..2 (06 0A)…" >> "$OUT_DIR/T2-selfdefine.txt"
    run_kbc raw "$PID_USB" feature-send "$(packet_write_colors "$pat")" >> "$OUT_DIR/T2-selfdefine.txt" 2>&1 || true
    sleep 2
    local ct_rb
    ct_rb="$(snapshot_colors)"
    local rb1
    rb1="$(printf '%s' "$ct_rb" | awk '{print $9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20}')"
    echo "— readback цветов 0..2: $rb1 (ожидалось FF 00 00 00 ×3)" >> "$OUT_DIR/T2-selfdefine.txt"
    if [ "x$rb1" = "xFF 00 00 00 FF 00 00 00 FF 00 00 00" ]; then
        ok "color table запись работает (readback подтверждает красный)"
    else
        warn "readback цветов не совпал: $rb1 — возможно, запись 0x0A игнорируется"
    fi

    # варианты-кандидаты селектора self-define — полный перебор data[10] ∈ 0x01..0x1F.
    # Пропускаем: известные аппаратные эффекты (карта 10.26: 02,03,04,05,06,07,08,0A,0B,0C —
    # это не self-define, а разгрузка вопросов владельцу) и уже проверенные негативы
    # (0x14..0x17, сессия 10.85). Цель — найти НОВЫЙ код, включающий цвет-таблицу.
    local v hx cur cur_sel pat2
    for ((v=1; v<=31; v++)); do
        case "$v" in
            2|3|4|5|6|7|8|10|11|12)
                echo "— пропуск 0x$(printf '%02X' "$v") (известный эффект, карта 10.26)" >> "$OUT_DIR/T2-selfdefine.txt"
                continue ;;
            20|21|22|23)
                echo "— пропуск 0x$(printf '%02X' "$v") (уже негатив, сессия 10.85)" >> "$OUT_DIR/T2-selfdefine.txt"
                continue ;;
        esac
        hx=$(printf '%02X' "$v")
        echo "— кандидат data[10]=0x$hx: запись конфига + кик…" >> "$OUT_DIR/T2-selfdefine.txt"
        cur="$(snapshot_config)"
        [ -z "$cur" ] && { warn "конфиг не читается на шаге 0x$hx — прерываю перебор"; break; }
        pat2="$(printf '%s' "$cur" | awk -v s="$hx" '{ $19=s; print }')"
        run_kbc raw "$PID_USB" feature-send "$(packet_write_config "$pat2")" >> "$OUT_DIR/T2-selfdefine.txt" 2>&1 || true
        sleep 5

        echo "  — с физической клавиатуры: загорелись ли клавиши 0..2 КРАСНЫМ? (фон — эффект 0x$hx)" >> "$OUT_DIR/T2-selfdefine.txt"
        if ask_yn "data[10]=0x$hx: виден ли красный на первых трёх клавишах? (смотрите на клаву)"; then
            ok "SELF-DEFINE НАЙДЕН ВАРИАНТОМ 0x$hx: per-key цвета включаются этим селектором"
            echo "  — ПОЛОЖИТЕЛЬНЫЙ результат: селектор self-define = 0x$hx" >> "$OUT_DIR/T2-selfdefine.txt"
            FOUND_SD="$hx"
            break
        else
            echo "  — негативно (0x$hx)" >> "$OUT_DIR/T2-selfdefine.txt"
        fi
    done
    if [ -z "${FOUND_SD:-}" ]; then
        warn "ни один кандидат 0x01..0x1F не показал per-key — либо колор-режим другой, либо static требует иной селектор"
    fi

    # ВОССТАНОВЛЕНИЕ: исходная color table, затем исходный конфиг (именно в таком порядке,
    # чтобы после последнего кика клава осталась в baseline-эффекте с original-цветами)
    echo "— восстанавливаю исходную color table…" >> "$OUT_DIR/T2-selfdefine.txt"
    run_kbc raw "$PID_USB" feature-send "$(packet_write_colors "$ct_before")" >> "$OUT_DIR/T2-selfdefine.txt" 2>&1 || true
    sleep 2
    echo "— восстанавливаю исходный конфиг (data[10]=0x$(printf '%s' "$cfg_before" | awk '{print $19}'))…" >> "$OUT_DIR/T2-selfdefine.txt"
    run_kbc raw "$PID_USB" feature-send "$(packet_write_config "$cfg_before")" >> "$OUT_DIR/T2-selfdefine.txt" 2>&1 || true
    sleep 5

    local cfg_check ct_check
    cfg_check="$(snapshot_config)"
    ct_check="$(snapshot_colors)"
    if [ "x$cfg_check" = "x$cfg_before" ]; then
        ok "конфиг восстановлен (байт-в-байт)"
    else
        warn "конфиг после restore отличается от снимка — сверьте вручную"
    fi
    if [ "x$ct_check" = "x$ct_before" ]; then
        ok "color table восстановлена (байт-в-байт)"
    else
        warn "color table после restore отличается от снимка — сверьте вручную"
    fi
}

# T3: индикаторы через boot output-report 0x02 (CapsLock)
t3_indicators() {
    hdr "T3: индикаторы через boot output-report (CapsLock)"
    printf '\nT3: индикаторы через boot output-report 0x02 (CapsLock)\n\n' > "$OUT_DIR/T3-indicators.txt"
    if [ -z "$BOOT_HIDRAW" ]; then
        echo "boot-интерфейс не найден — тест пропущен" >> "$OUT_DIR/T3-indicators.txt"
        warn "boot-интерфейс не найден — тест пропущен"
        return 0
    fi
    if ! ask_yn "Включить CapsLock-индикатор через $BOOT_HIDRAW (output-send)? Смотрите на клавиатуру."; then
        echo "T3 пропущен" >> "$OUT_DIR/T3-indicators.txt"
        return 0
    fi
    # HID LED report: [2][bitmap Num/Caps/Scroll][0 x 6]; bit1=CapsLock
    echo "— output-send: 02 02 00 00 00 00 00 00" >> "$OUT_DIR/T3-indicators.txt"
    run_kbc raw "$PID_USB" --path "$BOOT_HIDRAW" output-send "02 02 00 00 00 00 00 00" >> "$OUT_DIR/T3-indicators.txt" 2>&1 || true
    if ask_yn "Индикатор CapsLock загорелся?"; then
        ok "индикаторы управляются через boot output-report"
        echo "— CapsLock загорелся (подтверждено визуально)" >> "$OUT_DIR/T3-indicators.txt"
    else
        warn "индикатор не загорелся — либо канал другой, либо физически нет индикатора"
        echo "— индикатор не загорелся" >> "$OUT_DIR/T3-indicators.txt"
    fi
    run_kbc raw "$PID_USB" --path "$BOOT_HIDRAW" output-send "02 00 00 00 00 00 00 00" >> "$OUT_DIR/T3-indicators.txt" 2>&1 || true
    ok "индикатор погашен"
}

# T4: direct-канал — простой кадр 06 08 vs enable-seq (0x39/0x3c/0x39) + disable
# Байты из cloudglides direct-mode.ts (report id 0x39/0x3c, WEBHID отбрасывает первый байт).
packet_small() { # $1=report_id, остальные байты data -> "ID d0 d1 ..." + пад до 520
    local rid="$1"; shift
    local s="$rid" b
    for b in "$@"; do s="$s $b"; done
    local used=$((1 + $#))
    printf '%s ' "$s"
    yes 00 | head -$((520 - used)) | tr '\n' ' '
}

packet_direct_frame() { # $1=RGB hex-triplet, $2=count -> 06 08 ... + RGB*count + пад
    local rgb="$1" n="$2" i s
    s="06 08 00 00 01 00 7A 01"
    for ((i=0; i<n; i++)); do s="$s $rgb"; done
    local used=$((8 + n*3))
    printf '%s ' "$s"
    yes 00 | head -$((520 - used)) | tr '\n' ' '
}

# burst_frames: $1=RGB hex-triplet, $2=count LED, $3=кадров, $4=пауза (с)
# Прямой кадр живёт лишь ~1 с (журнал 10.7) — шлём серию, чтобы свет держался
# всё время визуального вопроса.
burst_frames() {
    local rgb="$1" ledcount="$2" frames="$3" pause="$4" i
    for ((i=0; i<frames; i++)); do
        run_kbc raw "$PID_USB" feature-send "$(packet_direct_frame "$rgb" "$ledcount")" >> "$OUT_DIR/T4-direct-enable.txt" 2>&1 || true
        sleep "$pause"
    done
}

t4_direct_enable() {
    hdr "T4: direct-канал — простой кадр vs enable-seq (0x39/0x3c/0x39) + disable"
    printf '\nT4: direct-канал (06 08) — plain vs enable-seq\n\n' > "$OUT_DIR/T4-direct-enable.txt"
    if ! ask_yn "Тест direct-канала? (серия кадров 06 08, затем enable-seq 0x39/0x3c/0x39)"; then
        echo "T4 пропущен" >> "$OUT_DIR/T4-direct-enable.txt"
        return 0
    fi

    echo "— A) ПРОСТЫЕ кадры 06 08 (красный, 122 LED, burst 12×300 мс)…" >> "$OUT_DIR/T4-direct-enable.txt"
    burst_frames 'FF 00 00' 122 12 0.3
    if ask_yn "A) во время ПРОСТЫХ кадров светился красным?"; then
        ok "простой direct-кадр работает (как и в наших прошлых тестах)"
        echo "— A: красный виден без enable-seq" >> "$OUT_DIR/T4-direct-enable.txt"
    else
        warn "простой кадр не дал видимого света"
        echo "— A: красный не виден" >> "$OUT_DIR/T4-direct-enable.txt"
    fi

    echo "— B) enable-seq (0x39 → 0x3c → 0x39), затем зелёный burst…" >> "$OUT_DIR/T4-direct-enable.txt"
    run_kbc raw "$PID_USB" feature-send "$(packet_small 39 20 06 00 01 00)" >> "$OUT_DIR/T4-direct-enable.txt" 2>&1 || true
    sleep 1
    run_kbc raw "$PID_USB" feature-send "$(packet_small 3C 20 01 00)" >> "$OUT_DIR/T4-direct-enable.txt" 2>&1 || true
    sleep 1
    run_kbc raw "$PID_USB" feature-send "$(packet_small 39 20 06 01 01 00)" >> "$OUT_DIR/T4-direct-enable.txt" 2>&1 || true
    sleep 1
    burst_frames '00 FF 00' 122 12 0.3
    if ask_yn "B) с enable-seq (после seq) светился зелёным?"; then
        ok "enable-seq не мешает (зелёный виден)"
        echo "— B: зелёный виден с enable-seq" >> "$OUT_DIR/T4-direct-enable.txt"
    else
        warn "с enable-seq зелёный не виден (возможно, 0x39/0x3c выключают подсветку на этой ревизии)"
        echo "— B: зелёный не виден" >> "$OUT_DIR/T4-direct-enable.txt"
    fi

    echo "— C) disable (0x3c 20 00 00) + пауза…" >> "$OUT_DIR/T4-direct-enable.txt"
    run_kbc raw "$PID_USB" feature-send "$(packet_small 3C 20 00 00)" >> "$OUT_DIR/T4-direct-enable.txt" 2>&1 || true
    sleep 2
    if ask_yn "C) после disable подсветка погасла?"; then
        ok "disable работает (свет погашен)"
        echo "— C: свет погашен после disable" >> "$OUT_DIR/T4-direct-enable.txt"
    else
        warn "свет не погас после disable"
        echo "— C: свет не погас" >> "$OUT_DIR/T4-direct-enable.txt"
    fi
    # В любом случае погасим штатно: чёрный кадр (память не трогаем)
    run_kbc raw "$PID_USB" feature-send "$(packet_direct_frame '00 00 00' 122)" >> "$OUT_DIR/T4-direct-enable.txt" 2>&1 || true
    ok "завершено (последний чёрный кадр)"
}

# T5: поиск начала эффект-таблицы (offset 58 vs 64) — read-only дифф снимков
t5_effect_table() {
    hdr "T5: эффект-таблица — дифф снимков при Fn+\\ и Fn+Вниз"
    printf '\nT5: дифф снимков конфига (поиск offset эффект-таблицы: 58 vs 64)\n\n' > "$OUT_DIR/T5-effect-table.txt"

    local base a b
    base="$(snapshot_config)"
    [ -n "$base" ] || { warn "конфиг не прочитался — тест невозможен"; return 0; }
    printf '0: BASELINE  %s\n' "$base" > "$OUT_DIR/T5-effect-table.txt"

    if ! ask_yn "T5: нажмите Fn+\\ (сменить эффект), затем ответьте — снимки сравним (read-only, ничего не пишем)"; then
        echo "T5 пропущен" >> "$OUT_DIR/T5-effect-table.txt"
        return 0
    fi
    sleep 2
    a="$(snapshot_config)"
    printf '1: AFTER-FN-BACKSLASH  %s\n' "$a" >> "$OUT_DIR/T5-effect-table.txt"

    if ! ask_yn "Затем нажмите Fn+СтрелкаВниз (яркость вниз), затем ответьте — ещё один снимок"; then
        echo "T5 (часть 2) пропущен" >> "$OUT_DIR/T5-effect-table.txt"
    else
        sleep 2
        b="$(snapshot_config)"
        printf '2: AFTER-FN-DOWN  %s\n' "$b" >> "$OUT_DIR/T5-effect-table.txt"
    fi

    diff_snaps() { # $1=файл-лог, $2=имя A, $3=строка A, $4=имя B, $5=строка B
        local A="$2" B="$4"
        local i old new
        printf '\nДифф %s → %s (по полям; поле n = data[n-9]):\n' "$A" "$B" >> "$OUT_DIR/T5-effect-table.txt"
        old="$(printf '%s' "$3")"; new="$(printf '%s' "$5")"
        diff <(printf '%s' "$old" | tr ' ' '\n') <(printf '%s' "$new" | tr ' ' '\n') \
            >> "$OUT_DIR/T5-effect-table.txt" 2>&1 || {
            printf '  (поля идентичны или одной строки нет)\n' >> "$OUT_DIR/T5-effect-table.txt"
        }
    }

    diff_snaps "$OUT_DIR/T5-effect-table.txt" "BASELINE" "$base" "AFTER-FN-BACKSLASH" "$a"
    [ -n "${b:-}" ] && diff_snaps "$OUT_DIR/T5-effect-table.txt" "AFTER-FN-BACKSLASH" "$a" "AFTER-FN-DOWN" "$b"

    local changed
    changed="$(grep -c '^[<>]' "$OUT_DIR/T5-effect-table.txt" || true)"
    ok "диффы сохранены (изменилось полей: $changed) → $OUT_DIR/T5-effect-table.txt"
    echo "Подсказка: если эффект-таблица начинается с offset 58, дифф Fn+Вниз покажет поля 67+; если 64 — поля 73+."
    echo "(поле n строки = data[n-9]; data[58] = поле 67, data[64] = поле 73)"
}

# ---------- summary ----------
make_summary() {
    hdr "Шаг 6: сводка"
    local f
    {
        echo "Диагностика AULA F75 (USB $PID_USB)"
        echo "Каталог: $OUT_DIR"
        echo "Время:   $(date '+%Y-%m-%d %H:%M:%S')"
        echo
        echo "Собранные файлы:"
        for f in "$OUT_DIR"/0*.txt "$OUT_DIR"/T*.txt; do
            [ -f "$f" ] && echo "  - $(basename "$f")"
        done
        echo
        echo "Ключевые факты:"
        echo "  boot-интерфейс: ${BOOT_HIDRAW:-(не найден)}"
        echo "  self-define-селектор (T2): ${FOUND_SD:-не найден/не тестировался}"
        echo "  write-тесты:   $([ "$WRITE_TESTS" = "1" ] && [ "$AUTO" != "1" ] && echo 'запускались T1–T5' || echo 'НЕ запускались (--write-tests + без --auto)')"
    } > "$OUT_DIR/10-svodka.txt"
    cat "$OUT_DIR/10-svodka.txt"
}

# ---------- main ----------
while [ $# -gt 0 ]; do
    case "$1" in
        --device) DEVICE="${2:-f75}"; shift 2 ;;
        --sg)     USE_SG=1; shift ;;
        --write-tests) WRITE_TESTS=1; shift ;;
        --auto)   AUTO=1; shift ;;
        -h|--help)
            sed -n '1,30p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *) die "неизвестный параметр: $1 (см. шапку файла или --help)" ;;
    esac
done

[ "$DEVICE" = "f75" ] || die "поддерживается только --device f75"

if [ "$(id -u)" = "0" ]; then
    die "запускайте скрипт от обычного пользователя (не root)"
fi

[ "$WRITE_TESTS" = "1" ] && [ "$AUTO" = "1" ] \
    && die "--write-tests и --auto несовместимы (write-тесты интерактивные)"

find_kbc

printf 'Keyboard-Center — реверс AULA F75 (USB 258A:010C)\n'
printf 'Каталог результатов: %s\n' "$OUT_DIR"
mkdir -p "$OUT_DIR"

detect_device
collect_readonly

if [ "$WRITE_TESTS" = "1" ]; then
    warn "Включены write-тесты — они ПИШУТ в клавиатуру, но каждый шаг восстанавливает исходные данные"
    warn "Маркер 5A A5 (data[126..127]) не трогается; конфиг/цвета восстанавливаются"
    if ! ask_yn "Продолжить с write-тестами (T1–T5)?"; then
        warn "write-тесты отменены"
    else
        t1_config_roundtrip
        t2_selfdefine_hunt
        t3_indicators
        t4_direct_enable
        t5_effect_table
    fi
else
    warn "write-тесты не запущены (нужен флаг --write-tests)"
fi

make_summary

tar -czf "$HOME/kbc-diag-$STAMP.tar.gz" -C "$(dirname "$OUT_DIR")" "$(basename "$OUT_DIR")" 2>/dev/null \
    && ok "архив: $HOME/kbc-diag-$STAMP.tar.gz" \
    || warn "не удалось создать архив (tar недоступен?)"

printf '\nГотово. Каталог: %s\n' "$OUT_DIR"
printf 'Пришлите содержимое каталога (или .tar.gz) владельцу проекта Keyboard-Center.\n'