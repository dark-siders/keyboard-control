#!/usr/bin/env bash
# =============================================================================
# f99-usb-rev.sh — реверс-инженерная диагностика AULA F99 (USB 258A:010C)
#
# Цель: собрать с реальной клавиатуры данные, которых всё ещё не хватает
# драйверу aula_f99.rs:
#   1. работает ли аппаратная запись конфига (06 04) и "latch";
#   2. управляются ли индикаторы Caps/Num/Scroll через boot output-reports;
#   3. какой селектор data[10] включает per-key self-define цвета
#      (записанные в color table через 06 0A) — чтобы оживить set_rgb_color.
#   4. работает ли запись keymap (remaп) — и через какой CMD (0x03 не работает);
#   5. работает ли per-key color table write (06 0A) + self-define селектор;
#   6. где в конфиге живут биты индикаторов Caps/Num/Scroll (diff конфига).
#
# ВАЖНО: по умолчанию скрипт НИЧЕГО НЕ ПИШЕТ в клавиатуру — только читает.
# Write-тесты (T1–T6) запускаются ИСКЛЮЧИТЕЛЬНО с флагом --write-tests
# и после каждого теста ВСЕГДА восстанавливают исходные данные.
# Маркер 5A A5 на data[126..127] никогда не трогается:
# затирание маркера = откат клавиатуры к заводским настройкам (журнал 10.36).
# =============================================================================
# Параметры:
#   --device f99    — только AULA F99 (по умолчанию; autodetect по PID+PSD)
#   --sg            — оборачивать команды через `sg plugdev -c "…"`
#                     (нужно, если текущая сессия не видит группу plugdev)
#   --write-tests   — разрешить секции T1–T6 (запись в клавиатуру)
#   --auto          — без интерактивных вопросов (write-тесты пропускаются)
#   -h|--help       — показать справку
# =============================================================================

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="$HOME/kbc-diag-$STAMP"

DEVICE="${DEVICE:-f99}"
USE_SG=0
WRITE_TESTS=0
AUTO=0
PID_USB=""
BOOT_HIDRAW=""

# ---------- вывод ----------
GREEN='\033[0;32m'; YELLOW='\033[0;33m'; RED='\033[0;31m'; NC='\033[0m'
ok()   { printf "${GREEN}  ✓ %s${NC}\n" "$*"; }
warn() { printf "${YELLOW}  ! %s${NC}\n" "$*"; }
fail() { printf "${RED}  ✗ %s${NC}\n" "$*"; }
hdr()  { printf "\n${GREEN}== %s ==${NC}\n" "$*"; }

die() { printf "${RED}Ошибка: %s${NC}\n" "$*" >&2; exit 1; }

ask_yn() { # вопрос -> "да"/"нет" (если AUTO — "нет")
    local prompt="$1" ans
    if [ "$AUTO" = "1" ]; then echo "нет"; return 1; fi
    printf "  ? %s [y/N] " "$prompt"
    read -r ans
    case "$ans" in
        [yY]|[yY]*) echo "да"; return 0 ;;
        *)          echo "нет"; return 1 ;;
    esac
}

# ---------- поиск kbc ----------
find_kbc() {
    local kbc_bin
    if command -v kbc >/dev/null 2>&1; then
        kbc_bin="$(command -v kbc)"
    elif [ -x "$ROOT/target/release/kbc" ]; then
        kbc_bin="$ROOT/target/release/kbc"
    else
        die "kbc не найден. Соберите: make release (или nix develop . --command cargo build --release)"
    fi
    echo "$kbc_bin"
}

KBC_BIN="$(find_kbc)"

run_kbc() { # обёртка: вызов kbc с/без sg plugdev
    if [ "$USE_SG" = "1" ]; then
        sg plugdev -c "${KBC_BIN} $*"
    else
        "$KBC_BIN" "$@"
    fi
}

