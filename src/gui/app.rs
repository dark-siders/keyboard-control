// --- Главное приложение Keyboard-Center (GUI) ---
// AdwApplication + главное окно с сайдбаром и стеком страниц.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};

use adw::prelude::*;
use gtk::glib;
use gtk::glib::clone;

use crate::devices::scanner::FoundDevice;
use crate::devices::{KeyboardDevice, Layout};
use crate::VERSION;

use super::{keymap_editor, rgb_studio, screen_manager, settings_panel, tray};

/// Текущее устройство в приложении.
type DeviceSlot = Rc<RefCell<Option<Rc<dyn KeyboardDevice>>>>;

const APP_ID: &str = "org.darkside.keyboard-center";

/// Запуск GUI.
pub fn run_gtk() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();

    // Иконка приложения (обязательна для окон и прочих мест, где GTK
    // запрашивает app icon). Имя совпадает с .desktop `Icon=keyboard-center`
    // и файлом data/keyboard-center.svg, уложенным в hicolor через flake.
    // ВАЖНО: вызывается в активации (GTK к этому моменту уже инициализирован
    // внутри app.run()); статический вызов ДО run падает «GTK has not been
    // initialized» (gtk-window.rs assert).
    app.connect_activate(move |app| {
        gtk::Window::set_default_icon_name("keyboard-center");
        build_ui(app);
    });
    app.run()
}

/// Построение интерфейса.
fn build_ui(app: &adw::Application) {
    let win = adw::ApplicationWindow::builder()
        .application(app)
        .title("Keyboard-Center")
        .default_width(1100)
        .default_height(760)
        .build();

    let toolbar = adw::ToolbarView::new();
    let header = build_header_bar();
    toolbar.add_top_bar(&header);

    // --- Контент: сайдбар + стек страниц -------------------------------
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);

    // Сайдбар навигации
    let stack = gtk::Stack::builder()
        .vexpand(true)
        .hexpand(true)
        .transition_type(gtk::StackTransitionType::Crossfade)
        .build();

    // --- Общее хранилище устройства --------------------------------------
    let device_slot: DeviceSlot = Rc::new(RefCell::new(None));

    // Страница «Устройства»
    let device_page = build_device_page(device_slot.clone(), stack.clone());
    stack.add_titled(&device_page, Some("devices"), "Устройства");

    // Страницы функций
    let (rgb_page, persist_probe) = rgb_studio::build_rgb_studio(
        make_fetcher(device_slot.clone()),
        make_set_device(device_slot.clone()),
        make_last_vidpid(),
    );
    let km_page = keymap_editor::build_keymap_editor(make_fetcher(device_slot.clone()));
    let screen_page = screen_manager::build_screen_manager(make_fetcher(device_slot.clone()));
    let settings_page = settings_panel::build_settings_panel(make_fetcher(device_slot.clone()));

    stack.add_titled(&rgb_page, Some("rgb"), "RGB & Light Studio");
    stack.add_titled(&km_page, Some("engine"), "Keymap & Fn");
    stack.add_titled(&screen_page, Some("screen"), "Screen Manager");
    stack.add_titled(&settings_page, Some("settings"), "Настройки");

    // Сайдбар
    let sidebar = build_sidebar(&stack);
    root.append(&sidebar);
    root.append(&stack);

    toolbar.set_content(Some(&root));
    win.set_content(Some(&toolbar));

    // --- Трей (SNI, журнал 10.111) ----------------------------------------
    // Сворачивание в системный трей через ksni. Поведение close-request:
    //   * трей онлайн — крестик прячет окно (hide), эффекты продолжают идти;
    //   * трей офлайн/нет хоста — прежний автосейв + закрытие;
    //   * реальный выход — только из меню трея / header «Выход» (RealQuit).
    // Отключить трей можно переменной KEYBOARD_CENTER_NO_TRAY (непустая).
    let (tray_tx, tray_rx) = std::sync::mpsc::channel::<tray::TrayCmd>();
    let tray_online: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
    let real_quit: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));

    if std::env::var_os("KEYBOARD_CENTER_NO_TRAY").is_none() {
        tray::spawn(tray_tx);
        // Приём команд из D-Bus-потока трея в главном GTK-цикле (mpsc опрос).
        let win_c = win.clone();
        let online = tray_online.clone();
        let quit_flag = real_quit.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
            while let Ok(cmd) = tray_rx.try_recv() {
                match cmd {
                    tray::TrayCmd::ToggleWindow => {
                        if win_c.is_visible() {
                            win_c.set_visible(false);
                        } else {
                            win_c.present();
                        }
                    }
                    tray::TrayCmd::RealQuit => {
                        *quit_flag.borrow_mut() = true;
                        win_c.close();
                    }
                    tray::TrayCmd::Available => *online.borrow_mut() = true,
                    tray::TrayCmd::Unavailable => *online.borrow_mut() = false,
                }
            }
            glib::ControlFlow::Continue
        });
    }

    // Явное действие «Выход» (меню header ссылается на "app.quit"):
    // реальное закрытие с автосейвом, минуя сворачивание в трей.
    let quit_act = gio::SimpleAction::new("quit", None);
    {
        let win_c = win.clone();
        let quit_flag = real_quit.clone();
        quit_act.connect_activate(move |_, _| {
            *quit_flag.borrow_mut() = true;
            win_c.close();
        });
    }
    app.add_action(&quit_act);

    // Автосохранение при закрытии: докинуть последнее состояние (свет выкл /
    // выбранный эффект) в память клавиатуры, чтобы оно держалось без приложения.
    setup_close_autosave(
        &win,
        persist_probe,
        make_last_vidpid(),
        tray_online,
        real_quit,
    );
    win.present();
}

