# --- Flake: Keyboard-Center ---
# Сборка Rust + GTK4 через crane.
# Используется ТОЛЬКО stable nixpkgs (nixos-26.05).

{
  description = "Keyboard-Center — управление механическими клавиатурами AULA/Ajazz";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    # crane — аналог naersk, современный инструмент для Rust в Nix.
    # Пин версии фиксируем в flake.lock.
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        # --- Инструмент crane для сборки Rust ---------------------------
        # Использует стандартный rustPlatform (stable nixpkgs).
        craneLib = crane.mkLib pkgs;

        # Нативные зависимости GTK4 + HID.
        nativeBuildInputs = with pkgs; [
          pkg-config
          glib
          gtk4
          libadwaita
          cairo
          pango
          gdk-pixbuf
          wrapGAppsHook4 # для XDG_DATA_DIRS и GSETTINGS_SCHEMA_DIR (GTK4)
        ];

        buildInputs = with pkgs; [
          gtk4
          libadwaita
          glib
          cairo
          pango
          gdk-pixbuf
          libgcc
          udev # для hidapi (libudev)
          systemdLibs
          pulseaudio # для audio-reactive (libpulse-simple: монитор системного звука)
        ];

        # Общие аргументы для cargo.
        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          pname = "keyboard-center";
          version = "0.4.0";
          doCheck = false; # тесты запускаются отдельно
          nativeBuildInputs = nativeBuildInputs;
          buildInputs = buildInputs;
        };

        # Полный cargoArtifacts (исходники, зависимости) для кэша.
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        # Бинарник.
        keyboard-center = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
            postInstall = ''
              # Установка .desktop + иконки.
              # ВАЖНО: только install -D с ЯВНЫМ именем приёмника.
              # `cp <исходник> $out/share/<каталог>/` копирует файл под
              # basename'ом store-пути исходника, а Nix (2.34) кладёт файлы
              # репозитория в store как `<хэш>-keyboard-center.desktop` /
              # `<хэш>-keyboard-center.svg` — в итоге .desktop и иконка в $out
              # получали префикс-хэш, `Icon=keyboard-center` не резолвился,
              # и приложение показывалось в меню без иконки.
              install -Dm444 ${./data/keyboard-center.desktop} \
                $out/share/applications/keyboard-center.desktop
              install -Dm444 ${./data/keyboard-center.svg} \
                $out/share/icons/hicolor/scalable/apps/keyboard-center.svg
              # Установка udev-правил (для ручной установки)
              install -Dm444 ${./99-keyboard-center.rules} \
                $out/lib/udev/rules.d/99-keyboard-center.rules
            '';
          }
        );
      in
      {
        packages = {
          inherit keyboard-center;
          default = keyboard-center;
        };

        apps = {
          # Пример запуска: nix run .#keyboard-center
          default = {
            type = "app";
            program = "${keyboard-center}/bin/keyboard-center";
          };
          kbc = {
            type = "app";
            program = "${keyboard-center}/bin/kbc";
          };
        };

        checks = {
          inherit keyboard-center;
        };

        # Простой dev-shell для разработки.
        devShells.default = pkgs.mkShell {
          inputsFrom = [
            keyboard-center
          ];
          packages = [
            pkgs.cargo
            pkgs.rustc
            pkgs.rust-analyzer
            pkgs.clippy
            pkgs.gdb
            pkgs.udev
          ];
        };
      }
    )
    // rec {
      # --- NixOS-модуль (импорт из других конфигов) -------------------
      # Вынесен на верхний уровень outputs: флейк-утл раскладывает их
      # по системам, а модуль должен быть system-agnostic.
      # Пакет берём с целевой платформы x86_64-linux.
      nixosModules = {
        default = import ./nix/nixos-module.nix {
          keyboard-center = self.packages."x86_64-linux".keyboard-center;
        };
        keyboard-center = import ./nix/nixos-module.nix {
          keyboard-center = self.packages."x86_64-linux".keyboard-center;
        };
      };

      # --- Home-Manager-модуль ----------------------------------------
      homeManagerModules = {
        default = import ./nix/home-module.nix {
          keyboard-center = self.packages."x86_64-linux".keyboard-center;
        };
        keyboard-center = import ./nix/home-module.nix {
          keyboard-center = self.packages."x86_64-linux".keyboard-center;
        };
      };
    };
}