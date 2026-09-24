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
    daemon = lib.mkOption {
      type = lib.types.submodule {
        options = {
          enable = lib.mkEnableOption "daemon плавного host-рендеринга (kbc daemon)";
          preset = lib.mkOption {
            type = lib.types.str;
            default = "default";
            description = ''
              Имя пресета для kbc daemon (см. `kbc preset save`).
              Для аудио-реактива в фоне без GUI укажите встроенный
              `"audio-viz"` (работает только по USB).
            '';
          };
        };
      };
      default = { };
      description = "systemd user unit, держащий даемон эффектов включённым при входе в графическую сессию";
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [
      cfg.package
    ];

    # .desktop файл и иконка поставляются в пакете.
    # Автозапуск (опционально) можно добавить через:
    #   xdg.autostartEntries — но не по умолчанию.

    # Даемон плавного host-рендеринга: стартует при входе в графическую сессию
    # и держит эффект пресета cfg.daemon.preset. Оверрайды режима — через
    # `kbc preset save` (пока daemon остановлен: systemctl --user stop).
    systemd.user.services.keyboard-center-daemon = lib.mkIf cfg.daemon.enable {
      Unit = {
        Description = "Keyboard-Center daemon (плавные host-эффекты)";
        After = [ "graphical-session.target" ];
        PartOf = [ "graphical-session.target" ];
      };
      Service = {
        Type = "simple";
        ExecStart = "${cfg.package}/bin/kbc daemon --preset ${cfg.daemon.preset}";
        Restart = "on-failure";
        RestartSec = "3";
      };
      Install = {
        WantedBy = [ "graphical-session.target" ];
      };
    };
  };
}