// --- Keymap & Fn-Layer Editor ---
// Переназначение клавиш, слои Fn, макросы.

use gtk::glib::clone;
use gtk::prelude::*;

use std::rc::Rc;

use crate::devices::KeyboardDevice;
use crate::types::{ConnectionType, KeyAction};

/// Собрать панель редактора клавиш.
pub fn build_keymap_editor(
    device_fetcher: impl Fn() -> Option<Rc<dyn KeyboardDevice>> + 'static,
) -> gtk::Box {
    let fetcher: Rc<dyn Fn() -> Option<Rc<dyn KeyboardDevice>>> = Rc::new(device_fetcher);
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 6);
    panel.set_margin_top(8);

    let title = gtk::Label::new(Some("Keymap & Fn-Layer Editor"));
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Start);
    panel.append(&title);

    let hint = gtk::Label::new(Some("Переназначение клавиш выполняется через CLI: `kbc remap <VID:PID> --list` (показать карту), `--key <слот> --to <usage>` (клавиша) или `--media <usage>` (кнопка мультимедиа). Кнопка ниже показывает текущую раскладку Default и FN1 — с меткой физической клавиши для каждого слота."));
    hint.add_css_class("dim-label");
    hint.set_wrap(true);
    hint.set_halign(gtk::Align::Start);
    panel.append(&hint);

    // --- Редактор клавиш (основной + Fn) -------------------------------
    let mode_sw = gtk::Switch::builder().valign(gtk::Align::Center).build();
    let mode_sw_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let mode_lbl = gtk::Label::new(Some("Редактировать Fn-слой"));
    mode_lbl.set_halign(gtk::Align::Start);
    mode_sw_row.append(&mode_lbl);
    mode_lbl.set_hexpand(true);
    mode_sw_row.append(&mode_sw);
    panel.append(&mode_sw_row);

    // Область макета (переиспользуем LayoutWidget)
    let (layout_area, _colors) = crate::gui::keyboard_layout::LayoutWidget::new_drawing_area(
        crate::gui::keyboard_layout::Layout::for_layout(crate::devices::Layout::L75),
    );
    layout_area.set_height_request(280);
    panel.append(&layout_area);

    // --- Назначенные действия ------------------------------------------
    let actions_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    actions_box.set_vexpand(true);
    let actions_header = gtk::Label::new(Some("Текущая раскладка"));
    actions_header.add_css_class("heading");
    actions_header.set_halign(gtk::Align::Start);
    actions_box.append(&actions_header);

    // Список раскладки прокручивается: при большом слое (F75 — 127 слотов
    // Default + FN1) контент не должен уходить за край окна.
    let list_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .max_content_height(320)
        .build();
    let list = gtk::ListBox::builder().margin_top(4).build();
    list_scroll.set_child(Some(&list));
    actions_box.append(&list_scroll);
    panel.append(&actions_box);

    // --- Макросы ---------------------------------------------------------
    let macro_header = gtk::Label::new(Some("Макросы"));
    macro_header.add_css_class("heading");
    macro_header.set_halign(gtk::Align::Start);
    panel.append(&macro_header);

    let macro_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let macro_id_spin = gtk::SpinButton::with_range(1.0, 12.0, 1.0);
    macro_id_spin.set_value(1.0);
    let rec_btn = gtk::Button::with_label("Записать");
    let stop_btn = gtk::Button::with_label("Стоп");
    let macro_row2 = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    macro_row2.append(&macro_id_spin);
    macro_row2.append(&rec_btn);
    macro_row2.append(&stop_btn);
    panel.append(&macro_row2);
    let _ = macro_row;

    let remap_apply = gtk::Button::with_label("Показать раскладку");
    remap_apply.add_css_class("suggested-action");
    panel.append(&remap_apply);

    let clear_all = gtk::Button::with_label("Сбросить всё");
    panel.append(&clear_all);

    // --- Обработчики --------------------------------------------------------
    // Показ карты клавиш: читаем текущую раскладку (Default + FN1) и
    // выводим в списке слоты с меткой физической клавиши (карта F75).
    // Переназначение — через CLI `kbc remap` (GUI-клик-редактор — TODO).
    let fetcher2 = fetcher.clone();
    remap_apply.connect_clicked(clone!(
        #[strong]
        list,
        move |_| {
        let Some(dev) = fetcher2() else { return; };
        // Очищаем старый список
        while let Some(row) = list.first_child() {
            list.remove(&row);
        }
        let add = |text: String| {
            let lbl = gtk::Label::new(Some(&text));
            lbl.set_halign(gtk::Align::Start);
            lbl.set_xalign(0.0);
            list.append(&lbl);
        };
        match dev.get_current_keymap() {
            Ok(km) => {
                add("Default-слой:".to_string());
                for (k, act) in &km.base {
                    let label = dev.slot_label_name(k.0 as u8)
                        .map(|l| format!(" [{l}]"))
                        .unwrap_or_default();
                    add(format!(
                        "  слот {:>3} (0x{:02X}){} → {}",
                        k.0,
                        k.0,
                        label,
                        action_label(act)
                    ));
                }
                add("FN1-слой:".to_string());
                for (k, act) in &km.fn_layer.bindings {
                    let label = dev.slot_label_name(k.0 as u8)
                        .map(|l| format!(" [{l}]"))
                        .unwrap_or_default();
                    add(format!(
                        "  слот {:>3} (0x{:02X}){} → {}",
                        k.0,
                        k.0,
                        label,
                        action_label(act)
                    ));
                }
            }
            Err(_) if dev.connection_type() != ConnectionType::Usb => {
                // Ремап прошивки работает только по USB (wired): на 2.4G/BT
                // радиоканал настройки не ревёрснут (journal 10.59/10.62).
                add(
                    "Раскладка доступна только при подключении по USB (кабель). \
                     Сейчас клавиатура подключена по 2.4G — переключи на USB и \
                     нажми «Показать раскладку» снова."
                        .to_string(),
                );
            }
            Err(e) => add(format!("Ошибка чтения раскладки: {e}")),
        }
    }));