/// Автосохранение состояния подсветки при закрытии окна.
///
/// Проблема (жалоба владельца): «Выключить свет» гасило LED, но пользователь
/// закрывал приложение — запись во флеш либо не успевала завершиться в фоновом
/// потоке, либо вообще не дожималась кнопкой «Записать в память», и после
/// отключения/переподключения клавиатуры свет возвращался. Решение: `close-request`
/// перед закрытием сам спрашивает probe (rgb_studio::PersistProbe) и, если есть
/// что зафиксировать, запускает фоновую запись в память и НЕ закрывает окно,
/// пока она не завершится (потом закрывает сам).
fn setup_close_autosave(
    win: &adw::ApplicationWindow,
    probe: Rc<dyn Fn() -> Option<rgb_studio::SaveKind>>,
    last_vidpid: impl Fn() -> Option<(u16, u16)> + 'static,
    tray_online: Rc<RefCell<bool>>,
    real_quit: Rc<RefCell<bool>>,
) {
    // Защита от зацикливания: close-request срабатывает повторно, когда мы сами
    // зовём win.close() после завершения записи.
    let saving = Rc::new(RefCell::new(false));

    // Канал из фонового потока записи (опрашивается по таймеру, UI не блокирует).
    let result_rx: Rc<RefCell<Option<std::sync::mpsc::Receiver<Result<(), String>>>>> =
        Rc::new(RefCell::new(None));

    // last_vidpid — не-Clone closure, а он нужен в move-замыкании close-request
    // и в потоке записи; оборачиваем в Rc<dyn Fn>.
    let last: Rc<dyn Fn() -> Option<(u16, u16)>> = Rc::new(last_vidpid);

    win.connect_close_request(move |win| {
        // Уже сохраняли по этому закрытию — разрешаем окну закрыться.
        if *saving.borrow() {
            return glib::Propagation::Proceed;
        }

        // Трей активен и это не реальный выход — сворачиваем в трей (10.111).
        // Запись в память не делаем: host-эффекты продолжают идти, окно лишь
        // спрятано. Реальный выход — из меню трея / header «Выход».
        if *tray_online.borrow() && !*real_quit.borrow() {
            win.set_visible(false);
            return glib::Propagation::Stop;
        }

        // Нечего сохранять (свет горит и эффект не выбран) — просто закрываем.
        let Some(action) = probe() else {
            return glib::Propagation::Proceed;
        };
        // Клавиатура не была открыта — нечего и во что писать.
        let Some((vid, pid)) = last() else {
            return glib::Propagation::Proceed;
        };

        *saving.borrow_mut() = true;

        // Запись во флеш содержит блокирующие hidapi-вызовы (read_config +
        // write_config + кик клавиатуры) — выполняем в отдельном потоке на
        // независимом handle (open_vidpid), чтобы не вешать закрытие/UI.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let res: Result<(), String> = (|| {
                let dev = crate::devices::open_vidpid(vid, pid)
                    .map_err(|e| format!("не удалось открыть устройство: {e}"))?;
                match action {
                    rgb_studio::SaveKind::Effect(code) => {
                        dev.save_hardware_effect(code).map_err(|e| e.to_string())?
                    }
                    rgb_studio::SaveKind::LightOff => {
                        dev.save_hardware_light(false).map_err(|e| e.to_string())?
                    }
                    rgb_studio::SaveKind::Config => {
                        dev.save_profile_to_eeprom(0).map_err(|e| e.to_string())?
                    }
                }
                Ok(())
            })();
            let _ = tx.send(res);
        });
        *result_rx.borrow_mut() = Some(rx);

        // Откладываем закрытие: ждём завершения записи, не блокируя главный
        // поток (опрашиваем канал короткими паузами). Страховка от зависания:
        // после 200 попыток (~30 с) принудительно закрываем окно.
        let win_c = win.clone();
        let rx_guard = result_rx.clone();
        let attempts = Rc::new(Cell::new(0u32));
        glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
            if attempts.get() >= 200 {
                *rx_guard.borrow_mut() = None;
                win_c.close();
                return glib::ControlFlow::Break;
            }
            attempts.set(attempts.get() + 1);
            let done = {
                let b = rx_guard.borrow();
                b.as_ref().map(|rx| rx.try_recv())
            };
            match done {
                Some(Ok(Ok(()))) | Some(Ok(Err(_))) | Some(Err(_)) => {
                    *rx_guard.borrow_mut() = None;
                    win_c.close();
                    glib::ControlFlow::Break
                }
                _ => glib::ControlFlow::Continue,
            }
        });

        // Останавливаем закрытие — вернёмся в него сами после записи.
        glib::Propagation::Stop
    });
}