# ---------- детект устройства ----------
detect_device() {
    local list out pid
    hdr "Шаг 1: поиск AULA F99 по USB"
    list="$(run_kbc list 2>&1)" || die "не удалось выполнить kbc list"
    printf '%s\n' "$list" > "$OUT_DIR/01-kbc-list.txt"

    # F75 и F99 делят USB PID 258A:010C — берём USB-строку
    pid="$(printf '%s\n' "$list" | grep -i '258A:010C.*USB' | head -1 || true)"
    if [ -z "$pid" ]; then
        warn "Устройство 258A:010C по USB не найдено в kbc list"
        warn "  (подключите клавиатуру кабелем; в 2.4G-режиме USB-узла нет)"
        warn "  Видно было:"
        printf '%s\n' "$list" | sed 's/^/    /'
        die "USB-подключение F99 не найдено"
    fi
    PID_USB="258A:010C"

    out="$(run_kbc info "$PID_USB" 2>&1 || true)"
    printf '%s\n' "$out" > "$OUT_DIR/02-info.txt"
    if printf '%s\n' "$out" | grep -q 'Модель:\s\+AULA F99'; then
        ok "модель F99 подтверждена по PSD (Psd=0xA4)"
    else
        warn "kbc info не вернул 'Модель: AULA F99' — проверьте вывод 02-info.txt"
    fi

    # пробуем определить boot-интерфейс (клавиатурный HID) для индикаторов
    run_kbc probe "$PID_USB" > "$OUT_DIR/03-probe.txt" 2>&1 || true
    BOOT_HIDRAW="$(sed -n 's/.*\(\/dev\/hidraw[0-9]*\)  usage_page=0x0001 usage=0x0006.*/\1/p' "$OUT_DIR/03-probe.txt" | head -1)"
    if [ -n "$BOOT_HIDRAW" ]; then
        ok "boot-интерфейс (HID keyboard): $BOOT_HIDRAW"
    else
        warn "boot-интерфейс (usage 0x0006) в probe не найден — индикаторы тестировать нечем"
    fi
}

# ---------- пакетные хелперы (только чтение) ----------
# Форматы из collect-data.sh: чтение = SET(пак 520) -> GET 06 520.
# data[i] читается с позиции 8+i в строке ответа (0: report id 06).

packet_read() { # $1=cmd hex, $2=длина LE (Пример: 0x84 0x0080)
    local cmd="$1" len="$2"
    local lo hi
    lo=$(( len & 0xFF )); hi=$(( (len >> 8) & 0xFF ))
    printf '06 %02X 00 00 01 00 %02X %02X ' "$((cmd))" "$lo" "$hi"
    yes 00 | head -512 | tr '\n' ' '
}

packet_read_keymap() { # $1=слой (0=Default, 1=FN1)
    local layer="$1"
    printf '06 83 00 %s 01 00 00 02 ' "$layer"
    yes 00 | head -512 | tr '\n' ' '
}

feature_get_line() { # последняя строка вывода feature-get (сам hex-ответ)
    sed -n '/^0[0-9A-Fa-f]/p' | tail -1
}

