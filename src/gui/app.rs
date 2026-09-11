// --- Главное приложение Keyboard-Center (GUI) ---
// AdwApplication + главное окно с сайдбаром и стеком страниц.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use gtk::glib::clone;

use crate::devices::scanner::FoundDevice;
use crate::devices::{KeyboardDevice, Layout};
use crate::VERSION;

use super::{keymap_editor, rgb_studio, screen_manager, settings_panel};

/// Текущее устройство в приложении.
type DeviceSlot = Rc<RefCell<Option<Rc<dyn KeyboardDevice>>>>;

const APP_ID: &str = "org.darkside.keyboard-center";

/// Запуск GUI.
pub fn run_gtk() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(build_ui);
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
    let rgb_page = rgb_studio::build_rgb_studio(make_fetcher(device_slot.clone()));
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
    win.present();
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

    let name = gtk::Label::new(Some(&format!(
        "{} — {}",
        dev.info.model,
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
fn connect_selected(
    device_slot: &DeviceSlot,
    info: &crate::devices::DeviceInfo,
    path: Option<&str>,
) -> anyhow::Result<String> {
    let api = hidapi::HidApi::new()?;
    let device = match path {
        Some(p) => {
            let cstr = std::ffi::CString::new(p)?;
            api.open_path(&cstr)
        }
        None => api.open(info.vid, info.pid),
    };
    let device = device.map_err(|_| anyhow::anyhow!("ошибка открытия HID-устройства"))?;
    let bridge = crate::devices::hid::HidBridge {
        device,
        vid: info.vid,
        pid: info.pid,
        connection: info.connection,
    };

    let opened: Box<dyn KeyboardDevice> = match (info.vid, info.pid) {
        (0x0328, _) => Box::new(crate::devices::ajazz_ak820::AjazzAk820::open(bridge)),
        (0x258A, 0x004B..=0x004D | 0x010C) => Box::new(crate::devices::aula_f75::AulaF75::open(bridge)),
        (0x258A, 0x004E..=0x0050) => Box::new(crate::devices::aula_f99::AulaF99::open(bridge)),
        _ => return Err(anyhow::anyhow!("неподдерживаемое устройство")),
    };
    let name = opened.name().to_string();
    let opened: Rc<dyn KeyboardDevice> = Rc::from(opened);
    *device_slot.borrow_mut() = Some(opened);
    Ok(name)
}