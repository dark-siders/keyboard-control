# --- NixOS-модуль Keyboard-Center ---
# Устанавливает:
#   1. Бинарники keyboard-center, kbc
#   2. Udev-правила (пакетные правила через services.udev.packages)
#   3. Группу plugdev (если её нет) и включает пользователя в неё (опционально)
#
# Пример использования:
#   nixosModules = [ inputs.keyboard-center.nixosModules.default ];
#   programs.keyboard-center.enable = true;

{ keyboard-center }:
{
  lib,
  pkgs,
  config,
  ...
}:
let
  cfg = config.programs.keyboard-center;
in
{
  options.programs.keyboard-center = {
    enable = lib.mkEnableOption "Keyboard-Center (управление клавиатурами AULA/Ajazz)";

    # Пакет: по умолчанию сам keyboard-center (открыт из флейки)
    package = lib.mkOption {
      type = lib.types.package;
      default = keyboard-center;
      description = "Пакет Keyboard-Center для установки";
    };

    # Включить пользователя в группу plugdev (доступ к hidraw без sudo)
    enableUserGroup = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Добавить пользователей в группу plugdev для доступа к клавиатурам";
    };

    # Установить udev-правила через services.udev.packages
    enableUdevRules = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Установить 99-keyboard-center.rules в систему";
    };
  };

  config = lib.mkIf cfg.enable {
    # --- Пакеты -------------------------------------------------------
    environment.systemPackages = [
      cfg.package
    ];

    # --- Udev-правила ----------------------------------------------------
    services.udev.packages = lib.mkIf cfg.enableUdevRules [
      # Правила лежат в $out/lib/udev/rules.d внутри пакета (см. flake.nix)
      cfg.package
    ];

    # --- Группа plugdev ----------------------------------------------------
    # Создаём группу, если её нет (та же, что и в rules-файте).
    # Активируем только если включено.
    users.groups = lib.mkIf cfg.enableUserGroup {
      plugdev = { };
    };

    # Примечание: вы сами указываете пользователей в группе:
    #   users.users.ваше_имя.extraGroups = [ "plugdev" ];
    # Автоматическое добавление в группу всех пользователей не делаем,
    # чтобы не менять права системы молча.

    # --- Дополнительно: слои/группы для hidraw (не всегда нужны) ----
    # В новых ядрах с udev TAG+="uaccess" достаточно. Оставлено как
    # задел: services.udev.rules = lib.mkIf cfg.enableUdevRules (toString ...
    # ) — помещено в пакет автоматически.
  };
}