# ---------- сбор read-only данных ----------
collect_readonly() {
    mkdir -p "$OUT_DIR"

    hdr "Шаг 2: чтение конфиг-региона (06 84, 520 б)"
    printf '# feature-send: чтение конфига (полный 520-байтный пакет)\n' > "$OUT_DIR/04-config-dump.txt"
    run_kbc raw "$PID_USB" feature-send "$(packet_read 0x84 0x0080)" >> "$OUT_DIR/04-config-dump.txt" || true
    printf '# feature-get 06 520\n' >> "$OUT_DIR/04-config-dump.txt"
    run_kbc raw "$PID_USB" feature-get 06 520 >> "$OUT_DIR/04-config-dump.txt" || true
    ok "конфиг сохранён → $OUT_DIR/04-config-dump.txt"

    hdr "Шаг 3: чтение color table (06 8A, 512 б)"
    printf '# feature-send: чтение color table\n' > "$OUT_DIR/05-color-table.txt"
    run_kbc raw "$PID_USB" feature-send "$(packet_read 0x8A 0x0200)" >> "$OUT_DIR/05-color-table.txt" || true
    printf '# feature-get 06 520\n' >> "$OUT_DIR/05-color-table.txt"
    run_kbc raw "$PID_USB" feature-get 06 520 >> "$OUT_DIR/05-color-table.txt" || true
    ok "color table сохранена → $OUT_DIR/05-color-table.txt"

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

    # текущий снимок конфига (128 байт data)
    local old_data new_data readback
    old_data="$(run_kbc raw "$PID_USB" feature-send "$(packet_read 0x84 0x0080)" >/dev/null 2>&1; run_kbc raw "$PID_USB" feature-get 06 520 \
                | feature_get_line | tr -d '\r' )"

    # data[10] = селектор эффекта; меняем на 0x03 ("смена цветов", карта 10.26)
    local sel_old sel_new
    sel_old="$(printf '%s' "$old_data" | awk '{print $19}')"   # позиция 8+10 = 18 (0-based -> $19)
    sel_new="03"

    if ! ask_yn "Сменить селектор эффекта data[10] с 0x$sel_old на 0x$sel_new и восстановить? (тест записи)"; then
        echo "T1 пропущен" >> "$OUT_DIR/T1-config-write.txt"
        return 0
    fi

    new_data="$(printf '%s' "$old_data" | awk -v s="$sel_new" '{ $19=s; print }')"

    # write-пакет: 06 04 00 00 01 00 80 00 + 128 байт data + пад до 520.
    # ВАЖНО: из строки ответа (06 + эхо-заголовок 7 + data 128) берём ТОЛЬКО
    # data = поля 9..NF (128 б), а не всю строку целиком — иначе пакет
    # разъедется (8 + 136 + пад != 520). Пад до 520: 8 + 128 = 136 + 384.
    local write_pkt
    write_pkt="$(printf '06 04 00 00 01 00 80 00 %s' \
        "$(printf '%s' "$new_data" | awk '{ s=$9; for (i=10; i<=NF; i++) s=s " " $i; print s }')"; \
        yes 00 | head -384 | tr '\n' ' ')"

    echo "— записываю селектор 0x$sel_new (пакет 520 б)…" >> "$OUT_DIR/T1-config-write.txt"
    run_kbc raw "$PID_USB" feature-send "$write_pkt" >> "$OUT_DIR/T1-config-write.txt" 2>&1 || true
    echo "— кик: жду пересоздания узлов (5 с)…" >> "$OUT_DIR/T1-config-write.txt"
    sleep 5

    # readback после записи
    readback="$(run_kbc raw "$PID_USB" feature-send "$(packet_read 0x84 0x0080)" >/dev/null 2>&1; run_kbc raw "$PID_USB" feature-get 06 520 \
                | feature_get_line | tr -d '\r' )"
    local sel_rb
    sel_rb="$(printf '%s' "$readback" | awk '{print $19}')"
    echo "— readback: data[10]=0x$sel_rb (ожидалось 0x$sel_new)" >> "$OUT_DIR/T1-config-write.txt"
    if [ "x$sel_rb" = "x$sel_new" ]; then
        ok "ROUND-TRIP РАБОТАЕТ: data[10] = 0x$sel_rb"
    else
        warn "readback = 0x$sel_rb (ожидалось 0x$sel_new) — возможно, запись не применилась"
    fi
    if ! ask_yn "Визуально эффект изменился после записи?"; then
        warn "визуально эффект не изменился (нужен latch?)"
        echo "— визуального изменения нет" >> "$OUT_DIR/T1-config-write.txt"
    else
        echo "— визуальное изменение подтверждено" >> "$OUT_DIR/T1-config-write.txt"
    fi

    # ВОССТАНОВЛЕНИЕ: всегда записываем оригинал (data = поля 9..NF, пад до 520)
    echo "— восстанавливаю оригинальный селектор 0x$sel_old…" >> "$OUT_DIR/T1-config-write.txt"
    local restore_pkt
    restore_pkt="$(printf '06 04 00 00 01 00 80 00 %s' \
        "$(printf '%s' "$old_data" | awk '{ s=$9; for (i=10; i<=NF; i++) s=s " " $i; print s }')"; \
        yes 00 | head -384 | tr '\n' ' ')"
    run_kbc raw "$PID_USB" feature-send "$restore_pkt" >> "$OUT_DIR/T1-config-write.txt" 2>&1 || true
    sleep 5
    readback="$(run_kbc raw "$PID_USB" feature-send "$(packet_read 0x84 0x0080)" >/dev/null 2>&1; run_kbc raw "$PID_USB" feature-get 06 520 \
                | feature_get_line | tr -d '\r' )"
    sel_rb="$(printf '%s' "$readback" | awk '{print $19}')"
    if [ "x$sel_rb" = "x$sel_old" ]; then
        ok "восстановлено: data[10] = 0x$sel_old"
    else
        warn "ВОССТАНОВЛЕНИЕ НЕ СОВПАЛО: data[10] = 0x$sel_rb (ожидалось 0x$sel_old)"
    fi
}

