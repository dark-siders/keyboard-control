#!/usr/bin/env bash
# --- Keyboard-Center: установка / обновление / удаление (Fedora и др. без nix) ---
# Единый скрипт вместо fedora_install.sh и fedora_uninstall.sh.
#
# БЕЗ АРГУМЕНТОВ — интерактивное меню: скрипт спрашивает, что делать
# (установить с нуля / обновить до последней версии / полностью удалить),
# а при установке — куда (для пользователя или системно).
#
# Режимы (неинтерактивно, для make/автоматизации):
#   ./fedora.sh install  [--user|--system|--udev-only] [--install-deps] [--dry-run]
#   ./fedora.sh update   [--user|--system|--auto]      [--install-deps] [--dry-run]
#   ./fedora.sh uninstall [--purge] [--keep-rules] [--dry-run]
#   ./fedora.sh deps     [--dry-run]            # только системные зависимости (dnf)
#
# Флаги:
#   --user       установка для текущего пользователя: ~/.local/bin, без sudo
#   --system     системная установка: /usr/local/bin + /etc/udev/rules.d (sudo)
#   --udev-only  только udev-правила (sudo)
#   --auto       (для update) определить место установки автоматически
#   --install-deps  поставить системные зависимости через dnf (sudo)
#   --purge      (для uninstall) + пользовательские данные и кэш иконок
#   --keep-rules (для uninstall) оставить /etc/udev/rules.d/99-keyboard-center.rules
#   --dry-run    показать действия без изменений
#
# Примеры:
#   ./fedora.sh                                   # интерактивно
#   sudo ./fedora.sh install --install-deps --system
#   ./fedora.sh update                            # пересборка + установка по факту
#   sudo ./fedora.sh uninstall --purge
set -euo pipefail

# --- пути проекта -------------------------------------------------------
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RULES_SRC="$ROOT/99-keyboard-center.rules"
DESKTOP_SRC="$ROOT/data/keyboard-center.desktop"
ICON_SRC="$ROOT/data/keyboard-center.svg"

SYSTEM_BIN_DIR="/usr/local/bin"
SYSTEM_ICON_DIR="/usr/share/icons/hicolor/scalable/apps"
SYSTEM_DESKTOP_DIR="/usr/share/applications"
USER_BIN_DIR="$HOME/.local/bin"
USER_ICON_DIR="$HOME/.local/share/icons/hicolor/scalable/apps"
USER_DESKTOP_DIR="$HOME/.local/share/applications"
RULES_DIR="/etc/udev/rules.d"
RULES_NAME="99-keyboard-center.rules"

ACTION=""       # install | update | uninstall | deps
TARGET=""       # user | system | udev-only | auto
INSTALL_DEPS=0
DRY_RUN=0
PURGE=0
KEEP_RULES=0

# --- разбор аргументов --------------------------------------------------
while [[ $# -gt 0 ]]; do
    case "$1" in
        install|update|uninstall|deps) ACTION="$1" ;;
        --user)      TARGET="user" ;;
        --system)    TARGET="system" ;;
        --udev-only) TARGET="udev-only" ;;
        --auto)      TARGET="auto" ;;
        --install-deps) INSTALL_DEPS=1 ;;
        --purge)     PURGE=1 ;;
        --keep-rules) KEEP_RULES=1 ;;
        --dry-run)   DRY_RUN=1 ;;
        -h|--help)   sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "неизвестный аргумент: $1 (см. './fedora.sh --help')" >&2; exit 2 ;;
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

# Требует root (для --system/--udev-only и dnf); при необходимости
# перезапускает себя через sudo с теми же аргументами.
need_root() {
    [[ "$DRY_RUN" -eq 1 ]] && return 0
    [[ "$(id -u)" -eq 0 ]] && return 0
    echo "нужны права root — перезапускаю через sudo:"
    echo "  sudo $0 install --${TARGET}${INSTALL_DEPS:+, --install-deps}"
    exec sudo "$0" install --"$TARGET" $([ "$INSTALL_DEPS" -eq 1 ] && echo --install-deps)
}