/// Сайдбар с навигацией. Переключает страницы стека.
fn build_sidebar(stack: &gtk::Stack) -> gtk::Box {
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 4);
    sidebar.set_width_request(200);
    sidebar.add_css_class("sidebar");
    sidebar.set_margin_top(12);
    sidebar.set_margin_bottom(12);
    sidebar.set_margin_start(8);
    sidebar.set_margin_end(8);

    let title = gtk::Label::new(Some("Keyboard-Center"));
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Start);
    sidebar.append(&title);

    let version = gtk::Label::new(Some(&format!("v{VERSION}")));
    version.add_css_class("dim-label");
    version.set_halign(gtk::Align::Start);
    sidebar.append(&version);

    let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    sidebar.append(&sep);

    let pages: Vec<(&str, &str)> = vec![
        ("devices", "Устройства"),
        ("rgb", "RGB & Light Studio"),
        ("engine", "Keymap & Fn"),
        ("screen", "Screen Manager"),
        ("settings", "Настройки"),
    ];

    for (name, label) in pages {
        let btn = gtk::Button::with_label(label);
        btn.set_vexpand(false);
        btn.set_halign(gtk::Align::Fill);
        btn.add_css_class("flat");
        let stack2 = stack.clone();
        btn.connect_clicked(move |_| {
            stack2.set_visible_child_name(name);
        });
        sidebar.append(&btn);
    }

    sidebar
}