# T2: индикаторы через boot output-report 0x02 (CapsLock)
t2_indicators() {
    hdr "T2: индикаторы через boot output-report (CapsLock)"
    printf '\nT2: индикаторы через boot output-report 0x02 (CapsLock)\n\n' > "$OUT_DIR/T2-indicators.txt"
    if [ -z "$BOOT_HIDRAW" ]; then
        echo "boot-интерфейс не найден — тест пропущен" >> "$OUT_DIR/T2-indicators.txt"
        warn "boot-интерфейс не найден — тест пропущен"
        return 0
    fi
    if ! ask_yn "Включить CapsLock-индикатор через $BOOT_HIDRAW (output-send)? Смотрите на клавиатуру."; then
        echo "T2 пропущен" >> "$OUT_DIR/T2-indicators.txt"
        return 0
    fi
    # HID LED report: [2][bitmap Num/Caps/Scroll][0 x 6]; bit1=CapsLock
    echo "— output-send: 02 02 00 00 00 00 00 00" >> "$OUT_DIR/T2-indicators.txt"
    run_kbc raw "$PID_USB" --path "$BOOT_HIDRAW" output-send "02 02 00 00 00 00 00 00" >> "$OUT_DIR/T2-indicators.txt" 2>&1 || true
    if ask_yn "Индикатор CapsLock загорелся?"; then
        ok "индикаторы управляются через boot output-report"
        echo "— CapsLock загорелся (подтверждено визуально)" >> "$OUT_DIR/T2-indicators.txt"
    else
        warn "индикатор не загорелся — либо канал другой, либо физически нет индикатора"
        echo "— индикатор не загорелся" >> "$OUT_DIR/T2-indicators.txt"
    fi
    # погасить
    run_kbc raw "$PID_USB" --path "$BOOT_HIDRAW" output-send "02 00 00 00 00 00 00 00" >> "$OUT_DIR/T2-indicators.txt" 2>&1 || true
    ok "индикатор погашен"
}

# T3: поиск селектора self-define (per-key цвета через 06 0A)
t3_selfdefine_hunt() {
    hdr "T3: поиск self-define — запись per-key цвета и охота за селектором"
    printf '\nT3: поиск self-define (06 0A запись цвета → поиск селектора data[10])\n\n' > "$OUT_DIR/T3-selfdefine.txt"
    if ! ask_yn "Записать красные LED на первые 3 клавиши (06 0A) для теста self-define?"; then
        echo "T3 пропущен" >> "$OUT_DIR/T3-selfdefine.txt"
        return 0
    fi

    # снимок текущей color table (520 б: report id + header 7 б + 512 б данных + 0)
    local ct_before ct_after
    ct_before="$(run_kbc raw "$PID_USB" feature-send "$(packet_read 0x8A 0x0200)" >/dev/null 2>&1; run_kbc raw "$PID_USB" feature-get 06 520 \
                 | feature_get_line | tr -d '\r' )"
    [ -n "$ct_before" ] || { warn "color table не прочиталась — тест невозможен"; return 0; }

    # Паттерн: данные color table = поля 9..520 строки ответа (8..519 в 0-based:
    # 0=report id, 1..7=header). Меняем только первые 3 записи (поля 9..20):
    #   FF 00 00 00 | FF 00 00 00 | FF 00 00 00  (красный на клавишах 0..2)
    # Остальные поля (включая трейлер/модель 0xA4/маркер 5A A5) НЕ трогаем.
    # Поля 9..20 = первые 3 записи (record0=[9 10 11 12], record1=[13 14 15 16],
    # record2=[17 18 19 20]). Красный = R G B X (FF 00 00 00).
    local pat ct_write
    pat="$(printf '%s' "$ct_before" | awk '{
        $9="FF";  $10="00"; $11="00"; $12="00"
        $13="FF"; $14="00"; $15="00"; $16="00"
        $17="FF"; $18="00"; $19="00"; $20="00"
        print }')"

    # write-пакет 520 б: cmd 0x0A (write, без бита чтения; те же addr/len, что у 0x8A).
    # Данные = поля 9..520 паттерна (8+512=520; report id и header ответа НЕ включаем).
    pat_data="$(printf '%s' "$pat" | awk '{ s=$9; for (i=10; i<=NF; i++) s=s " " $i; print s }')"
    ct_write="$(printf '06 0A 00 00 01 00 00 02 %s' "$pat_data")"

    echo "— записываю красный на клавиши 0..2 (06 0A, 512 б данных)…" >> "$OUT_DIR/T3-selfdefine.txt"
    run_kbc raw "$PID_USB" feature-send "$ct_write" >> "$OUT_DIR/T3-selfdefine.txt" 2>&1 || true
    sleep 2

    # readback-verify color table
    ct_after="$(run_kbc raw "$PID_USB" feature-send "$(packet_read 0x8A 0x0200)" >/dev/null 2>&1; run_kbc raw "$PID_USB" feature-get 06 520 \
                | feature_get_line | tr -d '\r' )"
    local rb1
    rb1="$(printf '%s' "$ct_after" | awk '{print $9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20}')"
    echo "— readback цветов 0..2: $rb1 (ожидалось FF 00 00 00 FF 00 00 00 FF 00 00 00)" >> "$OUT_DIR/T3-selfdefine.txt"
    if [ "x$rb1" = "xFF 00 00 00 FF 00 00 00 FF 00 00 00" ]; then
        ok "color table запись работает (readback подтверждает красный)"
    else
        warn "readback цветов не совпал: $rb1"
    fi

    echo "— теперь ищите эффект Self-define: жмите Fn+\\ до НЕПОДВИЖНОГО одноцветного эффекта" >> "$OUT_DIR/T3-selfdefine.txt"
    if ask_yn "Посохраняете ли эффект? Переключайте Fn+\\ пока не увидите static/однотонную подсветку, затем Enter"; then
        echo "  — сделаю снимок конфига — всегда снимается селектор data[10]…" >> "$OUT_DIR/T3-selfdefine.txt"

        # снимок конфига после найденного эффекта: фиксируем актуальный data[10]
        local snap snap_sel
        snap="$(run_kbc raw "$PID_USB" feature-send "$(packet_read 0x84 0x0080)" >/dev/null 2>&1; run_kbc raw "$PID_USB" feature-get 06 520 \
                | feature_get_line | tr -d '\r' )"
        snap_sel="$(printf '%s' "$snap" | awk '{print $19}')"
        echo "  — актуальный селектор data[10] = 0x$snap_sel" >> "$OUT_DIR/T3-selfdefine.txt"
        printf '%s\n' "$snap" > "$OUT_DIR/T3-config-after.txt"
        ok "конфиг при найденном эффекте сохранён (data[10]=0x$snap_sel)"

        if ask_yn "Клавиши 0..2 светятся КРАСНЫМ (per-key цвета видны)?"; then
            ok "SELF-DEFINE НАЙДЕН: data[10]=0x$snap_sel включает per-key цвета!"
            echo "  — ПОЛОЖИТЕЛЬНЫЙ результат: селектор self-define = 0x$snap_sel" >> "$OUT_DIR/T3-selfdefine.txt"
        else
            warn "красного нет — или селектор другой, или требуется запись поверх цвета (путь не стал)"
            echo "  — красный не подтверждён (data[10]=0x$snap_sel)" >> "$OUT_DIR/T3-selfdefine.txt"
        fi
    else
        echo "  — снимок после эффекта не делался" >> "$OUT_DIR/T3-selfdefine.txt"
    fi

    # ВОССТАНОВЛЕНИЕ: data-поля 9..520 исходного снимка (без report id/header),
    # header 06 0A + ПРОБЕЛ перед данными (иначе 02 склеится с первым байтом).
    echo "— восстанавливаю исходную color table…" >> "$OUT_DIR/T3-selfdefine.txt"
    local ct_restore
    ct_restore="$(printf '06 0A 00 00 01 00 00 02 %s' "$(printf '%s' "$ct_before" | \
        awk '{ s=$9; for (i=10; i<=NF; i++) s=s " " $i; print s }')")"
    run_kbc raw "$PID_USB" feature-send "$ct_restore" >> "$OUT_DIR/T3-selfdefine.txt" 2>&1 || true
    sleep 2

    local ct_check
    ct_check="$(run_kbc raw "$PID_USB" feature-send "$(packet_read 0x8A 0x0200)" >/dev/null 2>&1; run_kbc raw "$PID_USB" feature-get 06 520 \
                | feature_get_line | tr -d '\r' )"
    if [ "x$ct_check" = "x$ct_before" ]; then
        ok "color table восстановлена (байт-в-байт)"
    else
        warn "color table после restore отличается от снимка — сверьте вручную"
    fi
}
 
