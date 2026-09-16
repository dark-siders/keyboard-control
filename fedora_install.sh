#!/usr/bin/env bash
# --- Установщик Keyboard-Center (для Fedora и др. без nix) ---
# Ставит: бинарники (kbc, keyboard-center), udev-правила, .desktop и иконку.
#
# Режимы:
#   --user      (по умолчанию) в ~/.local/bin + ~/.local/share, без sudo
#   --system    в /usr/local/bin + /etc/udev/rules.d (требует sudo)
#   --udev-only только udev-правила (требует sudo)
#   --install-deps установить системные зависимости через dnf (требует sudo);
#                 используется вместе с --system/--user/--udev-only или отдельно
#   --uninstall удалить то, что ставил этот скрипт
#   --dry-run   показать, что будет сделано, без изменений
#
# Примеры:
#   sudo ./fedora_install.sh --install-deps --system   # зависимости + системная установка
#   sudo ./fedora_install.sh --udev-only --install-deps
#   sudo ./fedora_install.sh --install-deps            # только поставить зависимости
#   ./fedora_install.sh --user
#   sudo ./fedora_install.sh --system
set -euo pipefail

# --- пути проекта -------------------------------------------------------
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RULES_SRC="$ROOT/99-keyboard-center.rules"
DESKTOP_SRC="$ROOT/data/keyboard-center.desktop"
ICON_SRC="$ROOT/data/keyboard-center.svg"

MODE="user"
MODE_SET=0
UNINSTALL=0
DRY_RUN=0
INSTALL_DEPS=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --user)        MODE="user"; MODE_SET=1 ;;
        --system)      MODE="system"; MODE_SET=1 ;;
        --udev-only)   MODE="udev-only"; MODE_SET=1 ;;
        --install-deps) INSTALL_DEPS=1 ;;
        --uninstall)   UNINSTALL=1 ;;
        --dry-run)     DRY_RUN=1 ;;
        -h|--help)
            sed -n '1,16p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) echo "неизвестный аргумент: $1 (см. './fedora_install.sh --help')" >&2; exit 2 ;;
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

# --- определение целевых путей -----------------------------------------
if [[ "$MODE" == "system" || "$MODE" == "udev-only" ]]; then
    BIN_DIR="/usr/local/bin"
    ICON_DIR="/usr/share/icons/hicolor/scalable/apps"
    DESKTOP_DIR="/usr/share/applications"
    RULES_DIR="/etc/udev/rules.d"
    RULES_NAME="99-keyboard-center.rules"
else
    BIN_DIR="$HOME/.local/bin"
    ICON_DIR="$HOME/.local/share/icons/hicolor/scalable/apps"
    DESKTOP_DIR="$HOME/.local/share/applications"
    RULES_DIR="/etc/udev/rules.d"
    RULES_NAME="99-keyboard-center.rules"
fi

# --- проверка исходных файлов ------------------------------------------
for f in "$RULES_SRC" "$DESKTOP_SRC" "$ICON_SRC"; do
    [[ -f "$f" ]] || { echo "ошибка: не найден $f" >&2; exit 1; }
done

echo "Keyboard-Center installer ($MODE${INSTALL_DEPS:+, install-deps})"

# ============================================================
# ЗАВИСИМОСТИ (dnf) — по флагу --install-deps
# ============================================================
if [[ "$INSTALL_DEPS" -eq 1 ]]; then
    echo "Установка системных зависимостей через dnf..."
    if [[ "$(id -u)" -ne 0 && "$DRY_RUN" -ne 1 ]]; then
        echo "для установки зависимостей нужен root: sudo ${0##*/} --install-deps" >&2
        exit 1
    fi
    # Инструменты сборки Rust + dev-пакеты GTK4/hidapi (соответствуют флейк-зависимостям).
    DEP_PACKAGES=(
        rust cargo gcc pkgconf
        gtk4-devel libadwaita-devel
        glib2-devel gdk-pixbuf2-devel
        cairo-devel pango-devel
        hidapi-devel libudev-devel
    )
    run dnf install -y "${DEP_PACKAGES[@]}"
    echo "Зависимости установлены."
    # Если был задан только --install-deps (без режима) — на этом всё.
    if [[ "$MODE_SET" -eq 0 ]]; then
        exit 0
    fi
fi

