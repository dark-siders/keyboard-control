// --- Keymap & Fn-Layer Editor ---
// Переназначение клавиш, слои Fn, макросы, энкодер.

use gtk::glib::clone;
use gtk::prelude::*;

use std::rc::Rc;

use crate::devices::KeyboardDevice;
use crate::types::{EncoderAction, FnLayer, KeyAction, KeyId, Keymap};

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

    let hint = gtk::Label::new(Some("Кликните по клавише, чтобы переназначить её. Выберите действие в появившемся диалоге."));
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
    let actions_header = gtk::Label::new(Some("Текущие переназначения"));
    actions_header.add_css_class("heading");
    actions_header.set_halign(gtk::Align::Start);
    actions_box.append(&actions_header);

    let list = gtk::ListBox::builder().margin_top(4).build();
    actions_box.append(&list);
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

    // --- Энкодер/ролик --------------------------------------------------
    let enc_header = gtk::Label::new(Some("Энкодер / ролик"));
    enc_header.add_css_class("heading");
    enc_header.set_halign(gtk::Align::Start);
    panel.append(&enc_header);

    let enc_dd = gtk::DropDown::builder().build();
    let enc_model = gtk::StringList::new(&[
        "Громкость",
        "Медиа: следующий трек",
        "Медиа: предыдущий трек",
        "Медиа: play/pause",
        "Медиа: mute",
        "Прокрутка: вверх",
        "Прокрутка: вниз",
        "Яркость: вверх",
        "Яркость: вниз",
        "Отключено",
    ]);
    enc_dd.set_model(Some(&enc_model));
    panel.append(&enc_dd);

    let apply_enc = gtk::Button::with_label("Применить настройку энкодера");
    apply_enc.add_css_class("suggested-action");
    panel.append(&apply_enc);

    let remap_apply = gtk::Button::with_label("Применить раскладку");
    remap_apply.add_css_class("suggested-action");
    panel.append(&remap_apply);

    let clear_all = gtk::Button::with_label("Сбросить всё");
    panel.append(&clear_all);

    // --- Обработчики --------------------------------------------------------
    // Применение карты клавиш
    let fetcher2 = fetcher.clone();
    remap_apply.connect_clicked(clone!(
        #[strong]
        layout_area,
        #[strong]
        mode_sw,
        move |_| {
        if let Some(dev) = fetcher2() {
            let km = Keymap {
                base: Vec::new(),
                fn_layer: FnLayer { bindings: Vec::new() },
            };
            let _ = dev.set_fn_layer(km.fn_layer.clone());
            let _ = dev.remap_key(KeyId(0x2C), KeyAction::Press(0x2C));
        }
        let _ = (mode_sw.is_active(), layout_area.width_request());
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

    // Энкодер
    let fetcher3 = fetcher.clone();
    apply_enc.connect_clicked(clone!(
        #[strong]
        enc_dd,
        move |_| {
        let idx = enc_dd.selected();
        let action = match idx {
            0 => EncoderAction::Volume,
            1 => EncoderAction::MediaNext,
            2 => EncoderAction::MediaPrev,
            3 => EncoderAction::PlayPause,
            4 => EncoderAction::Mute,
            5 => EncoderAction::ScrollUp,
            6 => EncoderAction::ScrollDown,
            7 => EncoderAction::BrightnessUp,
            8 => EncoderAction::BrightnessDown,
            _ => EncoderAction::None,
        };
        if let Some(dev) = fetcher3() {
            let _ = dev.set_encoder_action(action);
        }
    }));

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