/// Верхняя панель.
fn build_header_bar() -> gtk::HeaderBar {
    let header = gtk::HeaderBar::builder().build();

    let menu = gio::Menu::new();
    let section = gio::Menu::new();
    section.append(Some("_Выход"), Some("app.quit"));
    menu.append_section(None, &section);

    let menu_btn = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .build();
    header.pack_end(&menu_btn);

    header
}

/// Страница «Устройства».
fn build_device_page(device_slot: DeviceSlot, stack: gtk::Stack) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 6);
    page.set_margin_top(8);
    page.set_margin_bottom(8);
    page.set_margin_start(8);
    page.set_margin_end(8);

    let title = gtk::Label::new(Some("Устройства"));
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Start);
    page.append(&title);

    let hint = gtk::Label::new(Some(
        "Нажмите «Открыть», чтобы управлять клавиатурой. Устройства определяются автоматически.",
    ));
    hint.add_css_class("dim-label");
    hint.set_wrap(true);
    hint.set_halign(gtk::Align::Start);
    page.append(&hint);

    // Список устройств
    let list: gtk::ListBox = gtk::ListBox::builder().margin_top(4).build();
    page.append(&list);

    // Кнопка открытия
    let connect_btn = gtk::Button::with_label("Сканировать устройства");
    connect_btn.add_css_class("suggested-action");
    connect_btn.set_halign(gtk::Align::Start);
    page.append(&connect_btn);

    // Панель подключённого устройства
    let connected_panel = gtk::Box::new(gtk::Orientation::Vertical, 6);
    connected_panel.set_margin_top(12);
    page.append(&connected_panel);

    // Статус
    let status = gtk::Label::new(Some(""));
    status.add_css_class("dim-label");
    status.set_halign(gtk::Align::Start);
    page.append(&status);

    // Обработчики
    connect_btn.connect_clicked(clone!(
        #[strong]
        list,
        #[strong]
        connected_panel,
        #[strong]
        status,
        #[strong]
        stack,
        #[strong]
        device_slot,
        move |_| {
        // Очистка списка
        remove_all_children_list(&list);
        remove_all_children_box(&connected_panel);

        // Сканирование
        let (tx, _rx) = std::sync::mpsc::channel();
        let scanner = match crate::devices::scanner::Scanner::new(tx) {
            Ok(s) => s,
            Err(_) => {
                status.set_text("Не удалось инициализировать HID API");
                return;
            }
        };
        let found = match scanner.scan() {
            Ok(f) => f,
            Err(_) => {
                status.set_text("Ошибка сканирования");
                return;
            }
        };

        if found.is_empty() {
            status.set_text("Устройства не найдены. Проверьте udev-правила.");
            return;
        }

        status.set_text(&format!("Найдено устройств: {}", found.len()));
        for dev in found {
            let row = build_device_row(&dev, &device_slot, &stack, &status);
            list.append(&row);
        }
    }));

    page
}

/// Строка устройства в списке.
fn build_device_row(
    dev: &FoundDevice,
    device_slot: &DeviceSlot,
    stack: &gtk::Stack,
    status: &gtk::Label,
) -> gtk::Box {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();

    let glyph = gtk::Label::new(Some("⌨"));
    glyph.add_css_class("title-2");
    glyph.set_margin_start(6);
    glyph.set_margin_end(6);

    let info_col = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .halign(gtk::Align::Start)
        .build();

    let model_txt = dev
        .model
        .as_deref()
        .unwrap_or(dev.info.model);
    let name = gtk::Label::new(Some(&format!(
        "{} — {}",
        model_txt,
        dev.info.connection.label()
    )));
    name.set_halign(gtk::Align::Start);
    name.add_css_class("body");

    let layout_txt = match dev.info.layout {
        Layout::L75 => "75%",
        Layout::L96 => "96%",
    };
    let detail = gtk::Label::new(Some(&format!(
        "Раскладка: {layout_txt} | VID:PID {:04X}:{:04X} | {:?}",
        dev.info.vid, dev.info.pid, dev.path
    )));
    detail.set_halign(gtk::Align::Start);
    detail.add_css_class("caption");
    detail.add_css_class("dim-label");

    info_col.append(&name);
    info_col.append(&detail);

    let btn = gtk::Button::with_label("Открыть");
    btn.add_css_class("suggested-action");
    btn.set_margin_end(6);

    let dev_data = dev.info.clone();
    let paths = dev.path.clone();
    let slot = device_slot.clone();
    let stack2 = stack.clone();
    let status2 = status.clone();
    btn.connect_clicked(move |_| {
        match connect_selected(&slot, &dev_data, paths.as_deref()) {
            Ok(name) => {
                status2.set_text(&format!("Подключено: {name}"));
                // Открываем RGB-страницу
                stack2.set_visible_child_name("rgb");
            }
            Err(e) => {
                status2.set_text(&format!("Не удалось открыть устройство: {e}"));
            }
        }
    });

    row.append(&glyph);
    row.append(&info_col);
    row.append(&btn);
    row
}

