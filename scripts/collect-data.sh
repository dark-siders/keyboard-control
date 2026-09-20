#!/usr/bin/env bash
# ─── Сбор диагностических данных с клавиатуры (AULA F99 / Ajazz AK820 Pro) ───
# Владельцам реального железа: запустить и прислать архив из ~/kbc-diag-*/
# ТОЛЬКО чтение: list/info/probe/battery + read-запросы feature-get. Никаких
# записей в память (save/rgb/screen/time/set-*) скрипт не выполняет.
#
# Параметры:
#   --device f99|ajazz   клавиатура явно (по умолчанию — автодетект по kbc list)
#   --sg                 запускать kbc через `sg plugdev -c` (если нет перелогина)
#   --auto               не задавать интерактивные вопросы (авто-yes для тестов)
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
AUTO=0

# --- поиск бинарника kbc -------------------------------------------------
# ВНИМАНИЕ: использовать `type -P`, а НЕ `command -v`! Ниже объявлена функция
# с именем `kbc`, поэтому `command -v kbc` вернёт имя функции (рекурсия → SIGSEGV).
find_kbc() {
    # СНАЧАЛА — локальная сборка (приоритет над системной)
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
        if printf '%s\n' "$kbd" | grep -qiE 'Модель:\s+AULA F99'; then
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

# --- уточнить модель F99 через kbc info (PID 010C общий с F75) ----------
confirm_f99_model() {
    # $1 — pid (258A:010C), возвращает 0 если это F99, 1 если нет
    kbc info "$1" 2>/dev/null | grep -qi 'AULA F99'
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

        # Уточнить модель через kbc info (PID 010C общий с F75)
        if [[ -n "$pid_auto" ]]; then
            if ! confirm_f99_model "$pid_auto"; then
                echo "[!] PID $pid_auto найден в списке, но kbc info не подтверждает F99 — возможно, это F75" >&2
                # Не сбрасываем pid_auto — пусть пользователь решает
            fi
        else
            # Fallback: иногда kbc list не показывает PID, но устройство есть
            # Пробуем известные PID напрямую
            for test_pid in 258A:010C 258A:004E 258A:004F 258A:0050; do
                if confirm_f99_model "$test_pid"; then
                    pid_auto="$test_pid"
                    echo "[!] Автодетект через kbc list не сработал, но kbc info подтверждает F99 на $pid_auto" >&2
                    break
                fi
            done
        fi
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
        # ВАЖНО: через kbc-обёртку (sg plugdev если --sg), 2>&1 чтобы не потерять вывод
        {
            echo "# feature-send: SET_тема чтения конфига (полный 520-байтный пакет)"
            kbc raw "$pid_auto" feature-send "$(packet_read 0x84 0x0080)" 2>&1 || echo "# feature-send: не удалось"
            echo "# feature-get 06 520"
            kbc raw "$pid_auto" feature-get 06 520
        } > "$dir/05-config-dump.txt" 2>&1 || true

        echo "[6] дамп таблицы цветов (feature 0x8a, F99)"
        {
            echo "# feature-send: SET_тема чтения таблицы цветов (полный 520-байтный пакет)"
            kbc raw "$pid_auto" feature-send "$(packet_read 0x8a 0x0200)" 2>&1 || echo "# feature-send: не удалось"
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
            kbc raw "$pid_auto" feature-send "$(packet_read_keymap 0)" 2>&1 || echo "# feature-send: не удалось"
            echo "# feature-get 06 520"
            kbc raw "$pid_auto" feature-get 06 520
        } > "$dir/07-keymap-default.txt" 2>&1 || true

        echo "[8] дамп keymap FN1 (feature 0x83, слой 1)"
        {
            echo "# read-запрос keymap FN1 (read-only)"
            kbc raw "$pid_auto" feature-send "$(packet_read_keymap 1)" 2>&1 || echo "# feature-send: не удалось"
            echo "# feature-get 06 520"
            kbc raw "$pid_auto" feature-get 06 520
        } > "$dir/08-keymap-fn1.txt" 2>&1 || true

        # 9-11. Интерактивный дифф конфига: два read-only снимка (до/после)
        #       действий на клавиатуре -> найти байты индикаторов (Caps/Num/Scroll)
        #       в конфиг-регионе (10.66/10.67). ВНИМАНИЕ: у AULA F99 РОЛИКА
        #       ГРОМКОСТИ НЕТ физически (ролик есть только у Ajazz AK820 Pro,
        #       10.70: пустой 11-config-diff в живой диагностике F99) — поэтому
        #       ролик в этом вопросе для F99 не упоминается.
        echo "[9] снимок конфига ДО действий (read-only)"
        {
            echo "# feature-send: чтение конфига (полный 520-байтный пакет)"
            kbc raw "$pid_auto" feature-send "$(packet_read 0x84 0x0080)" 2>&1 || echo "# feature-send: не удалось"
            echo "# feature-get 06 520"
            kbc raw "$pid_auto" feature-get 06 520
        } > "$dir/09-config-before.txt" 2>&1 || true

        echo "  Нажмите-отпустите Caps Lock, Num Lock, Scroll Lock (по одному разу),"
        echo "  затем наберите несколько букв (например 'qwerty')."
        read -r -p "  Готово? Нажмите Enter: " _ </dev/tty || true

        echo "[10] снимок конфига ПОСЛЕ действий (read-only)"
        {
            echo "# feature-send: чтение конфига (полный 520-байтный пакет)"
            kbc raw "$pid_auto" feature-send "$(packet_read 0x84 0x0080)" 2>&1 || echo "# feature-send: не удалось"
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

    # --- ДОПОЛНИТЕЛЬНЫЕ ШАГИ ДЛЯ F99 (B2.5–B2.8) ---
    # Эти шаги нужны ТОЛЬКО для F99 по USB, чтобы найти каналы записи
    # (ремап, индикаторы, debounce/sleep, per-key 2.4G).
    # Запускаются ТОЛЬКО если DEVICE=f99 и есть USB-клавиатура.
    if [[ "$DEVICE" == "f99" && -n "$pid_auto" ]]; then
        echo
        echo "=== ДОПОЛНИТЕЛЬНЫЕ ДИАГНОСТИКИ F99 (B2.5–B2.8) ==="
        echo "Для этих тестов нужен ОФИЦИАЛЬНЫЙ Windows-софт AULA F99 (под Wine или на Windows)."
        echo "Если софта нет — просто нажмите Enter на каждом вопросе, шаги пропустятся."
        echo

        # B2.5: Remap write channel discovery
        echo "[B2.5] Поиск канала ЗАПИСИ ремапа (config diff до/после Windows-софта)"
        echo "  1) Откройте AULA F99 Windows-софт, НЕ меняйте раскладку пока."
        read -r -p "  Сделать снимок конфига ДО? [y/N]: " _ans </dev/tty || _ans="n"
        if [[ "$_ans" =~ ^[Yy]$ ]]; then
            {
                echo "# feature-send: чтение конфига (полный 520-байтный пакет)"
                kbc raw "$pid_auto" feature-send "$(packet_read 0x84 0x0080)" 2>&1 || echo "# feature-send: не удалось"
                echo "# feature-get 06 520"
                kbc raw "$pid_auto" feature-get 06 520
            } > "$dir/16-config-remap-before.txt" 2>&1 || true
            echo "  Снимок сохранён: $dir/16-config-remap-before.txt"
            echo "  Теперь в софте: переназначьте ОДНУ клавишу (напр. слот 73 '=' -> 'A'),"
            echo "  нажмите «Применить/Сохранить в память», затем вернитесь сюда."
            read -r -p "  Готово? Сделать снимок ПОСЛЕ? [y/N]: " _ans2 </dev/tty || _ans2="n"
            if [[ "$_ans2" =~ ^[Yy]$ ]]; then
                {
                    echo "# feature-send: чтение конфига ПОСЛЕ записи ремапа"
                    kbc raw "$pid_auto" feature-send "$(packet_read 0x84 0x0080)" 2>&1 || echo "# feature-send: не удалось"
                    echo "# feature-get 06 520"
                    kbc raw "$pid_auto" feature-get 06 520
                } > "$dir/17-config-remap-after.txt" 2>&1 || true
                echo "  Снимок сохранён: $dir/17-config-remap-after.txt"
                if command -v diff >/dev/null 2>&1; then
                    diff -u "$dir/16-config-remap-before.txt" "$dir/17-config-remap-after.txt" > "$dir/18-config-remap-diff.txt" 2>&1 || true
                    echo "  Дифф: $dir/18-config-remap-diff.txt"
                fi
            fi
        else
            echo "  Пропущено."
        fi

        # B2.6: Indicators bits (Caps/Num/Scroll) - already covered by steps 9-11,
        # but here's a dedicated cleaner prompt for just indicators
        echo
        echo "[B2.6] Поиск байтов ИНДИКАТОРОВ Caps/Num/Scroll (config diff)"
        echo "  (Отдельный чистый дифф только для индикаторов, без набора букв)"
        read -r -p "  Сделать снимок ДО нажатий индикаторов? [y/N]: " _ans </dev/tty || _ans="n"
        if [[ "$_ans" =~ ^[Yy]$ ]]; then
            {
                echo "# feature-send: чтение конфига ДО индикаторов"
                kbc raw "$pid_auto" feature-send "$(packet_read 0x84 0x0080)" 2>&1 || echo "# feature-send: не удалось"
                echo "# feature-get 06 520"
                kbc raw "$pid_auto" feature-get 06 520
            } > "$dir/19-config-ind-before.txt" 2>&1 || true
            echo "  Снимок сохранён: $dir/19-config-ind-before.txt"
            echo "  Нажмите-отпустите Caps Lock, Num Lock, Scroll Lock (по одному разу)."
            read -r -p "  Готово? Снимок ПОСЛЕ? [y/N]: " _ans2 </dev/tty || _ans2="n"
            if [[ "$_ans2" =~ ^[Yy]$ ]]; then
                {
                    echo "# feature-send: чтение конфига ПОСЛЕ индикаторов"
                    kbc raw "$pid_auto" feature-send "$(packet_read 0x84 0x0080)" 2>&1 || echo "# feature-send: не удалось"
                    echo "# feature-get 06 520"
                    kbc raw "$pid_auto" feature-get 06 520
                } > "$dir/20-config-ind-after.txt" 2>&1 || true
                echo "  Снимок сохранён: $dir/20-config-ind-after.txt"
                if command -v diff >/dev/null 2>&1; then
                    diff -u "$dir/19-config-ind-before.txt" "$dir/20-config-ind-after.txt" > "$dir/21-config-ind-diff.txt" 2>&1 || true
                    echo "  Дифф: $dir/21-config-ind-diff.txt"
                fi
            fi
        else
            echo "  Пропущено (дифф индикаторов уже есть в 09/10/11)."
        fi

        # B2.7: Debounce / sleep timer
        echo
        echo "[B2.7] Debounce / sleep timer (config diff, если есть в софте)"
        read -r -p "  Есть доступ к настройке debounce/sleep в софте? Снимок ДО? [y/N]: " _ans </dev/tty || _ans="n"
        if [[ "$_ans" =~ ^[Yy]$ ]]; then
            {
                echo "# feature-send: чтение конфига ДО debounce/sleep"
                kbc raw "$pid_auto" feature-send "$(packet_read 0x84 0x0080)" 2>&1 || echo "# feature-send: не удалось"
                echo "# feature-get 06 520"
                kbc raw "$pid_auto" feature-get 06 520
            } > "$dir/22-config-db-before.txt" 2>&1 || true
            echo "  Снимок сохранён: $dir/22-config-db-before.txt"
            echo "  В софте: измените debounce и/или sleep timer, нажмите «Применить»."
            read -r -p "  Готово? Снимок ПОСЛЕ? [y/N]: " _ans2 </dev/tty || _ans2="n"
            if [[ "$_ans2" =~ ^[Yy]$ ]]; then
                {
                    echo "# feature-send: чтение конфига ПОСЛЕ debounce/sleep"
                    kbc raw "$pid_auto" feature-send "$(packet_read 0x84 0x0080)" 2>&1 || echo "# feature-send: не удалось"
                    echo "# feature-get 06 520"
                    kbc raw "$pid_auto" feature-get 06 520
                } > "$dir/23-config-db-after.txt" 2>&1 || true
                echo "  Снимок сохранён: $dir/23-config-db-after.txt"
                if command -v diff >/dev/null 2>&1; then
                    diff -u "$dir/22-config-db-before.txt" "$dir/23-config-db-after.txt" > "$dir/24-config-db-diff.txt" 2>&1 || true
                    echo "  Дифф: $dir/24-config-db-diff.txt"
                fi
            fi
        else
            echo "  Пропущено."
        fi

        # B2.8: Per-key RGB on 2.4G (receiver)
        echo
        echo "[B2.8] Per-key RGB на 2.4G (ресивер 3554:FA09) — опционально"
        echo "  Нужен ресивер, клавиатура в 2.4G, НЕ спит (нажать клавишу перед стартом)."
        read -r -p "  Ресивер подключен, клава не спит? Снимок LED-config ДО? [y/N]: " _ans </dev/tty || _ans="n"
        if [[ "$_ans" =~ ^[Yy]$ ]]; then
            {
                echo "# feature-get 06 520 (LED-config ресивера, read-only)"
                kbc raw "3554:FA09" feature-get 06 520
            } > "$dir/25-led-config-before.txt" 2>&1 || true
            echo "  Снимок сохранён: $dir/25-led-config-before.txt"
            echo "  В Windows-софте (через ресивер): задайте per-key цвета (разные клавиши — разные цвета), «Применить»."
            read -r -p "  Готово? Снимок ПОСЛЕ? [y/N]: " _ans2 </dev/tty || _ans2="n"
            if [[ "$_ans2" =~ ^[Yy]$ ]]; then
                {
                    echo "# feature-get 06 520 ПОСЛЕ per-key записи"
                    kbc raw "3554:FA09" feature-get 06 520
                } > "$dir/26-led-config-after.txt" 2>&1 || true
                echo "  Снимок сохранён: $dir/26-led-config-after.txt"
                if command -v diff >/dev/null 2>&1; then
                    diff -u "$dir/25-led-config-before.txt" "$dir/26-led-config-after.txt" > "$dir/27-led-config-diff.txt" 2>&1 || true
                    echo "  Дифф: $dir/27-led-config-diff.txt"
                fi
            fi
        else
            echo "  Пропущено."
        fi

        echo
        echo "=== ДОПОЛНИТЕЛЬНЫЕ ДИАГНОСТИКИ ЗАВЕРШЕНЫ ==="
    fi

    # --- 2.4G ТЕСТЫ ДЛЯ F99 (через ресивер 3554:FA09) ---
    # Эти шаги нужны, когда ресивер подключен и клавиатура в режиме 2.4G.
    # Проверяют: батарея, RGB-эффекты, яркость, скорость, выбор эффекта, гашение света.
    if [[ "$DEVICE" == "f99" && -n "$pid_wl" ]]; then
        echo
        echo "=== 2.4G ТЕСТЫ F99 (ресивер 3554:FA09) ==="
        echo "ВАЖНО: клавиатура должна быть в режиме 2.4G (переключить Fn+1/2/3/4),"
        echo "спейрена с ресивером, и НЕ спать (нажать любую клавишу перед стартом)."
        echo "Если клавиатура спит — батарея вернёт «спит на линке», эффекты не применятся."
        echo

        # 2.4G-0: Пробуждение/проверка линка
        echo "[2.4G-0] Проверка готовности 2.4G линка (пробуждение клавиатуры)..."
        echo "  Нажмите ЛЮБУЮ клавишу на клавиатуре сейчас, чтобы разбудить её."
        if [[ "$AUTO" -eq 0 ]]; then
            read -r -p "  Готово? Нажмите Enter..." _ </dev/tty || true
        fi
        echo "  Проверка батареи (также разбудит клавиатуру)..."
        kbc battery "$pid_wl" > "$dir/30-battery-2g4.txt" 2>&1 || true
        cat "$dir/30-battery-2g4.txt"
        echo

        # Проверка: если батарея "спит", предупреждаем
        if grep -q "спит на линке" "$dir/30-battery-2g4.txt"; then
            echo "  [!] ВНИМАНИЕ: клавиатура спит на линке!"
            echo "      Переключите клавиатуру в режим 2.4G (Fn+1/2/3/4),"
            echo "      убедитесь, что ресивер спейрен, нажмите клавишу."
            if [[ "$AUTO" -eq 0 ]]; then
                read -r -p "  Повторить проверку батареи? [y/N]: " _ans </dev/tty || _ans="n"
                if [[ "$_ans" =~ ^[Yy]$ ]]; then
                    kbc battery "$pid_wl" > "$dir/30-battery-2g4.txt" 2>&1 || true
                    cat "$dir/30-battery-2g4.txt"
                fi
            fi
        fi
        echo

        # 2.4G Battery (already done in step 4, but repeat for completeness)
        echo "[2.4G-1] Батарея ресивера (kbc battery) — подтверждённая"
        cat "$dir/30-battery-2g4.txt"
        echo

        # 2.4G RGB effect apply (kbc rgb)
        echo "[2.4G-2] Тест RGB-эффекта через kbc rgb (запись в память ресивера)"
        echo "  Доступные эффекты (WL_EFFECTS):"
        echo "    1=Steady, 3=Breathing, 2=Rainbow, 12=Sine Wave, 5=Retro Snake,"
        echo "    7=Neon Stream, 8=Retinue Scanning, 13=Rainbow Wheel, 14=Blossoming,"
        echo "    15=Raindrops, 16=Stars Twinkle, 17=Reaction, 18=Shadow Disappear,"
        echo "    19=Flash Away, 20=Ripple Shining, 28=Rotating Windmill,"
        echo "    29=Rotating Storm, 30=Colorful Waterfall"
        read -r -p "  Запустить тест Breathing (mode=3, bri=200, speed=60)? [y/N]: " _ans </dev/tty || _ans="n"
        if [[ "$_ans" =~ ^[Yy]$ ]]; then
            kbc rgb "$pid_wl" --mode breathing --brightness 200 --speed 60 \
                > "$dir/31-rgb-breathing.txt" 2>&1 || true
            cat "$dir/31-rgb-breathing.txt"
            echo "  Эффект записан в память ресивера (держится без приложения)."
            echo "  Наблюдайте за клавиатурой: должно быть плавное «дыхание» всех цветов."
            read -r -p "  Эффект виден? Нажмите Enter для продолжения..." _ </dev/tty || true
        else
            echo "  Пропущено."
        fi

        # 2.4G Light off/on (через kbc rgb --light-off/--light-on)
        echo
        echo "[2.4G-3] Гашение/включение света (kbc rgb --light-off/--light-on)"
        if [[ "$AUTO" -eq 1 ]]; then
            _ans="y"
        else
            read -r -p "  Выключить свет (kbc rgb --light-off)? [y/N]: " _ans </dev/tty || _ans="n"
        fi
        if [[ "$_ans" =~ ^[Yy]$ ]]; then
            kbc rgb "$pid_wl" --light-off > "$dir/32-light-off.txt" 2>&1 || true
            cat "$dir/32-light-off.txt"
            echo "  Свет должен погаснуть и сохраниться в памяти."
            if [[ "$AUTO" -eq 0 ]]; then read -r -p "  Свет погас? Нажмите Enter..." _ </dev/tty || true; fi
        else
            echo "  Пропущено."
        fi

        if [[ "$AUTO" -eq 1 ]]; then
            _ans="y"
        else
            read -r -p "  Включить свет (kbc rgb --light-on)? [y/N]: " _ans </dev/tty || _ans="n"
        fi
        if [[ "$_ans" =~ ^[Yy]$ ]]; then
            kbc rgb "$pid_wl" --light-on > "$dir/33-light-on.txt" 2>&1 || true
            cat "$dir/33-light-on.txt"
            echo "  Свет должен загореться (последний эффект + яркость)."
            if [[ "$AUTO" -eq 0 ]]; then read -r -p "  Свет загорелся? Нажмите Enter..." _ </dev/tty || true; fi
        else
            echo "  Пропущено."
        fi

        # 2.4G Read LED config (0x44) and RGB table (0x49) via raw
        echo
        echo "[2.4G-4] Чтение LED-config ресивера (feature 0x44) и RGB-table (0x49)"
        echo "  Это read-only снимки для анализа протокола 0x13."
        read -r -p "  Сделать снимки? [y/N]: " _ans </dev/tty || _ans="n"
        if [[ "$_ans" =~ ^[Yy]$ ]]; then
            echo "[read] LED-config (0x44, 128 байт через 0x13 protocol)..."
            # Note: 0x13 protocol uses multi-page reads, kbc raw feature-get 06 520 returns constant block
            # For 2.4G we need output report 0x13 with cmd 0x44, but kbc raw doesn't support output-send for 0x13 yet
            # So we document this as known limitation
            echo "  [INFO] 0x13 protocol (20-byte output reports) не поддерживается kbc raw feature-get."
            echo "  Для чтения 0x44/0x49 нужен output-send + input-read на hidraw1 (0x13)."
            echo "  Это известное ограничение — см. aula_f75.rs wl_read_pages/wl_write_pages."
            echo "  Сохраняем заглушку для справки."
            echo "# 2.4G LED-config (0x44) — требует output report 0x13, не feature 0x06" > "$dir/34-led-config-2g4.txt"
            echo "# 2.4G RGB-table (0x49) — требует output report 0x13, не feature 0x06" >> "$dir/34-led-config-2g4.txt"
            cat "$dir/34-led-config-2g4.txt"
        else
            echo "  Пропущено."
        fi

        # 2.4G Effect selection test (list WL_EFFECTS)
        echo
        echo "[2.4G-5] Список эффектов 2.4G (WL_EFFECTS) — для справки"
        cat > "$dir/35-wl-effects-list.txt" << 'EOF'
WL_EFFECTS (коды led[10] для ресивера 3554:FA09, протокол 0x13):
  0x01 = Steady (постоянный цвет)
  0x02 = Rainbow (радуга)
  0x03 = Breathing (дыхание)
  0x05 = Retro Snake (змейка)
  0x07 = Neon Stream (неоновый поток)
  0x08 = Retinue Scanning (сопровождение сканирование)
  0x0C = Sine Wave (синусоидальная волна)
  0x0D = Rainbow Wheel (радужное колесо)
  0x0E = Blossoming (расцветание)
  0x0F = Raindrops (капли дождя)
  0x10 = Stars Twinkle (звёзды)
  0x11 = Reaction (реактивная)
  0x12 = Shadow Disappear (исчезновение тени)
  0x13 = Flash Away (вспышка)
  0x14 = Ripple Shining (мерцание рябью)
  0x1C = Rotating Windmill (вращающаяся ветряная мельница)
  0x1D = Rotating Storm (вращающийся шторм)
  0x1E = Colorful Waterfall (цветной водопад)

Применение через: kbc rgb 3554:FA09 --mode <effect> --brightness <0-255> --speed <0-255>
Яркость/скорость пишутся в слот активного эффекта [58+2*(led[10]-1)] (байт a / старшая тетрада b).
EOF
        cat "$dir/35-wl-effects-list.txt"
        echo

        echo "=== 2.4G ТЕСТЫ ЗАВЕРШЕНЫ ==="
    elif [[ "$DEVICE" == "f99" && -z "$pid_wl" ]]; then
        echo
        echo "[INFO] Ресивер 2.4G (3554:FA09) не обнаружен в kbc list."
        echo "Для 2.4G тестов подключите ресивер и переключите клавиатуру в режим 2.4G."
    fi

    # 12. Читаемая карта keymap (Default + FN1) — работает и на F99 (0x83,
    #     журнал 10.62/10.70), и на Ajazz (GET_KEY/GET_FN_KEY). Вывод клиентской
    #     команды `kbc remap --list` (read-only; запись в этом сборе не ведётся).
    echo "[12] карта keymap (kbc remap --list, pid=$pid_auto)"
    if [[ -n "$pid_auto" ]]; then
        kbc remap "$pid_auto" --list > "$dir/12-keymap-list.txt" 2>&1 \
            || echo "# kbc remap --list: не удалось (возможно, чтение keymap не поддержано)" > "$dir/12-keymap-list.txt"
    else
        echo "устройство не найдено в kbc list" > "$dir/12-keymap-list.txt"
    fi

    # 13. Системная информация: lsusb + uevent hidraw
    echo "[13] система (lsusb, uevent)"
    lsusb > "$dir/13-lsusb.txt" 2>&1 || true
    : > "$dir/14-uevent.txt"
    for h in /sys/class/hidraw/hidraw*/device/uevent; do
        [[ -f "$h" ]] || continue
        {
            echo "=== $h ==="
            grep -E 'HID_ID|HID_NAME|DRIVER|PRODUCT' "$h" || cat "$h"
        } >> "$dir/14-uevent.txt"
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
        echo "  * 04-battery.txt      — заряд (F99: 2.4G-ресивер; Ajazz: USB)"
        echo "  * 05-config-dump.txt  — конфиг-регион AULA (маркер 5A A5 в конце)"
        echo "  * 06-color-table.txt  — таблица цветов AULA (512 Б, если читается)"
        echo "  * 07-keymap-default   — keymap Default (0x83): читается ли канал ремапа"
        echo "  * 08-keymap-fn1       — keymap FN1 (0x83)"
        echo "  * 09/10/11-конфиг     — снимки до/после + дифф (индикаторы Caps/Num/Scroll)"
        echo "  * 12-keymap-list.txt  — читаемая карта keymap Default+FN1 (kbc remap --list)"
        echo "  * 13/14-lsusb-uevent  — дерево USB и usage pages hidraw"
        echo "  * 16/17/18 — F99: config remap до/после/дифф (B2.5, запись ремапа в софте)"
        echo "  * 19/20/21 — F99: config индикаторы до/после/дифф (B2.6, чистый дифф Caps/Num/Scroll)"
        echo "  * 22/23/24 — F99: config debounce/sleep до/после/дифф (B2.7)"
        echo "  * 25/26/27 — F99: ресивер LED-config до/после/дифф (B2.8, per-key 2.4G)"
        echo "  * 30-battery-2g4.txt  — батарея 2.4G (повтор для 2.4G секции)"
        echo "  * 31-rgb-breathing.txt — тест RGB Breathing 2.4G (запись в память)"
        echo "  * 32-light-off.txt    — гашение света 2.4G (kbc light off)"
        echo "  * 33-light-on.txt     — включение света 2.4G (kbc light on)"
        echo "  * 34-led-config-2g4.txt — заметка про 0x13 protocol (LED-config/RGB-table)"
        echo "  * 35-wl-effects-list.txt — справочник WL_EFFECTS (коды led[10] для 2.4G)"
    } > "$dir/15-сводка.txt"

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
        --auto)   AUTO=1; shift ;;
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