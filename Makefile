# --- Makefile: Keyboard-Center ---
# Сборка и установка без nix (Fedora/любой дистрибутив с dev-пакетами GTK4).
# Canonical-таргеты: build / check / test / release / install / install-user /
#                    install-udev / install-deps / update / uninstall
#
# Для NixOS используйте flake.nix (nixosModules / homeManagerModules) — тут
# Makefile является альтернативным путём, не заменяя флейк.

CARGO  ?= cargo
FEDORA ?= ./fedora.sh
PREFIX ?= /usr/local
# Флаги для `make uninstall` (например: make uninstall UNINSTALL_FLAGS=--purge)
UNINSTALL_FLAGS ?=

.PHONY: all build check test release install install-user install-udev install-deps update uninstall clean help

all: build

## Сборка debug-версии (быстро).
build:
	$(CARGO) build

## Проверка компиляции всех таргетов (run в CI / перед коммитом).
check:
	$(CARGO) check --all-targets

## Юнит-тесты библиотеки.
test:
	$(CARGO) test --lib

## Сборка оптимизированного release (рекомендуется для использования).
release:
	$(CARGO) build --release

## Установка в систему (нужен sudo): /usr/local/bin, udev, глобальн. .desktop/иконка.
install: release
	$(FEDORA) install --system

## Установка только для текущего пользователя: ~/.local/bin, .desktop/иконка (без sudo).
install-user: release
	$(FEDORA) install --user

## Только udev-правила (нужен sudo; остальное можно ставить вручную).
install-udev:
	$(FEDORA) install --udev-only

## Пересобрать release и переустановить туда, где программа уже стоит (user/system).
update: release
	$(FEDORA) update --auto

## Установить системные зависимости через dnf (rust, GTK4-devel, hidapi, pulseaudio-libs; нужен sudo).
install-deps:
	$(FEDORA) deps

## Полное удаление (бинарники, .desktop, иконка, udev-правила — и системные,
## и пользовательские). Пользовательские данные не трогаются.
## Для них: make uninstall UNINSTALL_FLAGS=--purge
uninstall:
	$(FEDORA) uninstall $(UNINSTALL_FLAGS)

## Полная очистка артефактов cargo.
clean:
	$(CARGO) clean

help:
	@echo "Примеры:"
	@echo "  make build          # debug"
	@echo "  make check          # cargo check --all-targets"
	@echo "  make test           # cargo test --lib"
	@echo "  make release        # release-бинарники в target/release/"
	@echo "  make install        # sudo $(FEDORA) install --system"
	@echo "  make install-user   # $(FEDORA) install --user (без sudo)"
	@echo "  make install-udev   # sudo $(FEDORA) install --udev-only"
	@echo "  make update         # $(FEDORA) update --auto (пересборка + переустановка)"
	@echo "  make install-deps   # sudo $(FEDORA) deps (dnf)"
	@echo "  make uninstall      # $(FEDORA) uninstall (полное удаление)"
	@echo "  make uninstall UNINSTALL_FLAGS=--purge   # + пресеты и кэш иконок"