/// Удалить всех детей из Box.
fn remove_all_children_box(widget: &gtk::Box) {
    while let Some(child) = widget.first_child() {
        widget.remove(&child);
    }
}

/// Удалить всех детей из ListBox.
fn remove_all_children_list(widget: &gtk::ListBox) {
    while let Some(child) = widget.first_child() {
        widget.remove(&child);
    }
}

/// Обёртка-фетчер для панелей: возвращает подключённое устройство.
fn make_fetcher(slot: DeviceSlot) -> impl Fn() -> Option<Rc<dyn KeyboardDevice>> {
    move || slot.borrow().clone()
}

/// Открытие устройства по полям из найденной строки.
/// Путь из списка игнорируется: HidBridge::open сам выбирает командный
/// интерфейс с вендорским usage page (у AULA это hidraw6), а не клавиатурный.
fn connect_selected(
    device_slot: &DeviceSlot,
    info: &crate::devices::DeviceInfo,
    _path: Option<&str>,
) -> anyhow::Result<String> {
    open_device(device_slot, info.vid, info.pid)
}

/// Последний открытый VID/PID — для переоткрытия после кика (когда write
/// заставляет клавиатуру пересоздать hidraw-узлы, старый handle умирает).
fn last_dev_slot() -> &'static Mutex<Option<(u16, u16)>> {
    static SLOT: OnceLock<Mutex<Option<(u16, u16)>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// Открыть устройство по VID/PID напрямую и положить в слот.
fn open_device(device_slot: &DeviceSlot, vid: u16, pid: u16) -> anyhow::Result<String> {
    let opened = crate::devices::open_vidpid(vid, pid)
        .map_err(|e| anyhow::anyhow!("ошибка открытия HID-устройства: {e}"))?;

    let name = opened.name().to_string();
    let opened: Rc<dyn KeyboardDevice> = Rc::from(opened);
    *device_slot.borrow_mut() = Some(opened);
    *last_dev_slot().lock().unwrap() = Some((vid, pid));
    Ok(name)
}

/// Последний открытый VID/PID (для фоновой записи в RGB-студии).
fn make_last_vidpid() -> impl Fn() -> Option<(u16, u16)> {
    move || *last_dev_slot().lock().unwrap()
}

/// Положить готовое устройство в слот (переподключение после кика).
/// Вызывается в главном GTK-потоке: hidraw переоткрывает фоновый поток
/// (спавен в rgb_studio), а сюда кладётся уже собранный `Box<dyn
/// KeyboardDevice>` — без блокирующих HID-вызовов (фикс 2026-09-13).
fn make_set_device(device_slot: DeviceSlot) -> impl Fn(Box<dyn KeyboardDevice>) -> String {
    move |dev| {
        let name = dev.name().to_string();
        let dev: Rc<dyn KeyboardDevice> = Rc::from(dev);
        *device_slot.borrow_mut() = Some(dev);
        name
    }
}