# Проверка исходных файлов.
check_sources() {
    for f in "$RULES_SRC" "$DESKTOP_SRC" "$ICON_SRC"; do
        [[ -f "$f" ]] || { echo "ошибка: не найден $f" >&2; exit 1; }
    done
}

# Позиция установки для update: смотрит, где уже стоит программа.
detect_target() {
    local has_sys=0 has_user=0
    [[ -e "$SYSTEM_BIN_DIR/keyboard-center" || -e "$SYSTEM_BIN_DIR/kbc" ]] && has_sys=1
    [[ -e "$USER_BIN_DIR/keyboard-center" || -e "$USER_BIN_DIR/kbc" ]] && has_user=1
    if [[ "$has_sys" -eq 1 && "$has_user" -eq 1 ]]; then
        echo "Программа установлена и в систему, и для пользователя."
        if [[ -t 0 ]]; then
            echo "Куда обновлять?  [1] system (по умолчанию)  [2] user:"
            read -r -p "> " answer
            if [[ "$answer" == "2" ]]; then echo "user"; else echo "system"; fi
        else
            # Неинтерактивный вызов: обновляем системную установку (приоритет system).
            echo "Беру system (неинтерактивный режим)." >&2
            echo "system"
        fi
    elif [[ "$has_sys" -eq 1 ]]; then
        echo "system"
    elif [[ "$has_user" -eq 1 ]]; then
        echo "user"
    else
        if [[ -t 0 ]]; then
            echo "Программа не установлена — будет сделана установка заново."
            echo "Куда?  [1] user (по умолчанию)  [2] system:"
            read -r -p "> " answer
            if [[ "$answer" == "2" ]]; then echo "system"; else echo "user"; fi
        else
            # Неинтерактивный вызов (make/CI): берём user по умолчанию.
            echo "Программа не установлена — ставлю заново для пользователя (user)." >&2
            echo "user"
        fi
    fi
}

# Сборка release-бинарников (обновление требует свежую сборку).
build_release() {
    if command -v cargo >/dev/null 2>&1; then
        run bash -c "cd \"$ROOT\" && cargo build --release"
    else
        echo "ВНИМАНИЕ: cargo не найден — беру готовые бинарники из target/release (если есть)." >&2
    fi
    for b in "$ROOT/target/release/kbc" "$ROOT/target/release/keyboard-center"; do
        [[ -x "$b" ]] || { echo "ошибка: нет бинарника $b (нужна сборка через cargo)" >&2; exit 1; }
    done
}

install_files() { # $1 = user|system
    local mode="$1"
    if [[ "$mode" == "system" ]]; then
        echo "Установка в систему..."
        run install -d "$SYSTEM_BIN_DIR" "$SYSTEM_DESKTOP_DIR" "$SYSTEM_ICON_DIR"
        run install -m 0755 "$ROOT/target/release/kbc" "$SYSTEM_BIN_DIR/kbc"
        run install -m 0755 "$ROOT/target/release/keyboard-center" "$SYSTEM_BIN_DIR/keyboard-center"
        run install -m 0644 "$DESKTOP_SRC" "$SYSTEM_DESKTOP_DIR/keyboard-center.desktop"
        run install -m 0644 "$ICON_SRC" "$SYSTEM_ICON_DIR/keyboard-center.svg"
        run install -d "$RULES_DIR"
        run install -m 0644 "$RULES_SRC" "$RULES_DIR/$RULES_NAME"
        run udevadm control --reload-rules
        run udevadm trigger
        echo "Системная установка завершена."
    else
        echo "Установка для пользователя $USER..."
        run install -d "$USER_BIN_DIR" "$USER_DESKTOP_DIR" "$USER_ICON_DIR"
        run install -m 0755 "$ROOT/target/release/kbc" "$USER_BIN_DIR/kbc"
        run install -m 0755 "$ROOT/target/release/keyboard-center" "$USER_BIN_DIR/keyboard-center"
        run install -m 0644 "$DESKTOP_SRC" "$USER_DESKTOP_DIR/keyboard-center.desktop"
        run install -m 0644 "$ICON_SRC" "$USER_ICON_DIR/keyboard-center.svg"
        echo "Установка пользователя завершена."
        echo "Для доступа к клавиатуре нужны udev-правила (только system):"
        echo "  sudo $0 install --udev-only"
    fi
}

