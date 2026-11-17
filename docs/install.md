# Установка

Keyboard-Center разрабатывается и тестируется на **NixOS** — это основной путь установки. Ниже собраны все варианты: NixOS-модуль, Home Manager, сборка без флейка, разработка и Fedora/другие дистрибутивы.

**Общее требование для всех вариантов** — доступ к `/dev/hidraw`: пользователь должен входить в группу `plugdev`, а udev-правила `99-keyboard-center.rules` должны быть установлены. После добавления в группу нужно **перелогиниться**; быстро проверить без перелогина: `sg plugdev -c "kbc list"`.

---

## Вариант 1 — NixOS (флейк в системном конфиге)

В `flake.nix` своего конфига добавьте вход:

```nix
inputs = {
  keyboard-center = {
    url = "git+https://altlinux.space/dark-siders/keyboard-control";
    inputs.nixpkgs.follows = "nixpkgs";   # или "flake-utils.crane.nixpkgs..."
  };
};
```

Подключите модуль и включите:

```nix
{
  imports = [
    inputs.keyboard-center.nixosModules.default
  ];

  programs.keyboard-center.enable = true;

  # Добавить пользователя в группу plugdev (доступ к hidraw без sudo)
  programs.keyboard-center.enableUserGroup = true;   # по умолчанию true
  programs.keyboard-center.enableUdevRules = true;   # по умолчанию true

  # Пользователь в группе (иначе hidraw без root не откроется):
  users.users.ваше_имя.extraGroups = [ "plugdev" ];
}
```

Собрать:

```bash
sudo nixos-rebuild switch --flake .#имя_хоста
```

Модуль ставит: бинарники `keyboard-center`/`kbc` в `environment.systemPackages`, udev-правила через `services.udev.packages`, группу `plugdev`, `.desktop`-файл и иконку.

> [!NOTE] Доступ к `/dev/hidraw`
> Если вы никогда не перелогинивались после добавления в `plugdev` — группа не применится к текущей сессии. Список активных групп: `id -nG`. Быстрый способ без перелогина: обернуть команду в `sg plugdev -c "kbc list"`.

## Вариант 2 — Home Manager (пользовательский уровень)

```nix
{ inputs, ... }:
{
  home-manager.users.ваше_имя = { ... }: {
    imports = [
      inputs.keyboard-center.homeManagerModules.default
    ];
    programs.keyboard-center.enable = true;
    # systemd user unit: держит host-эффект пресета при входе в сессию
    programs.keyboard-center.daemon.enable = true;
    programs.keyboard-center.daemon.preset = "wave2";   # имя пресета (kbc preset save …)
  };
}
```

Ставит бинарники и `.desktop`-файл на уровне пользователя. **Udev-правила при этом должен дать NixOS-модуль** (права на hidraw — системное дело).

Даемон перезапускается без ребилда: `systemctl --user restart keyboard-center-daemon`. До пересоздания пресета останавливайте юнит (`systemctl --user stop keyboard-center-daemon`), иначе `kbc preset save` перезапишет эффект на лету.

**Аудио-реактив в фоне, без GUI:** поставьте `programs.keyboard-center.daemon.preset = "audio-viz"` (встроенный режим верхнего ряда) или `"cava"` (встроенный режим столбиков по известной LED-матрице, включая NumPad у F99). Оба пресета зашиты в бинарник и не требуют пользовательского TOML-файла. Юнит стартует с графической сессией, сам открывает монитор `@DEFAULT_MONITOR@` и **переоткрывает поток** при перезапуске PipeWire/PulseAudio; если клавиатура или звук появятся позже старта — демон дождётся (ленивое открытие + ретраи). Нужен USB: на 2.4G/BT демон честно завершится с ошибкой «только USB».

## Вариант 3 — без флейка (NixOS / чистая сборка)

```bash
# собрать из репозитория
nix-build -E 'import (builtins.getFlake (toString ./.)) { }' -A packages.x86_64-linux.default

# либо через обёртку default.nix
nix-build
```

Результат — в `result/`, при желании установить в окружение:

```bash
# udev-правила
sudo cp 99-keyboard-center.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger

# бинарники (путь по образцу)
cp result/bin/* ~/.local/bin/

# .desktop + иконка
mkdir -p ~/.local/share/applications ~/.local/share/icons/hicolor/scalable/apps
cp result/share/applications/keyboard-center.desktop ~/.local/share/applications/
cp result/share/icons/hicolor/scalable/apps/keyboard-center.svg ~/.local/share/icons/hicolor/scalable/apps/
```

> [!NOTE] Доступ к hidraw без флейка
> Группа `plugdev` должна существовать и пользователь в неё входить (`sudo groupadd plugdev`, `sudo usermod -aG plugdev $USER`, перелогин). Правила выше выставляют `GROUP="plugdev" MODE="0664"`.

## Вариант 4 — разработка (cargo / dev-shell)

```bash
nix develop . --command cargo build --release
nix develop . --command cargo check --all-targets
nix develop . --command cargo test --lib
# без nix (если есть системные gtk4/libadwaita/udev)
cargo build --release
```

