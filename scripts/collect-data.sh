#!/usr/bin/env bash
# ─── Сбор диагностических данных с клавиатуры (AULA F99 / Ajazz AK820 Pro) ───
# Владельцам реального железа: запустить и прислать архив из ~/kbc-diag-*/
# ТОЛЬКО чтение: list/info/probe/battery + read-запросы feature-get. Никаких
# записей в память (save/rgb/screen/time/set-*) скрипт не выполняет.
#
# Параметры:
#   --device f99|ajazz   клавиатура явно (по умолчанию — автодетект по kbc list)
#   --sg                 запускать kbc через `sg plugdev -c` (если нет перелогина)
#   -h|--help            эта справка
#
# Примеры:
#   ./scripts/collect-data.sh
#   ./scripts/collect-data.sh --device ajazz --sg
#   sg plugdev -c "./scripts/collect-data.sh --device f99"
#
# Результат: папка $HOME/kbc-diag-ГГГГММДД-ЧЧММСС/ + одноимённый tar.gz в $HOME.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KBC=""

DEVICE=""      # автодетект по умолчанию
USE_SG=0

# --- поиск бинарника kbc -------------------------------------------------
# ВНИМАНИЕ: использовать `type -P`, а НЕ `command -v`! Ниже объявлена функция
# с именем `kbc`, поэтому `command -v kbc` вернёт имя функции (рекурсия → SIGSEGV).
find_kbc() {
    if type -P kbc >/dev/null 2>&1; then
        KBC="$(type -P kbc)"
    elif [[ -x "$ROOT/target/release/kbc" ]]; then
        KBC="$ROOT/target/release/kbc"
    else
        echo "не найден бинарник kbc. Соберите его (один раз) и повторите:" >&2
        echo "  sudo ./fedora_install.sh --install-deps   # Fedora и др." >&2
        echo "  make release" >&2
        echo "  # либо: nix develop . --command cargo build --release" >&2
        exit 1
    fi
}

# --- вызов kbc (с учётом --sg) -------------------------------------------
# ВАЖНО: аргументы (в т.ч. полный 520-байтный hex-пакет одной строкой с пробелами)
# экранируются через printf '%q', иначе `sg plugdev -c "$cmd"` разобьёт их по
# пробелам и clap получит 520 отдельных аргументов (feature-send «не удалось»).
kbc() {
    if [[ "$USE_SG" -eq 1 ]]; then
        local cmd
        cmd="$(printf '%q ' "$KBC" "$@")"
        sg plugdev -c "$cmd"
    else
        "$KBC" "$@"
    fi
}

# --- автодетект устройства по kbc list -----------------------------------
# ВАЖНО (10.70): AULA F75 и F99 имеют ОДИН PID 258A:010C, различаются
# только моделью в PSD (0xA4 = F99). Имя в `kbc list` берётся из реестра
# по PID и может показывать «AULA F75» для физической F99 — поэтому
# модель для AULA подтверждаем через `kbc info <pid>` -> «Модель: AULA F99».
detect_device() {
    local out pid kbd
    echo "kbc list — автодетект клавиатуры..."
    out="$(kbc list 2>&1 || true)"
    # формат строки: "  AULA F75  258A:010C  USB  ..."
    if pid="$(printf '%s\n' "$out" | grep -oE '258A:010C|258A:00(4[E-F]|50)' | head -n1)" && [[ -n "$pid" ]]; then
        # кандидат AULA (F75 или F99) — уточняем модель по PSD через kbc info
        kbd="$(kbc info "$pid" 2>&1 || true)"
        if printf '%s\n' "$kbd" | grep -qiE 'Модель: AULA F99'; then
            DEVICE="f99"
        else
            echo "AULA-клавиатура найдена ($pid), но по PSD это не F99" >&2
            echo "($0 собирает данные только для F99/Ajazz; F75 этим скриптом не снимается)." >&2
            echo "--- вывод kbc info $pid ---" >&2
            echo "$kbd" >&2
            exit 1
        fi
    elif printf '%s\n' "$out" | grep -qiE 'Ajazz AK820|0C45:800[9A]|0C45:8099'; then
        DEVICE="ajazz"
    else
        echo "не удалось автоматически определить клавиатуру." >&2
        echo "Запустите с явным указанием: $0 --device f99 | --device ajazz" >&2
        echo "--- вывод kbc list ---" >&2
        echo "$out" >&2
        exit 1
    fi
    echo "Определено устройство: $([ "$DEVICE" = f99 ] && echo 'AULA F99' || echo 'Ajazz AK820 Pro')"
}