# T4: Keymap Write — поиск рабочего канала записи ремапа (0x03 не работает на F99)
t4_keymap_write() {
    hdr "T4: Keymap Write — поиск рабочего канала записи ремапа"
    printf '\nT4: Keymap Write — поиск рабочего канала записи ремапа\n\n' > "$OUT_DIR/T4-keymap-write.txt"
 
    # 1. Снимок текущего keymap Default (layer 0)
    local old_km new_km readback
    old_km="$(run_kbc raw "$PID_USB" feature-send "$(packet_read_keymap 0)" >/dev/null 2>&1; \
             run_kbc raw "$PID_USB" feature-get 06 520 | feature_get_line | tr -d '\r' )"
    [ -n "$old_km" ] || { warn "keymap не прочитался — тест невозможен"; return 0; }
    printf '%s\n' "$old_km" > "$OUT_DIR/T4-keymap-before.txt"
    ok "keymap Default сохранён → $OUT_DIR/T4-keymap-before.txt"
 
    # 2. Модифицируем один слот для теста (слот 73 = "=" -> remap на "A" usage 0x04)
    # Формат слота: 4 байта [page, 0, 0, usage]. Press = page 0x00, usage = 0x04 (A)
    # Слот 73 = байты 292..295 в 512-байтной таблице (73 * 4 = 292)
    new_km="$(printf '%s' "$old_km" | awk '{ for(i=1;i<=NF;i++) if(i>=293 && i<=296) $i="00 00 00 04"; print }')"
 
    if ! ask_yn "Записать изменённый keymap (слот 73 '=' -> 'A') и восстановить? (тест записи ремапа)"; then
        echo "T4 пропущен" >> "$OUT_DIR/T4-keymap-write.txt"
        return 0
    fi
 
    # Пробуем записать через 0x03 (известный нерабочий на F99, но пробуем для подтверждения)
    # Формат: 06 03 <layer> 00 01 00 00 FF + 512 байт слотов + 5A A5 на конце
    local write_pkt
    write_pkt="$(printf '06 03 00 00 01 00 00 FF %s 5A A5' \
        "$(printf '%s' "$new_km" | awk '{ s=$1; for(i=2;i<=NF;i++) s=s " " $i; print s }')"; \
        yes 00 | head -0 | tr '\n' ' ')"
 
    echo "— записываю keymap через 0x03 (layer 0)..." >> "$OUT_DIR/T4-keymap-write.txt"
    run_kbc raw "$PID_USB" feature-send "$write_pkt" >> "$OUT_DIR/T4-keymap-write.txt" 2>&1 || true
    sleep 2
 
    # Readback
    readback="$(run_kbc raw "$PID_USB" feature-send "$(packet_read_keymap 0)" >/dev/null 2>&1; \
               run_kbc raw "$PID_USB" feature-get 06 520 | feature_get_line | tr -d '\r' )"
    local slot73_before slot73_after
    slot73_before="$(printf '%s' "$old_km" | awk '{print $293,$294,$295,$296}')"
    slot73_after="$(printf '%s' "$readback" | awk '{print $293,$294,$295,$296}')"
    echo "— readback слот 73: $slot73_after (было $slot73_before, ожидалось 00 00 00 04)" >> "$OUT_DIR/T4-keymap-write.txt"
    if [ "x$slot73_after" = "x00 00 00 04" ]; then
        ok "РЕМАП ЗАПИСАЛСЯ через 0x03!"
    else
        warn "0x03 не сработал (readback: $slot73_after) — ищем альтернативу"
    fi
 
    # ВОССТАНОВЛЕНИЕ оригинала
    echo "— восстанавливаю оригинальный keymap..." >> "$OUT_DIR/T4-keymap-write.txt"
    restore_pkt="$(printf '06 03 00 00 01 00 00 FF %s 5A A5' \
        "$(printf '%s' "$old_km" | awk '{ s=$1; for(i=2;i<=NF;i++) s=s " " $i; print s }')")"
    run_kbc raw "$PID_USB" feature-send "$restore_pkt" >> "$OUT_DIR/T4-keymap-write.txt" 2>&1 || true
    sleep 2
    readback="$(run_kbc raw "$PID_USB" feature-send "$(packet_read_keymap 0)" >/dev/null 2>&1; \
               run_kbc raw "$PID_USB" feature-get 06 520 | feature_get_line | tr -d '\r' )"
    slot73_after="$(printf '%s' "$readback" | awk '{print $293,$294,$295,$296}')"
    if [ "x$slot73_after" = "x$slot73_before" ]; then
        ok "восстановлено: слот 73 = $slot73_after"
    else
        warn "ВОССТАНОВЛЕНИЕ НЕ СОВПАЛО: $slot73_after (ожидалось $slot73_before)"
    fi
}
 
