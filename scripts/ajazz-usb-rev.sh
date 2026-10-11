#!/usr/bin/env bash
# =============================================================================
# ajazz-usb-rev.sh — реверс-инженерная диагностика Ajazz AK820 Pro (USB 0C45)
#
# Цель: собрать с реальной клавиатуры данные, которых не хватает драйверу
# ajazz_ak820.rs (подсветка через рабочий control-канал 0xFF13):
#   1. принимает ли hidraw feature-reports с report id 0x04 / кодом эффекта
#      (gohv шлёт через libusb в обход ядра; у нас — hidapi/hidraw, и это
#      главный открытый вопрос — журнал 10.106);
#   2. работает ли полная транзакция START(0x18) -> MODE(0x13) -> mode-data
#      -> FINISH(0xF0) — визуально меняется ли подсветка;
#   3. какой режим ставится первым после переподключения (запоминается ли);
#   4. синхронизация времени на TFT-экран (TIME 0x28, см. sync_system_time).
#
# ВАЖНО: по умолчанию скрипт НИЧЕГО НЕ ПИШЕТ в клавиатуру — только читает.
# Write-тесты (T1–T4) запускаются ИСКЛЮЧИТЕЛЬНО с флагом --write-tests.
# Наши транзакции подсветки НЕ шлют SAVE (0x02) — изменения живут в RAM и
# пропадают после переподключения/перезагрузки; наша задача — не оставить
# клавиатуру выключенной (в конце T3 ставим статичный приглушённый свет).
# Исключение: T4 (время) шлёт SAVE — это часы, их и надо сохранить.
# =============================================================================
# Параметры:
#   --device VID:PID  — устройство явно (по умолчанию автодетект 0C45:XXXX)
#   --sg              — оборачивать команды через `sg plugdev -c "…"`
#   --write-tests     — разрешить секции T1–T4 (запись в клавиатуру)
#   --auto            — без интерактивных вопросов (write-тесты пропускаются)
#   -h|--help         — показать справку
# =============================================================================

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="$HOME/kbc-diag-ajazz-rev-$STAMP"

DEVICE=""
USE_SG=0
WRITE_TESTS=0
AUTO=0
CTRL_HIDRAW=""

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
        # Экранирование через printf '%q' — иначе `sg plugdev -c` разобьёт
        # полный hex-аргумент по пробелам и clap не соберёт его (фикс 10.108).
        local cmd
        cmd="$(printf '%q ' "$KBC_BIN" "$@")"
        sg plugdev -c "$cmd"
    else
        "$KBC_BIN" "$@"
    fi
}

raw_get() { # feature-get с защитой от зависания (timeout 5 с)
    if command -v timeout >/dev/null 2>&1; then
        timeout 5 run_kbc raw "$DEVICE" --path "$CTRL_HIDRAW" feature-get "$1" "$2" 2>&1 || true
    else
        run_kbc raw "$DEVICE" --path "$CTRL_HIDRAW" feature-get "$1" "$2" 2>&1 || true
    fi
}

send_pkt() { # $1=метка, $2=hex-пакет; шлёт и логирует в $3 (файл записи теста)
    local outfile="$3"
    echo "— $1: $2" >> "$outfile"
    run_kbc raw "$DEVICE" --path "$CTRL_HIDRAW" feature-send "$2" >> "$outfile" 2>&1 || \
        echo "  ! feature-send не прошёл (EPIPE/ошибка)" >> "$outfile"
    sleep 0.3
}

# ---------- пакетные хелперы control-канала 0xFF13 ----------
# Байт 0 = report id: 0x04 для командных пакетов, код эффекта для mode-data
# (gohv protocol.rs: «byte 0 is the mode value itself»). Дельмитер AA 55.
# ВАЖНО: bash printf НЕ парсит голый hex («FF» → 0, «26» → 0x1A), поэтому все
# значения идут через hexb() — честная интерпретация строки как hexadecimal.

hexb() { # "26"|"0x26"|"FF"|"2" -> двухсимвольный hex-байт ("26"/"02")
    local v="${1#0x}"
    printf '%02X' "$((16#$v))"
}

