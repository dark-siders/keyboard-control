// --- System & Power Settings ---
// Polling Rate, Debounce, Sleep Timer, уровень батареи.

use gtk::glib::clone;
use gtk::prelude::*;

use std::rc::Rc;

use crate::devices::KeyboardDevice;

/// Панель системных настроек.
pub fn build_settings_panel(
    device_fetcher: impl Fn() -> Option<Rc<dyn KeyboardDevice>> + 'static,
) -> gtk::Box {
    let fetcher: std::rc::Rc<dyn Fn() -> Option<Rc<dyn KeyboardDevice>>> = std::rc::Rc::new(device_fetcher);
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 6);
    panel.set_margin_top(8);

    let title = gtk::Label::new(Some("System & Power"));
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Start);
    panel.append(&title);

    // --- Батарея --------------------------------------------------------
    let battery_label = gtk::Label::new(Some("Батарея: —"));
    battery_label.add_css_class("title-4");
    battery_label.set_halign(gtk::Align::Start);
    panel.append(&battery_label);

    let refresh_btn = gtk::Button::with_label("Обновить статус");
    refresh_btn.set_halign(gtk::Align::Start);
    panel.append(&refresh_btn);

    // --- Polling Rate -----------------------------------------------------
    let poll_header = gtk::Label::new(Some("Частота опроса (Polling Rate)"));
    poll_header.add_css_class("heading");
    poll_header.set_halign(gtk::Align::Start);
    panel.append(&poll_header);

    let poll_dd = gtk::DropDown::builder().build();
    let poll_model = gtk::StringList::new(&["125 Гц", "250 Гц", "500 Гц", "1000 Гц"]);
    poll_dd.set_model(Some(&poll_model));
    poll_dd.set_selected(3); // 1000 Гц по умолчанию
    panel.append(&poll_dd);

    // --- Debounce ---------------------------------------------------------
    let debounce_header = gtk::Label::new(Some("Задержка от дребезга (Debounce)"));
    debounce_header.add_css_class("heading");
    debounce_header.set_halign(gtk::Align::Start);
    panel.append(&debounce_header);

    let debounce_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let debounce_spin = gtk::SpinButton::with_range(1.0, 40.0, 1.0);
    debounce_spin.set_value(4.0);
    debounce_spin.set_snap_to_ticks(true);
    let debounce_unit = gtk::Label::new(Some("мс"));
    debounce_unit.add_css_class("dim-label");
    debounce_row.append(&debounce_spin);
    debounce_row.append(&debounce_unit);
    panel.append(&debounce_row);

    // --- Sleep Timer ------------------------------------------------------
    let sleep_header = gtk::Label::new(Some("Таймер автовыключения (Sleep)"));
    sleep_header.add_css_class("heading");
    sleep_header.set_halign(gtk::Align::Start);
    panel.append(&sleep_header);

    let sleep_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let sleep_spin = gtk::SpinButton::with_range(0.0, 3600.0, 10.0);
    sleep_spin.set_value(600.0);
    let sleep_unit = gtk::Label::new(Some("сек (0 = выкл)"));
    sleep_unit.add_css_class("dim-label");
    sleep_row.append(&sleep_spin);
    sleep_row.append(&sleep_unit);
    panel.append(&sleep_row);

    // --- Применение ------------------------------------------------------
    let apply_btn = gtk::Button::with_label("Применить все настройки");
    apply_btn.add_css_class("suggested-action");
    panel.append(&apply_btn);

    let status = gtk::Label::new(Some(""));
    status.add_css_class("dim-label");
    status.set_halign(gtk::Align::Start);
    panel.append(&status);

    // --- Обработчики --------------------------------------------------------
    let fetcher2 = fetcher.clone();
    refresh_btn.connect_clicked(clone!(
        #[strong]
        battery_label,
        #[strong]
        status,
        move |_| {
        if let Some(dev) = fetcher2() {
            let mut levels = "нет".to_string();
            match dev.get_battery_level() {
                Ok(Some(l)) => {
                    levels = format!("{l}%");
                    battery_label.set_text(&format!("Батарея: {levels}"));
                }
                Ok(None) if dev.connection_type() == crate::types::ConnectionType::Wireless2_4G => {
                    battery_label.set_text("Батарея: нет ответа (клавиатура спит на линке)");
                    status.set_text("Нажмите клавишу на клавиатуре и обновите снова");
                    return;
                }
                _ => {
                    battery_label.set_text("Батарея: не поддерживается");
                }
            }
            status.set_text(&format!("Обновлено. Заряд: {}", levels));
        } else {
            status.set_text("Устройство не подключено");
        }
    }));

    let fetcher3 = fetcher.clone();
    apply_btn.connect_clicked(clone!(
        #[strong]
        poll_dd,
        #[strong]
        debounce_spin,
        #[strong]
        sleep_spin,
        #[strong]
        status,
        move |_| {
        let rate = match poll_dd.selected() {
            0 => 125u16,
            1 => 250u16,
            2 => 500u16,
            _ => 1000u16,
        };
        let debounce = debounce_spin.value() as u16;
        let sleep = sleep_spin.value() as u16;
        if let Some(dev) = fetcher3() {
            let mut errors = Vec::new();
            if let Err(e) = dev.set_polling_rate(rate) {
                errors.push(format!("polling: {e}"));
            }
            if let Err(e) = dev.set_debounce(debounce) {
                errors.push(format!("debounce: {e}"));
            }
            if let Err(e) = dev.set_sleep_timer(sleep) {
                errors.push(format!("sleep: {e}"));
            }
            if errors.is_empty() {
                status.set_text("Настройки применены");
            } else {
                status.set_text(&format!("Ошибки: {}", errors.join(", ")));
            }
        } else {
            status.set_text("Устройство не подключено");
        }
    }));

    panel
}