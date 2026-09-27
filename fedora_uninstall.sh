#!/usr/bin/env bash
# --- Удалялка Keyboard-Center (для Fedora и др. без nix) ---
# Сносит всё, что ставит fedora_install.sh / make install:
#   бинарники (kbc, keyboard-center), .desktop, иконку, udev-правила —
#   и в системном (/usr/local, /usr/share, /etc/udev), и в пользовательском
#   (~/.local) вариантах.
#
# Пользовательские данные (пресеты в ~/.config/keyboard-center) по умолчанию
# НЕ трогаются — снести их можно флагом --purge.
#
# Флаги:
#   --purge    дополнительно удалить пользовательские данные и кэш иконок
#   --keep-rules  оставить /etc/udev/rules.d/99-keyboard-center.rules
#   --dry-run  показать, что будет сделано, без изменений
#
# Примеры:
#   ./fedora_uninstall.sh --dry-run        # посмотреть, что будет удалено
#   sudo ./fedora_uninstall.sh             # снести системную + пользовательскую
#   sudo ./fedora_uninstall.sh --purge     # то же + пресеты и кэш иконок
set -euo pipefail

DRY_RUN=0
PURGE=0
KEEP_RULES=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --purge)      PURGE=1 ;;
        --keep-rules) KEEP_RULES=1 ;;
        --dry-run)    DRY_RUN=1 ;;
        -h|--help)    sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "неизвестный аргумент: $1 (см. './fedora_uninstall.sh --help')" >&2; exit 2 ;;
    esac
    shift
done

# --- вспомогательные функции -------------------------------------------
run() {
    if [[ "$DRY_RUN" -eq 1 ]]; then
        echo "  [dry-run] $*"
    else
        echo "  $*"
        "$@"
    fi
}

# Права: файлы в /usr и /etc удаляются только под root, пользовательские — всегда.
need_root=0
for p in /usr/local/bin/kbc /usr/local/bin/keyboard-center \
         /usr/share/applications/keyboard-center.desktop \
         /usr/share/icons/hicolor/scalable/apps/keyboard-center.svg \
         /etc/udev/rules.d/99-keyboard-center.rules; do
    [[ -e "$p" ]] && need_root=1
done
if [[ "$PURGE" -eq 1 || "$KEEP_RULES" -eq 0 ]]; then
    [[ -e /etc/udev/rules.d/99-keyboard-center.rules ]] && need_root=1
fi

if [[ "$DRY_RUN" -ne 1 && "$need_root" -eq 1 && "$(id -u)" -ne 0 ]]; then
    echo "ошибка: найдены системные файлы — запустите через sudo" >&2
    echo "       sudo $0${*:+ $*}" >&2
    exit 1
fi

echo "Keyboard-Center uninstaller (dry-run=$DRY_RUN, purge=$PURGE)"

# --- списки удаляемого -------------------------------------------------
system_files=(
    /usr/local/bin/kbc
    /usr/local/bin/keyboard-center
    /usr/share/applications/keyboard-center.desktop
    /usr/share/icons/hicolor/scalable/apps/keyboard-center.svg
)
user_files=(
    "$HOME/.local/bin/kbc"
    "$HOME/.local/bin/keyboard-center"
    "$HOME/.local/share/applications/keyboard-center.desktop"
    "$HOME/.local/share/icons/hicolor/scalable/apps/keyboard-center.svg"
)
purge_dirs=(
    "$HOME/.config/keyboard-center"
)
purge_files=(
    "$HOME/.cache/icons"
    "$HOME/.cache/gtk-4.0"
)

# Считаем, что удалять, и показываем план
present=()
for f in "${system_files[@]}" "${user_files[@]}"; do
    [[ -e "$f" ]] && present+=("$f")
done
if [[ ${#present[@]} -eq 0 ]]; then
    echo "Ничего не найдено: Keyboard-Center, похоже, не установлен."
else
    echo "Найдено файлов: ${#present[@]}"
fi

# --- удаление ----------------------------------------------------------
echo "Удаление бинарников, .desktop и иконок..."
run rm -f "${system_files[@]}"
run rm -f "${user_files[@]}"

# --- udev-правила ------------------------------------------------------
if [[ "$KEEP_RULES" -eq 1 ]]; then
    echo "Udev-правила оставлены (--keep-rules)."
else
    echo "Удаление udev-правил..."
    run rm -f /etc/udev/rules.d/99-keyboard-center.rules
    run udevadm control --reload-rules
    run udevadm trigger
fi

# --- пользовательские данные (--purge) ---------------------------------
if [[ "$PURGE" -eq 1 ]]; then
    echo "Удаление пользовательских данных (пресеты, кэш иконок)..."
    for d in "${purge_dirs[@]}"; do
        [[ -d "$d" ]] && run rm -rf "$d"
    done
    for f in "${purge_files[@]}"; do
        [[ -d "$f" ]] && run rm -rf "$f"
    done
else
    echo "Пользовательские данные не тронуты (${purge_dirs[0]}). Снести их: $0 --purge"
fi

cat <<EOF

Готово. Проверить остатки:
  ls -l /usr/local/bin/kbc ~/.local/bin/kbc 2>&1
EOF