ctl_packet() { # $1=cmd $2=byte2 $3=byte8 — командный пакет 64 б
    local i
    printf '04 %s %s 00 00 00 00 00 %s' "$(hexb "$1")" "$(hexb "$2")" "$(hexb "$3")"
    for ((i = 9; i < 64; i++)); do printf ' 00'; done
}

mode_data_packet() { # $1=mode $2=R $3=G $4=B $5=br 0..5 $6=speed 0..5
    local i
    printf '%s %s %s %s 00 00 00 00 00 %s %s 00 00 00 55 AA' \
        "$(hexb "$1")" "$(hexb "$2")" "$(hexb "$3")" "$(hexb "$4")" \
        "$(hexb "$5")" "$(hexb "$6")"
    for ((i = 16; i < 64; i++)); do printf ' 00'; done
}

time_data_packet() { # $1=YY $2=MM $3=DD $4=hh $5=mm $6=ss (hex-стиль, напр. год 26)
    local i
    printf '00 01 5A %s %s %s %s %s %s 00 04' \
        "$(hexb "$1")" "$(hexb "$2")" "$(hexb "$3")" \
        "$(hexb "$4")" "$(hexb "$5")" "$(hexb "$6")"
    for ((i = 11; i < 62; i++)); do printf ' 00'; done
    printf ' AA 55'
}

# ---------- детект устройства и узла 0xFF13 ----------
find_ctrl_by_descriptor() {
    # Fallback: скан report_descriptor на usage page 06 13 FF (журнал 10.106).
    local d
    if ! command -v python3 >/dev/null 2>&1; then
        warn "python3 не найден — скан дескрипторов пропущен"
        return 1
    fi
    for d in /sys/class/hidraw/hidraw*; do
        [ -f "$d/device/report_descriptor" ] || continue
        if python3 -c "import sys;print(open(sys.argv[1],'rb').read().hex().upper())" \
                "$d/device/report_descriptor" 2>/dev/null | grep -q '0613FF'; then
            echo "${d##*/}"
            return 0
        fi
    done
    return 1
}

detect_device() {
    local list out pid
    hdr "Шаг 1: поиск Ajazz AK820 Pro по USB"
    list="$(run_kbc list 2>&1)" || die "не удалось выполнить kbc list"
    printf '%s\n' "$list" > "$OUT_DIR/01-kbc-list.txt"

    if [ -z "$DEVICE" ]; then
        pid="$(printf '%s\n' "$list" | grep -oE '0C45:[0-9A-Fa-f]{4}' | head -1 || true)"
        [ -n "$pid" ] || die "Ajazz (0C45:XXXX) не найден в kbc list"
        DEVICE="$pid"
    fi
    ok "устройство: $DEVICE"

    out="$(run_kbc info "$DEVICE" 2>&1 || true)"
    printf '%s\n' "$out" > "$OUT_DIR/02-info.txt"

    run_kbc probe "$DEVICE" > "$OUT_DIR/03-probe.txt" 2>&1 || true
    CTRL_HIDRAW="$(sed -n 's/.*\(\/dev\/hidraw[0-9]*\).*usage_page=0xFF13.*/\1/p' \
        "$OUT_DIR/03-probe.txt" | head -1 || true)"
    if [ -z "$CTRL_HIDRAW" ]; then
        warn "узел 0xFF13 не найден в probe — ищу по report_descriptor"
        CTRL_HIDRAW="$(find_ctrl_by_descriptor || true)"
    fi
    if [ -n "$CTRL_HIDRAW" ]; then
        ok "control-узел 0xFF13: $CTRL_HIDRAW"
    else
        warn "control-канал 0xFF13 НЕ найден — write-тесты будут пропущены/ошибочны"
    fi
}