// Сбросить всё: читаем заводскую раскладку (read-only, wsclx:
    // GET_DEFAULT_KEY_MATRIX/FN) — эталон для черновика. Полный заводской
    // сброс (factory_reset, SET_FACTORY_RESET 0xFF) по кнопке НЕ вызываем:
    // он стирает профили и подсветку, требует явного подтверждения и
    // живого железа. Метод остаётся доступен драйверу/CLI.
    let fetcher_clear = fetcher.clone();
    clear_all.connect_clicked(move |_| {
        let Some(dev) = fetcher_clear() else {
            return;
        };
        match dev.get_factory_keymap() {
            Ok(km) => println!(
                "keyboard: заводская раскладка прочитана (base={}, fn={})",
                km.base.len(),
                km.fn_layer.bindings.len()
            ),
            Err(_) => println!("keyboard: заводская раскладка этим устройством не отдаётся"),
        }
    });

    // Макросы
    let fetcher4 = fetcher.clone();
    rec_btn.connect_clicked(clone!(
        #[strong]
        macro_id_spin,
        move |_| {
        let id = macro_id_spin.value() as u8;
        if let Some(dev) = fetcher4() {
            let _ = dev.record_macro(id);
        }
    }));
    let fetcher5 = fetcher.clone();
    stop_btn.connect_clicked(move |_| {
        if let Some(dev) = fetcher5() {
            let _ = dev.stop_macro();
        }
    });

    panel
}

/// Превратить KeyAction в строку для отображения.
pub fn action_label(action: &KeyAction) -> String {
    match action {
        KeyAction::Press(k) => format!("Клавиша 0x{k:02X}"),
        KeyAction::Media(m) => format!("Медиа 0x{m:02X}"),
        KeyAction::Modifier(m) => format!("Модификатор {m}"),
        KeyAction::FnLayer(l) => format!("Слой Fn {l}"),
        KeyAction::Macro(m) => format!("Макрос {m}"),
    }
}