# T5: Color Table Write — per-key RGB + self-define hunt
t5_color_table_write() {
    hdr "T5: Color Table Write — per-key RGB + self-define hunt"
    printf '\nT5: Color Table Write — per-key RGB + self-define hunt\n\n' > "$OUT_DIR/T5-color-write.txt"
 
    # Снимок текущей color table
    local ct_before ct_after
    ct_before="$(run_kbc raw "$PID_USB" feature-send "$(packet_read 0x8A 0x0200)" >/dev/null 2>&1; \
                 run_kbc raw "$PID_USB" feature-get 06 520 | feature_get_line | tr -d '\r' )"
    [ -n "$ct_before" ] || { warn "color table не прочиталась — тест невозможен"; return 0; }
 
    # Пишем 6 разных цветов на клавиши 0..5 (R,G,B,C,M,Y)
    # Формат: 128 записей x 4 байта (RR GG BB XX). Поля 9..20 = ключи 0..2, меняем 0..5
    local pat
    pat="$(printf '%s' "$ct_before" | awk '{
        # key 0 = Red
        $9="FF";  $10="00"; $11="00"; $12="00"
        # key 1 = Green
        $13="00"; $14="FF"; $15="00"; $20="00"
        # key 2 = Blue
        $17="00"; $18="00"; $19="FF"; $20="00"
        # key 3 = Cyan
        $21="00"; $22="FF"; $23="FF"; $24="00"
        # key 4 = Magenta
        $25="FF"; $26="00"; $27="FF"; $28="00"
        # key 5 = Yellow
        $29="FF"; $30="FF"; $31="00"; $32="00"
        print }')"
 
    if ! ask_yn "Записать 6 цветов на клавиши 0..5 (R,G,B,C,M,Y) и найти self-define?"; then
        echo "T5 пропущен" >> "$OUT_DIR/T5-color-write.txt"
        return 0
    fi
 
    pat_data="$(printf '%s' "$pat" | awk '{ s=$9; for (i=10; i<=NF; i++) s=s " " $i; print s }')"
    ct_write="$(printf '06 0A 00 00 01 00 00 02 %s' "$pat_data")"
 
    echo "— записываю 6 цветов на клавиши 0..5 (06 0A, 512 б)..." >> "$OUT_DIR/T5-color-write.txt"
    run_kbc raw "$PID_USB" feature-send "$ct_write" >> "$OUT_DIR/T5-color-write.txt" 2>&1 || true
    sleep 2
 
    # readback-verify
    ct_after="$(run_kbc raw "$PID_USB" feature-send "$(packet_read 0x8A 0x0200)" >/dev/null 2>&1; \
               run_kbc raw "$PID_USB" feature-get 06 520 | feature_get_line | tr -d '\r' )"
    local rb1
    rb1="$(printf '%s' "$ct_after" | awk '{for(i=9;i<=32;i++) printf $i" "; print ""}')"
    echo "— readback ключи 0..5: $rb1" >> "$OUT_DIR/T5-color-write.txt"
    if [ "x$rb1" = "xFF 00 00 00 00 FF 00 00 00 FF 00 00 FF 00 00 FF FF 00 00 FF FF 00 00" ]; then
        ok "color table запись работает (readback подтверждает 6 цветов)"
    else
        warn "readback цветов не совпал: $rb1"
    fi
 
    echo "— теперь ищите эффект Self-define: жмите Fn+\\ до НЕПОДВИЖНОГО одноцветного эффекта" >> "$OUT_DIR/T5-color-write.txt"
    if ask_yn "Нашли static/self-define? Переключайте Fn+\\ до однотонного эффекта, затем Enter"; then
        local snap snap_sel
        snap="$(run_kbc raw "$PID_USB" feature-send "$(packet_read 0x84 0x0080)" >/dev/null 2>&1; \
               run_kbc raw "$PID_USB" feature-get 06 520 | feature_get_line | tr -d '\r' )"
        snap_sel="$(printf '%s' "$snap" | awk '{print $19}')"
        echo "  — актуальный селектор data[10] = 0x$snap_sel" >> "$OUT_DIR/T5-color-write.txt"
        printf '%s\n' "$snap" > "$OUT_DIR/T5-config-after.txt"
        ok "конфиг при найденном эффекте сохранён (data[10]=0x$snap_sel)"
 
        if ask_yn "Клавиши 0..5 светятся своими цветами (R,G,B,C,M,Y)?"; then
            ok "SELF-DEFINE НАЙДЕН: data[10]=0x$snap_sel включает per-key цвета!"
            echo "  — ПОЛОЖИТЕЛЬНЫЙ результат: селектор self-define = 0x$snap_sel" >> "$OUT_DIR/T5-color-write.txt"
        else
            warn "цветов нет — селектор другой или требуется другая запись"
            echo "  — self-define не подтверждён (data[10]=0x$snap_sel)" >> "$OUT_DIR/T5-color-write.txt"
        fi
    else
        echo "  — снимок после эффекта не делался" >> "$OUT_DIR/T5-color-write.txt"
    fi
 
    # ВОССТАНОВЛЕНИЕ
    echo "— восстанавливаю исходную color table..." >> "$OUT_DIR/T5-color-write.txt"
    ct_restore="$(printf '06 0A 00 00 01 00 00 02 %s' "$(printf '%s' "$ct_before" | \
        awk '{ s=$9; for (i=10; i<=NF; i++) s=s " " $i; print s }')")"
    run_kbc raw "$PID_USB" feature-send "$ct_restore" >> "$OUT_DIR/T5-color-write.txt" 2>&1 || true
    sleep 2
 
    ct_check="$(run_kbc raw "$PID_USB" feature-send "$(packet_read 0x8A 0x0200)" >/dev/null 2>&1; \
               run_kbc raw "$PID_USB" feature-get 06 520 | feature_get_line | tr -d '\r' )"
    if [ "x$ct_check" = "x$ct_before" ]; then
        ok "color table восстановлена (байт-в-байт)"
    else
        warn "color table после restore отличается — сверьте вручную"
    fi
}
 