# --- найти VID:PID по имени/маске в kbc list -----------------------------
find_pid() {
    # $1 — строка поиска (grep -E), $2 — файл с выводом kbc list
    grep -oE '[0-9A-F]{4}:[0-9A-F]{4}' "$2" | grep -m1 -E "$1" || true
}

# --- главная сборка -------------------------------------------------------
collect() {
    local dir="$1"
    echo "Сборка данных в $dir ..."

    {
        echo "Keyboard-Center диагностика — $(date '+%Y-%m-%d %H:%M:%S %Z')"
        echo "Клавиатура: $([ "$DEVICE" = f99 ] && echo 'AULA F99' || echo 'Ajazz AK820 Pro')"
        echo "Утилита:   $KBC"
        echo "ОС:        $(uname -a)"
        echo
    } > "$dir/00-обзор.txt"

    # 1. kbc list — что видит сканер
    echo "[1] kbc list"
    kbc list > "$dir/01-kbc-list.txt" 2>&1 || true

    # 2. Определение PID/USB-девайса и ресиверов
    local list_file="$dir/01-kbc-list.txt"
    local pid_auto pid_wl
    if [[ "$DEVICE" = f99 ]]; then
        # 10.70: реальный USB PID F99 = 258A:010C (общий с F75; модель уточняется
        # в kbc info). 258A:004E–50 — резерв по таблице (некоторые ревизии).
        pid_auto="$(find_pid '258A:010C|258A:00(4[E-F]|50)' "$list_file")" || true
        pid_wl="$(find_pid '3554:FA09' "$list_file")" || true
    else
        pid_auto="$(find_pid '0C45:800[9A]|0C45:8099' "$list_file")" || true
        pid_wl=""
    fi

    # 3. kbc info для найденного устройств
    echo "[2] kbc info (pid=$pid_auto)"
    if [[ -n "$pid_auto" ]]; then
        kbc info "$pid_auto" > "$dir/02-info.txt" 2>&1 || true
    else
        echo "устройство не найдено в kbc list" > "$dir/02-info.txt"
    fi

    # 4. Разведка интерфейсов/usage pages
    echo "[3] kbc probe (pid=$pid_auto)"
    if [[ -n "$pid_auto" ]]; then
        kbc probe "$pid_auto" > "$dir/03-probe.txt" 2>&1 || true
    else
        echo "устройство не найдено в kbc list" > "$dir/03-probe.txt"
    fi

    # 5. Батарея (F99 — 2.4G-ресивер; Ajazz — USB-девайс)
    echo "[4] kbc battery"
    if [[ -n "$pid_wl" ]]; then
        kbc battery "$pid_wl" > "$dir/04-battery.txt" 2>&1 || true
    elif [[ -n "$pid_auto" ]]; then
        kbc battery "$pid_auto" > "$dir/04-battery.txt" 2>&1 || true
    else
        echo "нет устройства для проверки заряда" > "$dir/04-battery.txt"
    fi

    # 6. Дамп конфиг-региона AULA (read-only: SET-читающего фича-репорта + GET).
    #    Для Ajazz пропускаем: у него другой транспорт (output-репорты), тыкать не будем.
    if [[ "$DEVICE" = f99 && -n "$pid_auto" ]]; then
        echo "[5] дамп конфиг-региона (feature 0x84, F99)"
        {
            echo "# feature-send: SET_тема чтения конфига (полный 520-байтный пакет)"
            kbc raw "$pid_auto" feature-send "$(packet_read 0x84 0x0080)" >/dev/null 2>&1 || echo "# feature-send: не удалось"
            echo "# feature-get 06 520"
            kbc raw "$pid_auto" feature-get 06 520
        } > "$dir/05-config-dump.txt" 2>&1 || true

        echo "[6] дамп таблицы цветов (feature 0x8a, F99)"
        {
            echo "# feature-send: SET_тема чтения таблицы цветов (полный 520-байтный пакет)"
            kbc raw "$pid_auto" feature-send "$(packet_read 0x8a 0x0200)" >/dev/null 2>&1 || echo "# feature-send: не удалось"
            echo "# feature-get 06 520"
            kbc raw "$pid_auto" feature-get 06 520
        } > "$dir/06-color-table.txt" 2>&1 || true

        # 7-8. Дампы keymap-таблиц (0x83): ремап F99 НЕ ревёрснут (10.66), этот
        #       read-запрос — единственный способ проверить, читается ли на F99
        #       канал ключевых карт (Default/FN1), знакомый нам по F75 (10.62).
        #       Read-only: пишущая команда 0x03 в этом сборе НЕ используется.
        echo "[7] дамп keymap Default (feature 0x83, слой 0)"
        {
            echo "# read-запрос keymap Default (read-only; write 0x03 не используется)"
            kbc raw "$pid_auto" feature-send "$(packet_read_keymap 0)" >/dev/null 2>&1 || echo "# feature-send: не удалось"
            echo "# feature-get 06 520"
            kbc raw "$pid_auto" feature-get 06 520
        } > "$dir/07-keymap-default.txt" 2>&1 || true

        echo "[8] дамп keymap FN1 (feature 0x83, слой 1)"
        {
            echo "# read-запрос keymap FN1 (read-only)"
            kbc raw "$pid_auto" feature-send "$(packet_read_keymap 1)" >/dev/null 2>&1 || echo "# feature-send: не удалось"
            echo "# feature-get 06 520"
            kbc raw "$pid_auto" feature-get 06 520
        } > "$dir/08-keymap-fn1.txt" 2>&1 || true

        # 9-11. Интерактивный дифф конфига: два read-only снимка (до/после)
        #       действий на клавиатуре -> найти байты энкодера (ролик громкости)
        #       и индикаторов (Caps/Num/Scroll) в конфиг-регионе (10.66/10.67).
        echo "[9] снимок конфига ДО действий (read-only)"
        {
            echo "# feature-send: чтение конфига (полный 520-байтный пакет)"
            kbc raw "$pid_auto" feature-send "$(packet_read 0x84 0x0080)" >/dev/null 2>&1 || echo "# feature-send: не удалось"
            echo "# feature-get 06 520"
            kbc raw "$pid_auto" feature-get 06 520
        } > "$dir/09-config-before.txt" 2>&1 || true

        echo "  Покрутите РОЛИК ГРОМКОСТИ на клавиатуре 3-4 раза в обе стороны,"
        echo "  затем нажмите-отпустите Caps Lock, Num Lock, Scroll Lock (по одному разу)."
        read -r -p "  Готово? Нажмите Enter: " _ </dev/tty || true

        echo "[10] снимок конфига ПОСЛЕ действий (read-only)"
        {
            echo "# feature-send: чтение конфига (полный 520-байтный пакет)"
            kbc raw "$pid_auto" feature-send "$(packet_read 0x84 0x0080)" >/dev/null 2>&1 || echo "# feature-send: не удалось"
            echo "# feature-get 06 520"
            kbc raw "$pid_auto" feature-get 06 520
        } > "$dir/10-config-after.txt" 2>&1 || true

        echo "[11] разница снимков (дифф)"
        if command -v diff >/dev/null 2>&1; then
            diff -u "$dir/09-config-before.txt" "$dir/10-config-after.txt" > "$dir/11-config-diff.txt" 2>&1 || true
        else
            echo "diff не найден — сравните 09 и 10 вручную" > "$dir/11-config-diff.txt"
        fi
    else
        echo "[5-11] дампы конфига пропущены (нужен AULA F99 по USB)"
        echo "пропущено (не применимо)" > "$dir/05-config-dump.txt"
        echo "пропущено (не применимо)" > "$dir/06-color-table.txt"
        echo "пропущено (не применимо)" > "$dir/07-keymap-default.txt"
        echo "пропущено (не применимо)" > "$dir/08-keymap-fn1.txt"
        echo "пропущено (не применимо)" > "$dir/09-config-before.txt"
        echo "пропущено (не применимо)" > "$dir/10-config-after.txt"
        echo "пропущено (не применимо)" > "$dir/11-config-diff.txt"
    fi

    # 12. Системная информация: lsusb + uevent hidraw
    echo "[12] система (lsusb, uevent)"
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
        echo "  * 01-kbc-list.txt     — видит ли сканер клавиатуру (PID ревизии)"
        echo "  * 02-info.txt         — модель, подключение, батарея"
        echo "  * 03-probe.txt        — интерфейсы/usage pages (ожидание: командный ≥0xFF00)"
        echo "  * 05-config-dump.txt  — конфиг-регион AULA (маркер 5A A5 в конце)"
        echo "  * 06-color-table.txt  — таблица цветов AULA (512 Б, если читается)"
        echo "  * 07-keymap-default   — keymap Default (0x83): читается ли канал ремапа"
        echo "  * 08-keymap-fn1       — keymap FN1 (0x83)"
        echo "  * 09/10/11-конфиг     — снимки до/после + дифф (энкодер/индикаторы)"
    } > "$dir/09-сводка.txt"

    echo "Готово. Данные в: $dir"
}

