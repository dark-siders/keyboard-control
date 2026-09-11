# --- Home Manager-модуль Keyboard-Center ---
# Устанавливает бинарники, .desktop-файл и иконку на уровне пользователя.
# Если udev-правила нужны на уровне системы — используйте NixOS-модуль.
#
# Пример:
#   home-manager.users.darkside = { ... }: {
#     imports = [ inputs.keyboard-center.homeManagerModules.default ];
#     programs.keyboard-center.enable = true;
#   };

{ keyboard-center }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.keyboard-center;
in
{
  options.programs.keyboard-center = {
    enable = lib.mkEnableOption "Keyboard-Center";
    package = lib.mkOption {
      type = lib.types.package;
      default = keyboard-center;
      description = "Пакет Keyboard-Center";
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [
      cfg.package
    ];

    # .desktop файл и иконка поставляются в пакете.
    # Автозапуск (опционально) можно добавить через:
    #   xdg.autostartEntries — но не по умолчанию.
  };
}