# T6: Indicators Config Diff — поиск байтов Caps/Num/Scroll в конфиге
t6_indicators_diff() {
    hdr "T6: Indicators Config Diff — поиск байтов Caps/Num/Scroll в конфиге"
    printf '\nT6: Indicators Config Diff — поиск байтов Caps/Num/Scroll\n\n' > "$OUT_DIR/T6-indicators-diff.txt"
 
    # Снимок конфига ДО
    echo "# feature-send: чтение конфига ДО" > "$OUT_DIR/T6-config-before.txt"
    run_kbc raw "$PID_USB" feature-send "$(packet_read 0x84 0x0080)" >> "$OUT_DIR/T6-config-before.txt" 2>&1 || true
    run_kbc raw "$PID_USB" feature-get 06 520 >> "$OUT_DIR/T6-config-before.txt"
    ok "снимок ДО сохранён → $OUT_DIR/T6-config-before.txt"
 
    echo "  Нажмите-отпустите Caps Lock, Num Lock, Scroll Lock (по одному разу)."
    if ! ask_yn "Готово? Снимок ПОСЛЕ?"; then
        echo "T6 пропущен" >> "$OUT_DIR/T6-indicators-diff.txt"
        return 0
    fi
 
    # Снимок ПОСЛЕ
    run_kbc raw "$PID_USB" feature-send "$(packet_read 0x84 0x0080)" >> "$OUT_DIR/T6-config-after.txt" 2>&1 || true
    run_kbc raw "$PID_USB" feature-get 06 520 >> "$OUT_DIR/T6-config-after.txt"
    ok "снимок ПОСЛЕ сохранён → $OUT_DIR/T6-config-after.txt"
 
    # Diff
    if command -v diff >/dev/null 2>&1; then
        diff -u "$OUT_DIR/T6-config-before.txt" "$OUT_DIR/T6-config-after.txt" > "$OUT_DIR/T6-config-ind-diff.txt" 2>&1 || true
        ok "дифф сохранён → $OUT_DIR/T6-config-ind-diff.txt"
        cat "$OUT_DIR/T6-config-ind-diff.txt"
    else
        warn "diff не найден — сравните вручную T6-config-before.txt и T6-config-after.txt"
    fi
 
    echo "  Ищите изменённые байты ВНЕ эффект-таблицы [58..125] — это и есть слоты индикаторов."
}
 