# --- сформировать полный (520 байт) read-запрос: ему нужно ровно 520 Б ---
# Заголовок: 06 <cmd> 00 00 01 00 <len_lo> <len_hi>, остальное — нули.
packet_read() {
    # Конвертация hex-"0x84"/"0x0080" в числа (работает и в bash, и в zsh).
    local cmd len lo hi i
    cmd=$(( $1 ))
    len=$(( $2 ))
    lo=$(( len & 0xFF ))
    hi=$(( (len >> 8) & 0xFF ))
    # print header "06 cmd 00 00 01 00 lo hi"
    printf '06 %02X 00 00 01 00 %02X %02X' "$cmd" "$lo" "$hi"
    for ((i = 0; i < 512; i++)); do printf ' 00'; done
}

# --- read-запрос keymap-таблицы (0x83): формат по openaula/cloudglides (10.62) ---
# Заголовок: 06 83 00 <layer> 01 00 00 02 + пад до 520. Layer: 0 = Default, 1 = FN1.
# Read-only: write-команда 0x03 здесь не формируется.
packet_read_keymap() {
    local layer i
    layer=$(( $1 ))
    printf '06 83 00 %02X 01 00 00 02' "$layer"
    for ((i = 0; i < 512; i++)); do printf ' 00'; done
}

# --- основной ход ---------------------------------------------------------
while [[ $# -gt 0 ]]; do
    case "$1" in
        --device) DEVICE="$2"; shift 2 ;;
        --sg)     USE_SG=1; shift ;;
        -h|--help)
            sed -n '1,16p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) echo "неизвестный аргумент: $1 (см. './scripts/collect-data.sh --help')" >&2; exit 2 ;;
    esac
done

find_kbc

if [[ -z "$DEVICE" ]]; then
    detect_device
fi
case "$DEVICE" in
    f99|ajazz) ;;
    *) echo "неизвестное устройство: $DEVICE (ожидается f99 или ajazz)" >&2; exit 2 ;;
esac

STAMP="$(date '+%Y%m%d-%H%M%S')"
DIR="$HOME/kbc-diag-${STAMP}"
mkdir -p "$DIR"

trap 'echo "данные частично сохранены в $DIR" >&2' ERR

collect "$DIR"

ARCHIVE="$HOME/kbc-diag-${STAMP}.tar.gz"
if command -v tar >/dev/null 2>&1; then
    tar -C "$HOME" -czf "$ARCHIVE" "kbc-diag-${STAMP}"
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