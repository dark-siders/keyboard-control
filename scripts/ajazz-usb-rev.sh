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
#
# FRAMING feature-пакетов (главный вопрос 10.106):
#   a — 64 б, buf[0] = report id (0x04 для команд, код эффекта для mode-data);
#       так шлёт gohv через libusb и так устроен наш драйвер ajazz_ak820.rs;
#   b — 65 б, ведущий байт 0x00 + тот же 64-б пакет; так уходил рабочий
#       ручной тест из журнала 10.106 и так шлёт ajazz_web (WebHID reportId 0).
# Дефолт — b (журнал 10.106: 65-б SET_REPORT уходил без EPIPE). Переключается
# флагом --framing. В T2 оба варианта пробуются последовательно.
# После КАЖДОГО командного пакета (START/MODE/FINISH) шлём GET_REPORT 0x04 —
# gohv/драйвер делают это, чтобы «продвинуть стейт-машину» прошивки.
# =============================================================================
# Параметры:
#   --device VID:PID  — устройство явно (по умолчанию автодетект 0C45:XXXX)
#   --sg              — оборачивать команды через `sg plugdev -c "…"`
#   --write-tests     — разрешить секции T1–T4 (запись в клавиатуру)
#   --framing a|b     — framing feature-пакетов (по умолчанию b: 65 б, ведущий 00)
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
FRAMING="b"   # a=64 б (report id в buf[0]), b=65 б (ведущий 00) — см. шапку
FINAL="0xF0"  # финал транзакции подсветки: 0xF0 (gohv/драйвер) или 0x02 (SAVE)
WIN_VARIANT="" # рабочий вариант T2 (framing:final | driver) — для сводки

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

run_kbc_timeout() { # $1=секунды, далее аргументы kbc. Защита от зависания.
    # ВАЖНО: нельзя писать `timeout 5 run_kbc …` — run_kbc это bash-функция,
    # а timeout запускает только внешние команды («Нет такого файла», баг до
    # версии 10.112). Здесь timeout навешивается на РЕАЛЬНЫЙ бинарь/sg.
    local secs="$1"; shift
    if [ "$USE_SG" = "1" ]; then
        local cmd; cmd="$(printf '%q ' "$KBC_BIN" "$@")"
        if command -v timeout >/dev/null 2>&1; then timeout "$secs" sg plugdev -c "$cmd"; else sg plugdev -c "$cmd"; fi
    else
        if command -v timeout >/dev/null 2>&1; then timeout "$secs" "$KBC_BIN" "$@"; else "$KBC_BIN" "$@"; fi
    fi
}

raw_get() { # feature-get с защитой от зависания (timeout 5 с)
    run_kbc_timeout 5 raw "$DEVICE" --path "$CTRL_HIDRAW" feature-get "$1" "$2" 2>&1 || true
}

send_pkt() { # $1=метка, $2=64-б hex-пакет, $3=файл, $4=хендшейк? (1/0, деф.)
    local label="$1" pkt="$2" outfile="$3" hs="${4:-1}" framed
    framed="$(frame_pkt "$pkt")"
    echo "— $label: $framed" >> "$outfile"
    run_kbc raw "$DEVICE" --path "$CTRL_HIDRAW" feature-send "$framed" >> "$outfile" 2>&1 || \
        echo "  ! feature-send не прошёл (EPIPE/ошибка)" >> "$outfile"
    sleep 0.3
    # GET_REPORT-хендшейк 0x04 (gohv: «нужен, чтобы продвинуть стейт-машину»).
    # Для mode-data (report id = код эффекта) хендшейк ломает EP0 — его не шлём.
    if [ "$hs" = "1" ]; then
        echo "  · GET 0x04 (хендшейк):" >> "$outfile"
        raw_get 04 64 >> "$outfile" 2>&1
    fi
}

# ---------- пакетные хелперы control-канала 0xFF13 ----------
# Логический пакет — 64 б; байт 0 = report id/код эффекта. Реальная отправка
# зависит от FRAMING: a — как есть (report id = buf[0]); b — с ведущим 00 (65 б).
# Дельмитер AA 55. ВАЖНО: bash printf НЕ парсит голый hex («FF» → 0, «26» → 0x1A),
# поэтому hex значения идут через hexb(), а десятичные (время) — через decb().

hexb() { # "26"|"0x26"|"FF"|"2" -> двухсимвольный hex-байт ("26"/"02")
    local v="${1#0x}"
    printf '%02X' "$((16#$v))"
}

decb() { # десятичное "13" -> hex "0D" (для времени: прошивка ждёт DECIMAL, не BCD)
    printf '%02X' "$((10#$1))"
}

frame_pkt() { # $1=64-б hex-пакет -> строка нужной framing-длины
    if [ "$FRAMING" = "a" ]; then
        printf '%s' "$1"          # 64 б, report id = первый байт
    else
        printf '00 %s' "$1"       # 65 б, report id 0x00 + 64-б пакет
    fi
}