# ---------- сбор read-only данных ----------
collect_readonly() {
    mkdir -p "$OUT_DIR"

    hdr "Шаг 2: report_descriptor (usage page 0xFF13)"
    : > "$OUT_DIR/04-descriptors.txt"
    if command -v python3 >/dev/null 2>&1; then
        for d in /sys/class/hidraw/hidraw*/device/report_descriptor; do
            [ -f "$d" ] || continue
            {
                echo "--- $d ---"
                python3 -c "import sys;print(open(sys.argv[1],'rb').read().hex().upper())" "$d"
            } >> "$OUT_DIR/04-descriptors.txt" 2>&1 || true
        done
    fi
    ok "дескрипторы сохранены → $OUT_DIR/04-descriptors.txt"

    hdr "Шаг 3: GET feature 0x04 (ответ прошивки на GET_REPORT-хендшейк)"
    if [ -n "$CTRL_HIDRAW" ]; then
        {
            echo "# feature-get 04 64 через $CTRL_HIDRAW"
            raw_get 04 64
        } > "$OUT_DIR/05-ctrl-get.txt" 2>&1 || true
        ok "ответ сохранён → $OUT_DIR/05-ctrl-get.txt"
        if [ -s "$OUT_DIR/05-ctrl-get.txt" ] && \
                grep -qE '^0[0-9A-Fa-f]' "$OUT_DIR/05-ctrl-get.txt"; then
            ok "прошивка ОТВЕЧАЕТ на GET_REPORT отчётом из 0x04"
        else
            warn "прошивка НЕ отвечает на GET_REPORT 0x04 (пусто/ошибка)"
        fi
    else
        warn "control-узел не найден — шаг пропущен"
    fi
}

# ---------- write-тесты (только --write-tests) ----------
apply_effect() { # $1=hex эффекта $2=метка $3=файл
    send_pkt "START(0x18)" "$(ctl_packet 0x18 0x00 0x01)" "$3"
    send_pkt "MODE(0x13)"   "$(ctl_packet 0x13 0x00 0x01)" "$3"
    send_pkt "data($2)"     "$(mode_data_packet "$1" FF 00 00 2 2)" "$3"
    send_pkt "FINISH(0xF0)" "$(ctl_packet 0xF0 0x00 0x01)" "$3"
    sleep 1
}

# T1: чувствительность транспорта — принимает ли hidraw report id 0x04
t1_transport() {
    hdr "T1: чувствительность транспорта (report id 0x04 через hidraw)"
    printf '\nT1: чувствительность транспорта (report id 0x04 через hidraw)\n\n' \
        > "$OUT_DIR/T1-transport.txt"
    [ -n "$CTRL_HIDRAW" ] || { warn "control-узел не найден — тест пропущен"; return 0; }

    if ! ask_yn "Отправить RESET (0x18) с report id 0x04? (проверка, что EP0 не отклоняет)"; then
        echo "T1 пропущен" >> "$OUT_DIR/T1-transport.txt"
        return 0
    fi

    local pkt
    pkt="$(ctl_packet 0x18 0x00 0x01)"
    echo "— отправляю RESET (0x18): $pkt" >> "$OUT_DIR/T1-transport.txt"
    if run_kbc raw "$DEVICE" --path "$CTRL_HIDRAW" feature-send "$pkt" >> "$OUT_DIR/T1-transport.txt" 2>&1; then
        ok "feature-send с report id 0x04 ПРОШЁЛ (EPIPE нет) — транспорт за полной транзакции"
    else
        fail "feature-send с report id 0x04 ОТКЛОНЁН — важный результат"
        warn "hidraw не пускает report id 0x04 → нужен обход (libusb): проверьте 05-ctrl-get.txt"
    fi

    echo "— GET feature 0x04 (хендшейк):" >> "$OUT_DIR/T1-transport.txt"
    raw_get 04 64 >> "$OUT_DIR/T1-transport.txt" 2>&1
    ok "дамп ответа → $OUT_DIR/T1-transport.txt"
}

