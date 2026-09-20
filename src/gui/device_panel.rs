// --- Панель выбора устройства ---
// Список найденных клавиатур, модель, раскладка, подключение.

use gtk::prelude::*;

use crate::devices::scanner::FoundDevice;
use crate::devices::{ConnectionType, Layout};

/// Готовая строка в списке устройств.
pub struct DeviceRow {
    pub box_: gtk::Box,
    pub name_label: gtk::Label,
    pub status_label: gtk::Label,
    pub battery_label: gtk::Label,
    pub layout_label: gtk::Label,
}

impl DeviceRow {
    /// Названия моделей в форме «model (mode)».
    /// `detected` — реальная модель из драйвера (косметика F75/F99, 10.78),
    /// если её удалось определить при сканировании (иначе карточка реестра).
    pub fn title(info: &crate::devices::DeviceInfo, detected: Option<&str>) -> String {
        format!("{} — {}", detected.unwrap_or(info.model), info.connection.label())
    }

    pub fn layout_name(layout: Layout) -> &'static str {
        match layout {
            Layout::L75 => "75%",
            Layout::L96 => "96%",
        }
    }
}

/// Найти layout по (vid, pid).
pub fn layout_for(vid: u16, pid: u16) -> Option<Layout> {
    crate::devices::scanner::Scanner::layout_of(vid, pid)
}

/// Каталог: кнопка «Подключить» + строка с инфо.
/// Connections не делаем здесь — чтобы не плодить временные Self-ссылки,
/// панель просто вызывает колбэк на выбор устройства.
pub fn build_device_list_box(devices: &[FoundDevice], on_select: impl Fn(&FoundDevice) + 'static) -> gtk::ListBox {
    let on_select = std::rc::Rc::new(on_select);
    let list = gtk::ListBox::builder()
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();

    for dev in devices {
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

        let name = gtk::Label::new(Some(&DeviceRow::title(dev.info, dev.model.as_deref())));
        name.set_halign(gtk::Align::Start);
        name.add_css_class("body");

        let layout = gtk::Label::new(Some(&format!(
            "Раскладка: {} | VID:PID {:04X}:{:04X} | path: {:?}",
            DeviceRow::layout_name(dev.info.layout),
            dev.info.vid,
            dev.info.pid,
            dev.path
        )));
        layout.set_halign(gtk::Align::Start);
        layout.add_css_class("caption");
        layout.add_css_class("dim-label");

        info_col.append(&name);
        info_col.append(&layout);

        let btn = gtk::Button::with_label("Открыть");
        btn.add_css_class("suggested-action");
        btn.set_margin_end(6);
        let dev_clone = dev.clone(); // клавиша не требует Send — клонируем данные
        let cb = on_select.clone();
        btn.connect_clicked(move |_| cb(&dev_clone));

        row.append(&glyph);
        row.append(&info_col);
        row.append(&btn);
        list.append(&row);
    }
    list
}

/// Панель «Подключено сейчас»: блок с батареей и параметрами.
pub fn build_connected_panel(
    name: &str,
    layout: Layout,
    connection: ConnectionType,
) -> gtk::Box {
    let panel = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .margin_top(8)
        .margin_bottom(8)
        .build();

    let header = gtk::Label::new(Some("Подключено"));
    header.add_css_class("heading");
    header.set_halign(gtk::Align::Start);
    panel.append(&header);

    let name_l = gtk::Label::new(Some(name));
    name_l.set_halign(gtk::Align::Start);
    name_l.add_css_class("title-3");
    panel.append(&name_l);

let info = gtk::Label::new(Some(&format!(
        "Раскладка: {} • Подключение: {}",
        DeviceRow::layout_name(layout),
        connection.label()
    )));
    info.set_halign(gtk::Align::Start);
    info.add_css_class("dim-label");
    panel.append(&info);

    panel
}