Готовые бинарники: `target/release/kbc`, `target/release/keyboard-center`.

## Вариант 5 — Fedora / любой дистрибутив без nix (Makefile + fedora.sh)

Один интерактивный скрипт `fedora.sh` вместо установки и удаления по отдельности.
Запустите без аргументов — скрипт спросит, что делать: **установить с нуля**,
**обновить до последней версии** (пересобрать и переустановить) или **полностью удалить**.
При установке/обновлении дополнительно выбирается место (для пользователя или системно).

**Полный пошаговый сценарий** (Fedora 40/41+, аналогично других дистрибутивах с `dnf`).

### Шаг 1. Разрешения на доступ к клавиатуре (группа `plugdev`)

Доступ к `/dev/hidraw` без root даёт группа `plugdev` и udev-правила `99-keyboard-center.rules`.
Скрипт установки создаёт правило при системной установке, а группу нужно сделать один раз:

```bash
sudo groupadd plugdev           # ошибка «уже существует» — не страшно
sudo usermod -aG plugdev $USER
```

> [!IMPORTANT]
> После добавления в `plugdev` нужно **перелогиниться** (группа применяется только к новой сессии).
> Проверка: `id -nG` должен содержать `plugdev`. Быстрый временный способ без перелогина —
> обернуть команду: `sg plugdev -c "kbc list"`.

### Шаг 2. Установка зависимостей и сборка

```bash
sudo ./fedora.sh deps           # системные зависимости через dnf (Rust, GTK4/hidapi dev-пакеты)
make release                    # cargo build --release
```

Или всё одной командой — скрипт поставит зависимости и сам соберёт:
`sudo ./fedora.sh install --install-deps --system`.

### Шаг 3. Установка

Проще всего запустить интерактивно и выбрать пункт «Установить с нуля»:

```bash
./fedora.sh
```

Либо сразу указать режим:

```bash
./fedora.sh install --user                 # для пользователя: ~/.local/bin, без sudo
sudo ./fedora.sh install --system          # системно: /usr/local/bin + udev + .desktop
sudo ./fedora.sh install --udev-only       # только 99-keyboard-center.rules
```

> [!NOTE]
> При **пользовательской** установке (`--user`) udev-правило не ставится — без него hidraw
> не откроется даже с группой `plugdev`. Решите разово (нужен sudo):
> `sudo ./fedora.sh install --udev-only`.

### Шаг 4. Обновление до последней версии

Скрипт пересобирает release-бинарники из текущих исходников и переустанавливает их туда,
где программа уже стоит (system или user — определяется автоматически):

```bash
./fedora.sh update
```

Или через Makefile: `make update`. Так «прилетают» любые изменения из последнего кода.

### Шаг 5. Проверка

```bash
kbc list                  # видит поддерживаемые клавиатуры (после перелогина)
keyboard-center           # запуск GUI
```

### Шаг 6. Удаление

Запустите интерактивно и выберите «Полностью удалить», либо напрямую:

```bash
./fedora.sh uninstall --dry-run     # сначала посмотреть, что будет удалено
sudo ./fedora.sh uninstall          # бинарники, .desktop, иконка, udev-правила
sudo ./fedora.sh uninstall --purge  # то же + пресеты (~/.config/keyboard-center) и кэш иконок
```

Или через Makefile: `make uninstall`, `make uninstall UNINSTALL_FLAGS=--purge`.
Скрипт требует `sudo`, если найдены системные файлы; пользовательские данные по умолчанию не трогаются.

Полезные таргеты Makefile:

| Цель | Что делает |
|---|---|
| `make build` / `make release` | debug / оптимизированная сборка |
| `make check` | `cargo check --all-targets` |
| `make test` | `cargo test --lib` |
| `make install-user` | локальная установка без sudo (`fedora.sh install --user`) |
| `make install` | системная установка (`fedora.sh install --system`) |
| `make install-udev` | только udev-правила |
| `make update` | пересборка + переустановка туда, где стоит (`fedora.sh update --auto`) |
| `make install-deps` | установка зависимостей через dnf (`fedora.sh deps`) |
| `make uninstall` | полное удаление (`fedora.sh uninstall`) |
| `make uninstall UNINSTALL_FLAGS=--purge` | удаление вместе с пресетами и кэшем иконок |

Режимы `fedora.sh`: `install` (по умолчанию `--user`, также `--system`, `--udev-only`), `update` (`--auto`/`--user`/`--system`), `uninstall`, `deps`. Флаги: `--install-deps` (поставить системные зависимости через dnf), `--purge` (пресеты `~/.config/keyboard-center` и кэш иконок), `--keep-rules` (оставить `/etc/udev/rules.d/99-keyboard-center.rules`), `--dry-run`. Без аргументов — интерактивное меню.

После установки/обновления/удаления скрипт пересобирает кэши иконок и `.desktop` (`gtk-update-icon-cache` + `update-desktop-database`) — без этого иконка в меню приложений не появится/не исчезнет на рабочих столах на базе GTK.
