# --- Makefile: Keyboard-Center ---
# Сборка и установка без nix (Fedora/любой дистрибутив с dev-пакетами GTK4).
# Canonical-таргеты: build / check / test / release / install / install-user /
#                    uninstall / install-udev / install-deps
#
# Для NixOS используйте flake.nix (nixosModules / homeManagerModules) — тут
# Makefile является альтернативным путём, не заменяя флейк.

CARGO    ?= cargo
INSTALLER ?= ./fedora_install.sh
PREFIX   ?= /usr/local

.PHONY: all build check test release install install-user install-udev install-deps uninstall clean help

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
	$(INSTALLER) --system

## Установка только для текущего пользователя: ~/.local/bin, .desktop/иконка (без sudo).
install-user: release
	$(INSTALLER) --user

## Только udev-правила (нужен sudo; остальное можно ставить вручную).
install-udev:
	$(INSTALLER) --udev-only

## Установить системные зависимости через dnf (rust, GTK4-devel, hidapi; нужен sudo).
install-deps:
	$(INSTALLER) --install-deps

## Удаление поставленного скриптом (системная установка снимает и udev-правила).
uninstall:
	$(INSTALLER) --uninstall

## Полная очистка артефактов cargo.
clean:
	$(CARGO) clean

help:
	@echo "Примеры:"
	@echo "  make build          # debug"
	@echo "  make check          # cargo check --all-targets"
	@echo "  make test           # cargo test --lib"
	@echo "  make release        # release-бинарники в target/release/"
	@echo "  make install        # sudo $(INSTALLER) --system"
	@echo "  make install-user   # $(INSTALLER) --user (без sudo)"
	@echo "  make install-udev   # sudo $(INSTALLER) --udev-only"
	@echo "  make install-deps   # sudo $(INSTALLER) --install-deps (dnf)"