ctl_packet() { # $1=cmd $2=byte2 $3=byte8 — командный пакет 64 б (report id 0x04)
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

time_data_packet() { # $1=YY $2=MM $3=DD $4=hh $5=mm $6=ss — DECIMAL (не BCD!)
    local i
    printf '00 01 5A %s %s %s %s %s %s 00 04' \
        "$(decb "$1")" "$(decb "$2")" "$(decb "$3")" \
        "$(decb "$4")" "$(decb "$5")" "$(decb "$6")"
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
# Полная транзакция подсветки: framing/final берутся из глобалов FRAMING/FINAL.
apply_effect() { # $1=hex эффекта $2=метка $3=файл $4=br(деф.2) $5=speed(деф.0)
    local br="${4:-2}" sp="${5:-0}"
    send_pkt "START(0x18)" "$(ctl_packet 0x18 0x00 0x01)" "$3"
    send_pkt "MODE(0x13)"   "$(ctl_packet 0x13 0x00 0x01)" "$3"
    send_pkt "data($2)"     "$(mode_data_packet "$1" FF 00 00 "$br" "$sp")" "$3" 0
    send_pkt "FINAL($FINAL)" "$(ctl_packet "$FINAL" 0x00 0x01)" "$3"
    sleep 1
}

# T1: чувствительность транспорта — проходит ли feature-send в обоих framing
t1_transport() {
    hdr "T1: чувствительность транспорта (feature-send, framing A и B)"
    printf '\nT1: чувствительность транспорта (framing A=64б, B=65б)\n\n' \
        > "$OUT_DIR/T1-transport.txt"
    [ -n "$CTRL_HIDRAW" ] || { warn "control-узел не найден — тест пропущен"; return 0; }

    if ! ask_yn "Отправить RESET (0x18) в framing A и B? (проверка, что EP0 не отклоняет)"; then
        echo "T1 пропущен" >> "$OUT_DIR/T1-transport.txt"
        return 0
    fi

    local f saved="$FRAMING" framed
    for f in a b; do
        FRAMING="$f"
        framed="$(frame_pkt "$(ctl_packet 0x18 0x00 0x01)")"
        echo "— framing $f: $framed" >> "$OUT_DIR/T1-transport.txt"
        if run_kbc raw "$DEVICE" --path "$CTRL_HIDRAW" feature-send "$framed" >> "$OUT_DIR/T1-transport.txt" 2>&1; then
            ok "framing $f — feature-send ПРОШЁЛ (EPIPE нет)"
        else
            fail "framing $f — feature-send ОТКЛОНЁН"
        fi
        sleep 0.3
    done
    FRAMING="$saved"

    echo "— GET feature 0x04 (хендшейк, framing-независимо):" >> "$OUT_DIR/T1-transport.txt"
    raw_get 04 64 >> "$OUT_DIR/T1-transport.txt" 2>&1
    if grep -qE '^0[0-9A-Fa-f]' "$OUT_DIR/T1-transport.txt" 2>/dev/null; then
        ok "прошивка ОТВЕЧАЕТ на GET_REPORT 0x04 (данные есть)"
    else
        warn "GET_REPORT 0x04 пуст (прошивка может не отдавать feature на чтение)"
    fi
    ok "дамп ответа → $OUT_DIR/T1-transport.txt"
}

# T2: подсветка Static — перебор framing × финал, затем прямой драйвер.
t2_lighting() {
    hdr "T2: подсветка Static красный — перебор framing/финала"
    local file="$OUT_DIR/T2-static.txt"
    printf '\nT2: подсветка Static красный (перебор вариантов)\n\n' > "$file"
    [ -n "$CTRL_HIDRAW" ] || { warn "control-узел не найден — тест пропущен"; return 0; }

    local variant f fin
    for variant in "b:0xF0" "a:0xF0" "b:0x02"; do
        f="${variant%%:*}"; fin="${variant##*:}"
        if ! ask_yn "Вариант framing=$f final=$fin — зажечь Static красный (ярк.2)? Смотрите на клавиатуру."; then
            echo "— вариант framing=$f final=$fin пропущен" >> "$file"; continue
        fi
        FRAMING="$f"; FINAL="$fin"
        echo "— ВАРИАНТ framing=$f final=$fin:" >> "$file"
        apply_effect 01 "Static FF0000 br2" "$file"
        if ask_yn "Засветилось КРАСНЫМ?"; then
            ok "ПОДСВЕТКА РАБОТАЕТ: framing=$f final=$fin"
            echo "  → ПОДТВЕРЖДЕНО: framing=$f final=$fin" >> "$file"
            WIN_VARIANT="$f:$fin"
            echo "ИТОГ T2: рабочий вариант = $WIN_VARIANT" >> "$file"
            return 0
        fi
        echo "  → красный не виден" >> "$file"
    done

    # Прямой драйверный путь: одна сессия hidapi, хендшейк, framing A, FINISH (0xF0).
    if ask_yn "Проверить драйвер напрямую (kbc daemon, одна сессия hidapi)?"; then
        echo "— драйверный путь: kbc daemon --mode static --color FF0000 --brightness 255" >> "$file"
        run_kbc_timeout 20 daemon --device "$DEVICE" --mode static --color FF0000 --brightness 255 \
            >> "$file" 2>&1 || true
        if ask_yn "Засветилось красным (драйвер)?"; then
            ok "ДРАЙВЕР РАБОТАЕТ напрямую (одна сессия hidapi)"
            echo "  → драйвер подтверждён" >> "$file"
            WIN_VARIANT="driver"; FRAMING="a"; FINAL="0xF0"
            echo "ИТОГ T2: рабочий вариант = driver (framing A, FINISH)" >> "$file"
            return 0
        fi
        echo "  → драйвер: нет" >> "$file"
    fi

    WIN_VARIANT=""
    warn "ни один вариант не засветился — см. T2-static.txt / 05-ctrl-get.txt"
    echo "ИТОГ T2: ничего не подтвердилось" >> "$file"
}

# T3: обход эффектов + всегда оставляем статичный свет (framing/final = рабочие из T2)
t3_sweep() {
    hdr "T3: обход эффектов (Off → Breath → Spectrum → Static) [framing=$FRAMING final=$FINAL]"
    printf '\nT3: обход эффектов (framing=%s final=%s)\n\n' "$FRAMING" "$FINAL" > "$OUT_DIR/T3-sweep.txt"
    [ -n "$CTRL_HIDRAW" ] || { warn "control-узел не найден — тест пропущен"; return 0; }

    while read -r code name; do
        if ask_yn "Поставить эффект $name (0x$code)?"; then
            echo "— эффект $name (0x$code):" >> "$OUT_DIR/T3-sweep.txt"
            apply_effect "$code" "$name" "$OUT_DIR/T3-sweep.txt" 2 2
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
    apply_effect 01 "Static white dim" "$OUT_DIR/T3-sweep.txt" 1 0
    ok "клавиатура оставлена в Static (яркость 1)"
}

# T4: синхронизация времени на TFT-экран (как sync_system_time)
t4_time() {
    hdr "T4: синхронизация времени (START → TIME 0x28 → data → SAVE)"
    printf '\nT4: синхронизация времени (framing=%s)\n\n' "$FRAMING" > "$OUT_DIR/T4-time.txt"
    [ -n "$CTRL_HIDRAW" ] || { warn "control-узел не найден — тест пропущен"; return 0; }

    local y mo d h mi s fmt
    y="$(date +%y)"; mo="$(date +%m)"; d="$(date +%d)"
    h="$(date +%H)"; mi="$(date +%M)"; s="$(date +%S)"
    fmt="$h:$mi:$s"

    if ! ask_yn "Отправить локальное время ($fmt) на экран клавиатуры?"; then
        echo "T4 пропущен" >> "$OUT_DIR/T4-time.txt"
        return 0
    fi

    echo "— время (десятичное, не BCD): YY=$y MM=$mo DD=$d $fmt" >> "$OUT_DIR/T4-time.txt"
    send_pkt "START(0x18)"  "$(ctl_packet 0x18 0x00 0x01)" "$OUT_DIR/T4-time.txt"
    send_pkt "TIME(0x28)"   "$(ctl_packet 0x28 0x00 0x01)" "$OUT_DIR/T4-time.txt"
    send_pkt "data($fmt)"   "$(time_data_packet "$y" "$mo" "$d" "$h" "$mi" "$s")" "$OUT_DIR/T4-time.txt" 0
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
        echo "  framing: $FRAMING (a=64б report id в buf[0], b=65б ведущий 00)"
        echo "  финал транзакции: $FINAL (0xF0 = as gohv/драйвер, 0x02 = SAVE)"
        echo "  write-тесты: $([ "$WRITE_TESTS" = "1" ] && echo 'запускались' || echo 'НЕ запускались (--write-tests)')"
        echo "  РАБОЧИЙ вариант подсветки (T2): ${WIN_VARIANT:-(не подтверждён)}"
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
        --framing) FRAMING="${2:-}"; shift 2 ;;
        --auto)   AUTO=1; shift ;;
        -h|--help)
            sed -n '2,39p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *) die "неизвестный параметр: $1 (см. шапку файла или --help)" ;;
    esac
done

[ -z "$DEVICE" ] || { [[ "$DEVICE" =~ ^0C45:[0-9A-Fa-f]{4}$ ]] || die "ожидается Ajazz VID 0C45:XXXX (получено: $DEVICE)"; }
[[ "$FRAMING" =~ ^[ab]$ ]] || die "неверный --framing: $FRAMING (ожидается a или b)"

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
        t2_lighting
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