# ============================================================
# УДАЛЕНИЕ
# ============================================================
if [[ "$UNINSTALL" -eq 1 ]]; then
    echo "Удаление Keyboard-Center"
    if [[ "$MODE" == "system" || "$MODE" == "udev-only" ]]; then
        run rm -f "$BIN_DIR/kbc" "$BIN_DIR/keyboard-center"
        run rm -f "$DESKTOP_DIR/keyboard-center.desktop"
        run rm -f "$ICON_DIR/keyboard-center.svg"
        run rm -f "$RULES_DIR/$RULES_NAME"
        run sudo udevadm control --reload-rules
        run sudo udevadm trigger
    else
        run rm -f "$BIN_DIR/kbc" "$BIN_DIR/keyboard-center"
        run rm -f "$DESKTOP_DIR/keyboard-center.desktop"
        run rm -f "$ICON_DIR/keyboard-center.svg"
        # пользовательские правила не трогаем — они требуют sudo
    fi
    echo "Готово. Пользовательские настройки (профили) не удалялись."
    exit 0
fi

# ============================================================
# СБОРКА (при наличии cargo)
# ============================================================
if [[ "$MODE" != "udev-only" ]]; then
    if command -v cargo >/dev/null 2>&1; then
        echo "Сборка release-версии..."
        (cd "$ROOT" && cargo build --release)
    else
        echo "ВНИМАНИЕ: cargo не найден — берём готовые бинарники из target/release (если есть)." >&2
    fi
    for b in "$ROOT/target/release/kbc" "$ROOT/target/release/keyboard-center"; do
        [[ -x "$b" ]] || { echo "ошибка: нет бинарника $b" >&2; exit 1; }
    done
fi

# ============================================================
# УСТАНОВКА
# ============================================================
if [[ "$MODE" == "system" || "$MODE" == "udev-only" ]]; then
    echo "Установка в систему (требуются права root)..."
    if [[ "$(id -u)" -ne 0 && "$DRY_RUN" -ne 1 ]]; then
        echo "запустите с правами root: sudo ./fedora_install.sh --system (или sudo ${0##*/} --system)" >&2
        exit 1
    fi
    if [[ "$MODE" == "system" ]]; then
        run install -d "$BIN_DIR" "$DESKTOP_DIR" "$ICON_DIR"
        run install -m 0755 "$ROOT/target/release/kbc" "$BIN_DIR/kbc"
        run install -m 0755 "$ROOT/target/release/keyboard-center" "$BIN_DIR/keyboard-center"
        run install -m 0644 "$DESKTOP_SRC" "$DESKTOP_DIR/keyboard-center.desktop"
        run install -m 0644 "$ICON_SRC" "$ICON_DIR/keyboard-center.svg"
    fi
    run install -d "$RULES_DIR"
    run install -m 0644 "$RULES_SRC" "$RULES_DIR/$RULES_NAME"
    run udevadm control --reload-rules
    run udevadm trigger
    echo "Системная установка завершена."
else
    echo "Установка для пользователя $USER..."
    run install -d "$BIN_DIR" "$DESKTOP_DIR" "$ICON_DIR"
    run install -m 0755 "$ROOT/target/release/kbc" "$BIN_DIR/kbc"
    run install -m 0755 "$ROOT/target/release/keyboard-center" "$BIN_DIR/keyboard-center"
    run install -m 0644 "$DESKTOP_SRC" "$DESKTOP_DIR/keyboard-center.desktop"
    run install -m 0644 "$ICON_SRC" "$ICON_DIR/keyboard-center.svg"
    echo "Установка пользователя завершена."
    echo "Путь к группам plugdev и udev-правилам:"
    echo "  sudo install -m 0644 $ROOT/99-keyboard-center.rules /etc/udev/rules.d/"
    echo "  sudo udevadm control --reload-rules && sudo udevadm trigger"
fi

# ============================================================
# ДОСТУП К HIDRAW (памятка)
# ============================================================
cat <<EOF

Памятка по доступу к клавиатуре:
  1. Убедитесь, что пользователь состоит в группе plugdev:
     sudo groupadd plugdev 2>/dev/null || true
     sudo usermod -aG plugdev $USER
  2. Перелогиньтесь (группа применится) или используйте: sg plugdev -c "kbc list"
  3. Если udev-правила ещё не стояли в системе — см. выше (--system / --udev-only).
EOF