# T2: подсветка Static — полная транзакция из драйвера
t2_static() {
    hdr "T2: подсветка Static красный (04 18 → 04 13 → data → 04 F0)"
    printf '\nT2: подсветка Static красный\n\n' > "$OUT_DIR/T2-static.txt"
    [ -n "$CTRL_HIDRAW" ] || { warn "control-узел не найден — тест пропущен"; return 0; }

    if ! ask_yn "Зажечь Static красный (яркость 2)? Смотрите на клавиатуру."; then
        echo "T2 пропущен" >> "$OUT_DIR/T2-static.txt"
        return 0
    fi

    send_pkt "START(0x18)" "$(ctl_packet 0x18 0x00 0x01)" "$OUT_DIR/T2-static.txt"
    send_pkt "MODE(0x13)"   "$(ctl_packet 0x13 0x00 0x01)" "$OUT_DIR/T2-static.txt"
    send_pkt "data(Static FF0000 br=2)" \
        "$(mode_data_packet 0x01 FF 00 00 2 0)" "$OUT_DIR/T2-static.txt"
    send_pkt "FINISH(0xF0)" "$(ctl_packet 0xF0 0x00 0x01)" "$OUT_DIR/T2-static.txt"

    if ask_yn "Клавиатура засветилась КРАСНЫМ?"; then
        ok "ПОДСВЕТКА ЧЕРЕЗ 0xFF13 РАБОТАЕТ (транзакция из драйвера верна)"
        echo "— подсветка красным ПОДТВЕРЖДЕНА" >> "$OUT_DIR/T2-static.txt"
    else
        warn "не видно — сверьте T1-transport.txt и пакеты в T2-static.txt"
        echo "— красный не подтверждён" >> "$OUT_DIR/T2-static.txt"
    fi
}

# T3: обход эффектов + всегда оставляем статичный свет
t3_sweep() {
    hdr "T3: обход эффектов (Off → Breath → Spectrum → Static)"
    printf '\nT3: обход эффектов\n\n' > "$OUT_DIR/T3-sweep.txt"
    [ -n "$CTRL_HIDRAW" ] || { warn "control-узел не найден — тест пропущен"; return 0; }

    while read -r code name; do
        if ask_yn "Поставить эффект $name (0x$code)?"; then
            echo "— эффект $name (0x$code):" >> "$OUT_DIR/T3-sweep.txt"
            apply_effect "$code" "$name" "$OUT_DIR/T3-sweep.txt"
            if ask_yn "Эффект $name виден?"; then
                echo "  → подтверждено" >> "$OUT_DIR/T3-sweep.txt"
                ok "$name подтверждён"
            else
                warn "$name не виден"
                echo "  → НЕ подтверждён" >> "$OUT_DIR/T3-sweep.txt"
            fi
        else
            echo "— эффект $name пропущен" >> "$OUT_DIR/T3-sweep.txt"
        fi
    done <<'EOF'
00 Off
07 Breath
08 Spectrum
EOF

    # Обязательно оставляем статичный приглушённый свет (не Off!)
    echo "— финальное состояние: Static (белый, яркость 1, скорость 0)" >> "$OUT_DIR/T3-sweep.txt"
    apply_effect 01 "Static white dim" "$OUT_DIR/T3-sweep.txt"
    ok "клавиатура оставлена в Static (яркость 1)"
}

# T4: синхронизация времени на TFT-экран (как sync_system_time)
t4_time() {
    hdr "T4: синхронизация времени (START → TIME 0x28 → data → SAVE)"
    printf '\nT4: синхронизация времени\n\n' > "$OUT_DIR/T4-time.txt"
    [ -n "$CTRL_HIDRAW" ] || { warn "control-узел не найден — тест пропущен"; return 0; }

    local y mo d h mi s fmt
    y="$(date +%y)"; mo="$(date +%m)"; d="$(date +%d)"
    h="$(date +%H)"; mi="$(date +%M)"; s="$(date +%S)"
    fmt="$h:$mi:$s"

    if ! ask_yn "Отправить локальное время ($fmt) на экран клавиатуры?"; then
        echo "T4 пропущен" >> "$OUT_DIR/T4-time.txt"
        return 0
    fi

    send_pkt "START(0x18)"  "$(ctl_packet 0x18 0x00 0x01)" "$OUT_DIR/T4-time.txt"
    send_pkt "TIME(0x28)"   "$(ctl_packet 0x28 0x00 0x01)" "$OUT_DIR/T4-time.txt"
    send_pkt "data($fmt)"   "$(time_data_packet "$y" "$mo" "$d" "$h" "$mi" "$s")" "$OUT_DIR/T4-time.txt"
    send_pkt "SAVE(0x02)"   "$(ctl_packet 0x02 0x00 0x00)" "$OUT_DIR/T4-time.txt"

    if ask_yn "На экране клавиатуры теперь $fmt?"; then
        ok "синк времени РАБОТАЕТ"
        echo "— время подтверждено" >> "$OUT_DIR/T4-time.txt"
    else
        warn "время не обновилось — 04-descriptors/T1 в помощь"
        echo "— время не подтверждено" >> "$OUT_DIR/T4-time.txt"
    fi
}