# ---------- summary ----------
make_summary() {
    hdr "Шаг 9: сводка"
    local f
    {
        echo "Диагностика AULA F99 (USB $PID_USB)"
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
        echo "  write-тесты:   $([ "$WRITE_TESTS" = "1" ] && echo 'запускались' || echo 'НЕ запускались (--write-tests)')"
    } > "$OUT_DIR/10-svodka.txt"
    cat "$OUT_DIR/10-svodka.txt"
}

# ---------- main ----------
while [ $# -gt 0 ]; do
    case "$1" in
        --device) DEVICE="${2:-f99}"; shift 2 ;;
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

[ "$DEVICE" = "f99" ] || die "поддерживается только --device f99"

if [ "$(id -u)" = "0" ]; then
    die "запускайте скрипт от обычного пользователя (не root)"
fi

printf 'Keyboard-Center — реверс AULA F99 (USB 258A:010C)\n'
printf 'Каталог результатов: %s\n' "$OUT_DIR"
mkdir -p "$OUT_DIR"

detect_device
collect_readonly

if [ "$WRITE_TESTS" = "1" ] && [ "$AUTO" != "1" ]; then
    warn "Включены write-тесты — они ПИШУТ в клавиатуру, но каждый восстанавливает исходные данные"
    if ! ask_yn "Продолжить с write-тестами (T1–T6)?"; then
        warn "write-тесты отменены"
    else
        t1_config_roundtrip
        t2_indicators
        t3_selfdefine_hunt
        t4_keymap_write
        t5_color_table_write
        t6_indicators_diff
    fi
else
    warn "write-тесты не запущены (нужен флаг --write-tests и не --auto)"
fi

make_summary

tar -czf "$HOME/kbc-diag-$STAMP.tar.gz" -C "$(dirname "$OUT_DIR")" "$(basename "$OUT_DIR")" 2>/dev/null \
    && ok "архив: $HOME/kbc-diag-$STAMP.tar.gz" \
    || warn "не удалось создать архив (tar недоступен?)"

printf '\nГотово. Каталог: %s\n' "$OUT_DIR"
printf 'Пришлите содержимое каталога (или .tar.gz) владельцу проекта Keyboard-Center.\n'