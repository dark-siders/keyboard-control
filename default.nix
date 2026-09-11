# --- Легаси default.nix (для использования без flakes) ---
# Обёртка: позволяет собрать пакет через `nix-build`.
# Основной путь сборки — flake.nix (см. flake.lock).

{ pkgs ? import <nixpkgs> { } }:
let
  # Загружаем flake-выражение напрямую (без overrides)
  flake = builtins.getFlake (toString ./.);
in
flake.packages.${pkgs.system}.default