install_udev_rules() {
    echo "Установка udev-правил..."
    run install -d "$RULES_DIR"
    run install -m 0644 "$RULES_SRC" "$RULES_DIR/$RULES_NAME"
    run udevadm control --reload-rules
    run udevadm trigger
    echo "Udev-правила установлены."
}

install_deps() {
    echo "Установка системных зависимостей через dnf..."
    [[ "$DRY_RUN" -eq 1 || "$(id -u)" -eq 0 ]] || {
        echo "для установки зависимостей нужен root: sudo $0 deps" >&2; exit 1
    }
    DEP_PACKAGES=(
        rust cargo gcc pkgconf
        gtk4-devel libadwaita-devel
        glib2-devel gdk-pixbuf2-devel
        cairo-devel pango-devel
        hidapi-devel libudev-devel
        pulseaudio-libs-devel
    )
    run dnf install -y "${DEP_PACKAGES[@]}"
    echo "Зависимости установлены."
}

do_uninstall() {
    # Права: файлы в /usr и /etc удаляются только под root.
    local need_root=0
    for p in "$SYSTEM_BIN_DIR/kbc" "$SYSTEM_BIN_DIR/keyboard-center" \
             "$SYSTEM_DESKTOP_DIR/keyboard-center.desktop" \
             "$SYSTEM_ICON_DIR/keyboard-center.svg" \
             "$RULES_DIR/$RULES_NAME"; do
        [[ -e "$p" ]] && need_root=1
    done
    if [[ "$KEEP_RULES" -eq 0 ]]; then
        [[ -e "$RULES_DIR/$RULES_NAME" ]] && need_root=1
    fi
    if [[ "$need_root" -eq 1 && "$(id -u)" -ne 0 && "$DRY_RUN" -ne 1 ]]; then
        echo "ошибка: найдены системные файлы — запустите через sudo" >&2
        echo "       sudo $0 uninstall $([ "$PURGE" -eq 1 ] && echo --purge)" >&2
        exit 1
    fi

    local present=0
    for f in "$SYSTEM_BIN_DIR/kbc" "$SYSTEM_BIN_DIR/keyboard-center" \
             "$SYSTEM_DESKTOP_DIR/keyboard-center.desktop" \
             "$SYSTEM_ICON_DIR/keyboard-center.svg" \
             "$USER_BIN_DIR/kbc" "$USER_BIN_DIR/keyboard-center" \
             "$USER_DESKTOP_DIR/keyboard-center.desktop" \
             "$USER_ICON_DIR/keyboard-center.svg"; do
        [[ -e "$f" ]] && present=1 && break
    done
    if [[ "$present" -eq 0 ]]; then
        echo "Ничего не найдено: Keyboard-Center, похоже, не установлен."
    fi

    echo "Удаление бинарников, .desktop и иконок..."
    run rm -f "$SYSTEM_BIN_DIR/kbc" "$SYSTEM_BIN_DIR/keyboard-center"
    run rm -f "$SYSTEM_DESKTOP_DIR/keyboard-center.desktop"
    run rm -f "$SYSTEM_ICON_DIR/keyboard-center.svg"
    run rm -f "$USER_BIN_DIR/kbc" "$USER_BIN_DIR/keyboard-center"
    run rm -f "$USER_DESKTOP_DIR/keyboard-center.desktop"
    run rm -f "$USER_ICON_DIR/keyboard-center.svg"

    if [[ "$KEEP_RULES" -eq 1 ]]; then
        echo "Udev-правила оставлены (--keep-rules)."
    else
        echo "Удаление udev-правил..."
        run rm -f "$RULES_DIR/$RULES_NAME"
        run udevadm control --reload-rules
        run udevadm trigger
    fi

    if [[ "$PURGE" -eq 1 ]]; then
        echo "Удаление пользовательских данных (пресеты, кэш иконок)..."
        [[ -d "$HOME/.config/keyboard-center" ]] && run rm -rf "$HOME/.config/keyboard-center"
        [[ -d "$HOME/.cache/icons" ]] && run rm -rf "$HOME/.cache/icons"
        [[ -d "$HOME/.cache/gtk-4.0" ]] && run rm -rf "$HOME/.cache/gtk-4.0"
    else
        echo "Пользовательские данные не тронуты (~/.config/keyboard-center). Снести их: $0 uninstall --purge"
    fi
    echo "Готово."
}

