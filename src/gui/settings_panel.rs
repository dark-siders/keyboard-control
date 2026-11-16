// --- System & Power Settings ---
// Polling Rate, Debounce, Sleep Timer, уровень батареи.

use gtk::glib::clone;
use gtk::prelude::*;

use std::cell::Cell;
use std::rc::Rc;

use crate::devices::KeyboardDevice;

/// Панель системных настроек.
pub fn build_settings_panel(
    device_fetcher: impl Fn() -> Option<Rc<dyn KeyboardDevice>> + 'static,
) -> gtk::Box {
    let fetcher: std::rc::Rc<dyn Fn() -> Option<Rc<dyn KeyboardDevice>>> = std::rc::Rc::new(device_fetcher);
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 6);
    panel.set_margin_top(8);

    let title = gtk::Label::new(Some("Настройки"));
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

    // --- Частота опроса ---------------------------------------------------
    let poll_header = gtk::Label::new(Some("Частота опроса"));
    poll_header.add_css_class("heading");
    poll_header.set_halign(gtk::Align::Start);
    panel.append(&poll_header);

    let poll_dd = gtk::DropDown::builder().build();
    let poll_model = gtk::StringList::new(&["125 Гц", "250 Гц", "500 Гц", "1000 Гц"]);
    poll_dd.set_model(Some(&poll_model));
    poll_dd.set_selected(3); // 1000 Гц по умолчанию
    panel.append(&poll_dd);

    // --- Задержка нажатий -------------------------------------------------
    let debounce_header = gtk::Label::new(Some("Задержка нажатий (антидребезг)"));
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

    // Флаг: показывать ли Polling/Debounce (F99 не поддерживает)
    let show_sys_controls: Rc<Cell<bool>> = Rc::new(Cell::new(true));

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
    let show_sys_controls_r = show_sys_controls.clone();
    let poll_header_c = poll_header.clone();
    let poll_dd_c = poll_dd.clone();
    let debounce_header_c = debounce_header.clone();
    let debounce_row_c = debounce_row.clone();
    refresh_btn.connect_clicked(clone!(
        #[strong]
        battery_label,
        #[strong]
        status,
        #[strong]
        show_sys_controls_r,
        #[strong]
        poll_header_c,
        #[strong]
        poll_dd_c,
        #[strong]
        debounce_header_c,
        #[strong]
        debounce_row_c,
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
                Ok(None) if dev.connection_type() == crate::types::ConnectionType::Bluetooth => {
                    battery_label.set_text("Батарея: не поддерживается");
                    status.set_text("По Bluetooth заряд не читается (BT-режим закрыт)");
                    return;
                }
                _ => {
                    battery_label.set_text("Батарея: не поддерживается");
                    if dev.connection_type() == crate::types::ConnectionType::Usb {
                        status.set_text(
                            "По USB заряд не читается (проводной канал без данных о заряде). \
                             Переключите клавиатуру на 2.4G, чтобы увидеть процент.",
                        );
                        return;
                    }
                }
            }
            // Скрываем Polling/Debounce для F99 (не поддерживает)
            let is_f99 = dev.name().contains("F99");
            show_sys_controls_r.set(!is_f99);
            poll_header_c.set_visible(!is_f99);
            poll_dd_c.set_visible(!is_f99);
            debounce_header_c.set_visible(!is_f99);
            debounce_row_c.set_visible(!is_f99);
            if is_f99 {
                status.set_text("F99: Polling/Debounce не поддерживаются прошивкой");
            } else {
                status.set_text(&format!("Обновлено. Заряд: {}", levels));
            }
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
        status,
        move |_| {
        let rate = match poll_dd.selected() {
            0 => 125u16,
            1 => 250u16,
            2 => 500u16,
            _ => 1000u16,
        };
        let debounce = debounce_spin.value() as u16;
        if let Some(dev) = fetcher3() {
            let mut errors = Vec::new();
            let is_f99 = dev.name().contains("F99");
            if !is_f99 {
                if let Err(e) = dev.set_polling_rate(rate) {
                    errors.push(format!("polling: {e}"));
                }
                if let Err(e) = dev.set_debounce(debounce) {
                    errors.push(format!("debounce: {e}"));
                }
            }
            if errors.is_empty() {
                if is_f99 {
                    status.set_text("F99: Polling/Debounce/Sleep не поддерживаются, пропущено");
                } else {
                    status.set_text("Настройки применены");
                }
            } else {
                status.set_text(&format!("Ошибки: {}", errors.join(", ")));
            }
        } else {
            status.set_text("Устройство не подключено");
        }
    }));

    panel
}
/// Страница «Таймаут режима сна» (отдельный пункт сайдбара).
/// Значение применяется автоматически через 300 мс после последнего
/// изменения (без кнопки).
pub fn build_sleep_page(
    device_fetcher: impl Fn() -> Option<Rc<dyn KeyboardDevice>> + 'static,
) -> gtk::Box {
    let fetcher: Rc<dyn Fn() -> Option<Rc<dyn KeyboardDevice>>> = Rc::new(device_fetcher);
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 6);
    panel.set_margin_top(8);

    let title = gtk::Label::new(Some("Таймаут режима сна"));
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Start);
    panel.append(&title);

    let hint = gtk::Label::new(Some(
        "Через какое время бездействия клавиатура уходит в сон (гаснет подсветка, \
         разрывается беспроводная связь). Прошивка поддерживает ровно эти ступени. \
         Значение применяется само после выбора — или нажмите «Применить».",
    ));
    hint.add_css_class("dim-label");
    hint.set_wrap(true);
    hint.set_halign(gtk::Align::Start);
    panel.append(&hint);

    // Ступени прошивки: (секунды для драйвера, подпись).
    const SLEEP_STEPS: &[(u16, &str)] = &[
        (0, "Не засыпать"),
        (60, "1 минута"),
        (300, "5 минут"),
        (1800, "30 минут"),
    ];

    let dd = gtk::DropDown::builder().halign(gtk::Align::Start).build();
    {
        let names: Vec<&str> = SLEEP_STEPS.iter().map(|(_, n)| *n).collect();
        dd.set_model(Some(&gtk::StringList::new(&names)));
        dd.set_selected(2); // «5 минут» по умолчанию
    }
    panel.append(&dd);

    let apply_btn = gtk::Button::with_label("Применить");
    apply_btn.add_css_class("suggested-action");
    apply_btn.set_halign(gtk::Align::Start);
    panel.append(&apply_btn);

    let status = gtk::Label::new(Some("Подключите клавиатуру на вкладке «Устройства»."));
    status.add_css_class("dim-label");
    status.set_halign(gtk::Align::Start);
    status.set_wrap(true);
    panel.append(&status);

    // Общая логика применения (кнопка «Применить» и авто-применение).
    let do_apply: Rc<dyn Fn()> = {
        let dd_c = dd.clone();
        let status_c = status.clone();
        let fetcher_c = fetcher.clone();
        Rc::new(move || {
            let seconds = SLEEP_STEPS
                .get(dd_c.selected() as usize)
                .map(|(s, _)| *s)
                .unwrap_or(300);
            let label = SLEEP_STEPS
                .iter()
                .find(|(s, _)| *s == seconds)
                .map(|(_, n)| *n)
                .unwrap_or("");
            let Some(dev) = fetcher_c() else {
                status_c.set_text("Устройство не подключено: вкладка «Устройства» → «Подключить»");
                return;
            };
            match dev.set_sleep_timer(seconds) {
                Ok(()) => status_c.set_text(&format!("Таймаут сна: {label} — применено")),
                Err(e) => status_c.set_text(&format!("Не удалось применить: {e}")),
            }
        })
    };
    {
        let do_apply_btn = do_apply.clone();
        apply_btn.connect_clicked(move |_| do_apply_btn());
    }
    // Авто-применение: выбор в списке сразу отправляется на клавиатуру.
    {
        let do_apply_dd = do_apply.clone();
        dd.connect_selected_notify(move |_| do_apply_dd());
    }

    panel
}
