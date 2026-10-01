// --- Трей GUI: сворачивание в системный трей (SNI) ---
// Журнал 10.111.
//
// GTK4/Libadwaita встроенного трея НЕ имеют. Реализация на крэйте `ksni`:
//   * org.kde.StatusNotifierItem + com.canonical.dbusmenu (меню) — чистый D-Bus
//     поверх zbus, без новых системных зависимостей (никакого
//     libappindicator/GTK3-агента).
//   * Работает под Wayland/Niri (трей это D-Bus, не X).
//   * Blocking-API (feature `blocking`): спавн в фоновом потоке.
//
// Связь с главным GTK-потоком — через `std::sync::mpsc`: меню/клик трея слают
// `TrayCmd`, app.rs опрашивает канал по таймеру главного цикла и выполняет
// команды (show/hide/quit).
// Иконка: `keyboard-center` из hicolor (ставится флейком и fedora.sh).

use std::sync::mpsc::Sender;

use ksni::blocking::TrayMethods;
use ksni::{menu::StandardItem, MenuItem, Tray};

/// Команды от трея, исполняемые в главном GTK-потоке.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TrayCmd {
    /// Клик по иконке / пункт меню «Показать/Скрыть окно».
    ToggleWindow,
    /// Пункт меню «Выход» — реальный выход (с автосейвом).
    RealQuit,
    /// Watcher подключился — трей реально отображается.
    Available,
    /// Watcher офлайн (SNI-хоста нет) — деградация к обычному поведению.
    Unavailable,
}

struct TrayItem {
    tx: Sender<TrayCmd>,
}

impl Tray for TrayItem {
    fn id(&self) -> String {
        "keyboard-center".into()
    }

    fn title(&self) -> String {
        "Keyboard-Center".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }

    fn icon_name(&self) -> String {
        "keyboard-center".into()
    }

    /// Левый клик по иконке = показать/скрыть окно.
    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(TrayCmd::ToggleWindow);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        use ksni::menu::*;
        vec![
            StandardItem {
                label: "Показать/Скрыть окно".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.send(TrayCmd::ToggleWindow);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Выход".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.send(TrayCmd::RealQuit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }

    /// Watcher офлайн (SNI-хост пропал/отсутствует) — уведомляем главный
    /// поток для деградации, но сам сервис держим (хост может вернуться).
    fn watcher_offline(&self, _reason: ksni::OfflineReason) -> bool {
        let _ = self.tx.send(TrayCmd::Unavailable);
        true
    }

    fn watcher_online(&self) {
        let _ = self.tx.send(TrayCmd::Available);
    }
}

/// Запустить трей в фоновом потоке.
///
/// `tx` — канал в главный GTK-поток. Если SNI-хоста в системе нет, `spawn()`
/// вернёт Err — трей просто не появится, `Available` не придёт, и приложение
/// деградирует к прежнему поведению (крестик = выход).
pub fn spawn(tx: Sender<TrayCmd>) {
    std::thread::Builder::new()
        .name("keyboard-center-tray".into())
        .spawn(move || {
            let tray = TrayItem { tx: tx.clone() };
            // blocking::spawn сам блокируется до готовности: вернёт Ok только
            // когда SNI-watcher принял регистрацию. По результату уведомляем
            // главный поток — крестик ведёт себя по-разному (10.111).
            match tray
                .assume_sni_available(false)
                .disable_dbus_name(false)
                .spawn()
            {
                Ok(_) => {
                    let _ = tx.send(TrayCmd::Available);
                }
                Err(_) => {
                    let _ = tx.send(TrayCmd::Unavailable);
                }
            }
        })
        .expect("не удалось запустить поток трея");
}