# --- интерактивное меню --------------------------------------------------
interactive_menu() {
    check_sources
    echo "=== Keyboard-Center (Fedora / без nix) ==="
    echo "Что сделать?"
    echo "  1) Установить с нуля"
    echo "  2) Обновить до последней версии (пересобрать и переустановить)"
    echo "  3) Полностью удалить"
    echo "  0) Выход"
    local choice
    read -r -p "> " choice
    case "$choice" in
        1)
            echo "Куда установить?"
            echo "  1) Для пользователя $USER (без sudo)"
            echo "  2) Системно (требует sudo)"
            read -r -p "> " target
            if [[ "$target" == "2" ]]; then
                TARGET="system"; ACTION="install"; need_root
            else
                TARGET="user"; ACTION="install"
            fi
            ;;
        2)
            ACTION="update"
            if [[ "$TARGET" == "" ]]; then
                TARGET="$(detect_target)"
            fi
            if [[ "$TARGET" == "system" ]]; then
                need_root
            fi
            ;;
        3)
            echo "Удалить и пользовательские данные (пресеты, кэш иконок)?  [y/N]"
            read -r -p "> " purge
            if [[ "$purge" == "y" || "$purge" == "Y" ]]; then PURGE=1; fi
            ACTION="uninstall"
            ;;
        0|"") exit 0 ;;
        *) echo "Неизвестный выбор."; exit 2 ;;
    esac
}

# ============================================================
# ДИСПЕТЧЕР
# ============================================================
if [[ "$ACTION" == "" ]]; then
    interactive_menu
fi

case "$ACTION" in
    deps)
        install_deps
        ;;
    install)
        check_sources
        [[ "$INSTALL_DEPS" -eq 1 ]] && install_deps
        if [[ "$TARGET" == "udev-only" ]]; then
            check_sources
            need_root
            install_udev_rules
        elif [[ "$TARGET" == "system" ]]; then
            need_root
            build_release
            install_files system
        else
            build_release
            install_files user
        fi
        ;;
    update)
        check_sources
        [[ "$INSTALL_DEPS" -eq 1 ]] && install_deps
        if [[ "$TARGET" == "" || "$TARGET" == "auto" ]]; then
            TARGET="$(detect_target)"
        fi
        if [[ "$TARGET" == "system" ]]; then
            need_root
        fi
        build_release
        install_files "$TARGET"
        echo "Обновление завершено: бинарники пересобраны и установлены."
        ;;
    uninstall)
        do_uninstall
        ;;
esac

# --- памятка по доступу -------------------------------------------------
if [[ "$ACTION" == "install" || "$ACTION" == "update" ]] && [[ "$TARGET" == "user" ]]; then
    cat <<EOF

Памятка по доступу к клавиатуре:
  1. Убедитесь, что пользователь состоит в группе plugdev:
     sudo groupadd plugdev 2>/dev/null || true
     sudo usermod -aG plugdev $USER
  2. Перелогиньтесь (группа применится) или используйте: sg plugdev -c "kbc list"
  3. udev-правила ставятся только системно: sudo $0 install --udev-only
EOF
fi