# ---------- summary ----------
make_summary() {
    hdr "Шаг 9: сводка"
    local f
    {
        echo "Диагностика Ajazz AK820 Pro (USB $DEVICE)"
        echo "Каталог: $OUT_DIR"
        echo "Время:   $(date '+%Y-%m-%d %H:%M:%S')"
        echo
        echo "Собранные файлы:"
        for f in "$OUT_DIR"/0*.txt "$OUT_DIR"/T*.txt; do
            [ -f "$f" ] && echo "  - $(basename "$f")"
        done
        echo
        echo "Ключевые факты:"
        echo "  control-узел 0xFF13: ${CTRL_HIDRAW:-(не найден)}"
        echo "  write-тесты: $([ "$WRITE_TESTS" = "1" ] && echo 'запускались' || echo 'НЕ запускались (--write-tests)')"
        echo
        echo "Трактовка T1:"
        echo "  * feature-send проходил  → hidraw принимает report id 0x04,"
        echo "    драйвер через hidapi корректен";
        echo "  * feature-send отклонён   → нужен обход (libusb/sg), это найдено."
    } > "$OUT_DIR/10-svodka.txt"
    cat "$OUT_DIR/10-svodka.txt"
}

# ---------- main ----------
while [ $# -gt 0 ]; do
    case "$1" in
        --device) DEVICE="${2:-}"; shift 2 ;;
        --sg)     USE_SG=1; shift ;;
        --write-tests) WRITE_TESTS=1; shift ;;
        --auto)   AUTO=1; shift ;;
        -h|--help)
            sed -n '2,27p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *) die "неизвестный параметр: $1 (см. шапку файла или --help)" ;;
    esac
done

[ -z "$DEVICE" ] || { [[ "$DEVICE" =~ ^0C45:[0-9A-Fa-f]{4}$ ]] || die "ожидается Ajazz VID 0C45:XXXX (получено: $DEVICE)"; }

if [ "$(id -u)" = "0" ]; then
    die "запускайте скрипт от обычного пользователя (не root)"
fi

printf 'Keyboard-Center — реверс Ajazz AK820 Pro (USB, канал 0xFF13)\n'
printf 'Каталог результатов: %s\n' "$OUT_DIR"
mkdir -p "$OUT_DIR"

detect_device
collect_readonly

if [ "$WRITE_TESTS" = "1" ] && [ "$AUTO" != "1" ]; then
    warn "Включены write-тесты — они ПИШУТ в клавиатуру (для подсветки бывают RAM-only,"
    warn "без SAVE; в конце T3 оставляем статичный свет)."
    if ! ask_yn "Продолжить с write-тестами (T1–T4)?"; then
        warn "write-тесты отменены"
    else
        t1_transport
        t2_static
        t3_sweep
        t4_time
    fi
else
    warn "write-тесты не запущены (нужен флаг --write-tests и не --auto)"
fi

make_summary

tar -czf "$HOME/kbc-diag-ajazz-rev-$STAMP.tar.gz" -C "$(dirname "$OUT_DIR")" "$(basename "$OUT_DIR")" 2>/dev/null \
    && ok "архив: $HOME/kbc-diag-ajazz-rev-$STAMP.tar.gz" \
    || warn "не удалось создать архив (tar недоступен?)"

printf '\nГотово. Каталог: %s\n' "$OUT_DIR"
printf 'Пришлите содержимое каталога (или .tar.gz) владельцу проекта Keyboard-Center.\n'