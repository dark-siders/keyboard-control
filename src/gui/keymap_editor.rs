// --- Keymap & Fn-Layer Editor ---
// Переназначение клавиш, слои Fn, макросы.

use gtk::glib::clone;
use gtk::prelude::*;

use std::cell::Cell;
use std::rc::Rc;

use crate::devices::KeyboardDevice;
use crate::types::{Color, ConnectionType, FnLayer, KeyAction, KeyId};

/// Собрать панель редактора клавиш.
pub fn build_keymap_editor(
    device_fetcher: impl Fn() -> Option<Rc<dyn KeyboardDevice>> + 'static,
) -> gtk::Box {
    let fetcher: Rc<dyn Fn() -> Option<Rc<dyn KeyboardDevice>>> = Rc::new(device_fetcher);
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 6);
    panel.set_margin_top(8);

    let title = gtk::Label::new(Some("Переназначение клавиш"));
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Start);
    panel.append(&title);

    let hint = gtk::Label::new(Some("Кликните по клавише на схеме, выберите новое действие справа и нажмите «Назначить». Работает при подключении по USB. Карту текущих назначений показывает кнопка «Показать раскладку»; подробный список — в терминале: `kbc remap <VID:PID> --list`."));
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
    let layout = crate::gui::keyboard_layout::Layout::for_layout(crate::devices::Layout::L75);
    let (layout_area, colors) =
        crate::gui::keyboard_layout::LayoutWidget::new_drawing_area(layout.clone());
    layout_area.set_height_request(280);
    panel.append(&layout_area);

    // --- Клик-редактор: выбор слота по клику -----------------------------
    let sel_lbl = gtk::Label::new(Some("Кликните по клавише в раскладке, чтобы выбрать слот."));
    sel_lbl.add_css_class("dim-label");
    sel_lbl.set_wrap(true);
    sel_lbl.set_halign(gtk::Align::Start);
    panel.append(&sel_lbl);

    let assign_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let type_dd = gtk::DropDown::from_strings(&["Клавиша (hex)", "Медиа (hex)", "Модификатор (0..7)"]);
    type_dd.set_selected(0);
    let usage_entry = gtk::Entry::builder()
        .placeholder_text("usage, напр. 2C")
        .width_chars(10)
        .build();
    let assign_btn = gtk::Button::with_label("Назначить");
    assign_btn.add_css_class("suggested-action");
    assign_row.append(&type_dd);
    assign_row.append(&usage_entry);
    assign_row.append(&assign_btn);
    panel.append(&assign_row);

    let assign_status = gtk::Label::new(Some(""));
    assign_status.add_css_class("dim-label");
    assign_status.set_wrap(true);
    assign_status.set_halign(gtk::Align::Start);
    panel.append(&assign_status);

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
    {
        let fetcher2 = fetcher.clone();
        remap_apply.connect_clicked(clone!(
            #[strong]
            list,
            move |_| {
            let Some(dev) = fetcher2() else { return; };
            fill_keymap_list(dev.as_ref(), &list);
        }));
    }

    // Клик по макету: выбираем слот, подсвечиваем клавишу через colors.
    let selected_slot: Rc<Cell<Option<u8>>> = Rc::new(Cell::new(None));
    let last_key: Rc<Cell<Option<u16>>> = Rc::new(Cell::new(None));
    {
        let fetcher_clk = fetcher.clone();
        let colors_clk = colors.clone();
        let layout_clk = layout.clone();
        let area_clk = layout_area.clone();
        let sel_lbl_clk = sel_lbl.clone();
        let assign_status_clk = assign_status.clone();
        let sel_clk = selected_slot.clone();
        let last_clk = last_key.clone();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(1);
        gesture.connect_pressed(move |_g, _n, x, y| {
            let Some(dev) = fetcher_clk() else { return; };
            let w = area_clk.width() as f64;
            let h = area_clk.height() as f64;
            if w <= 0.0 || h <= 0.0 {
                assign_status_clk.set_text("Область макета ещё не отрисована — попробуйте снова.");
                return;
            }
            let Some(keycode) = layout_clk.key_at(x, y, w, h) else {
                assign_status_clk.set_text("Клик мимо клавиш раскладки.");
                return;
            };
            let Some(slot) = dev.slot_for_keycode(keycode) else {
                assign_status_clk.set_text("Клавиша не поддерживает переназначение этим устройством.");
                return;
            };
            // Сброс подсветки ранее выбранной клавиши
            if let Some(prev) = last_clk.get() {
                colors_clk.borrow_mut().remove(&KeyId(prev));
            }
            colors_clk
                .borrow_mut()
                .insert(KeyId(keycode), Color { r: 255, g: 160, b: 0 });
            last_clk.set(Some(keycode));
            sel_clk.set(Some(slot));
            area_clk.queue_draw();
            let label = dev
                .slot_label_name(slot)
                .map(|l| format!(" ({l})"))
                .unwrap_or_default();
            let cur = dev
                .get_current_keymap()
                .ok()
                .and_then(|km| current_action(&km, slot).map(|a| action_label(a)))
                .unwrap_or_else(|| "не назначено".to_string());
            sel_lbl_clk.set_text(&format!(
                "Выбран слот {slot}{label} — текущее действие: {cur}. \
                 Введите новое действие справа и нажмите «Назначить»."
            ));
        });
        layout_area.add_controller(gesture);
    }

    // Назначение действия на выбранный слот
    {
        let fetcher_as = fetcher.clone();
        let sel_as = selected_slot.clone();
        let mode_as = mode_sw.clone();
        let type_as = type_dd.clone();
        let usage_as = usage_entry.clone();
        let status_as = assign_status.clone();
        let list_as = list.clone();
        assign_btn.connect_clicked(move |_| {
            let Some(dev) = fetcher_as() else { return; };
            let Some(slot) = sel_as.get() else {
                status_as.set_text("Сначала кликните по клавише в раскладке.");
                return;
            };
            let trimmed = usage_as.text().trim().to_string();
            if trimmed.is_empty() {
                status_as.set_text("Введите usage нового действия.");
                return;
            }
            let act = match type_as.selected() {
                0 => match parse_hex_usage(&trimmed) {
                    Ok(u) => KeyAction::Press(u),
                    Err(e) => {
                        status_as.set_text(&format!("Неверный hex usage клавиши (например 2C): {e}."));
                        return;
                    }
                },
                1 => match parse_hex_usage(&trimmed) {
                    Ok(u) => KeyAction::Media(u),
                    Err(e) => {
                        status_as.set_text(&format!("Неверный hex usage медиа (например E2): {e}."));
                        return;
                    }
                },
                _ => match trimmed.parse::<u8>() {
                    Ok(m) if m <= 7 => KeyAction::Modifier(m),
                    _ => {
                        status_as.set_text("Модификатор: число 0..7 (0=LCtrl..7=RWin).");
                        return;
                    }
                },
            };
            let layer = if mode_as.is_active() { "FN1" } else { "Default" };
            let result = if mode_as.is_active() {
                dev.set_fn_layer(FnLayer {
                    bindings: vec![(KeyId(slot as u16), act.clone())],
                })
            } else {
                dev.remap_key(KeyId(slot as u16), act.clone())
            };
            match result {
                Ok(_) => {
                    status_as.set_text(&format!(
                        "Слот {slot} записан в слой {layer}: {} (read-modify-write, память клавиатуры).",
                        action_label(&act)
                    ));
                    fill_keymap_list(dev.as_ref(), &list_as);
                }
                Err(e) => status_as.set_text(&format!("Запись не удалась: {e}")),
            }
        });
    }

// Сбросить на заводские: читаем заводскую раскладку (GET_DEFAULT_*) и
    // записываем её обратно в клавиатуру (remap_key + set_fn_layer).
    // Для F75 (нет GET_DEFAULT_*) — используем жёстко заданный заводской дефолт
    // из дампа (журнал 10.72). Клавиатура кикается после каждой записи —
    // переподключаемся через fetcher после каждого ремапа.
    let fetcher_clear = fetcher.clone();
    let status_clear = assign_status.clone();
    let list_clear = list.clone();
    clear_all.connect_clicked(move |_| {
        // Запоминаем VID:PID для переподключения
        let Some(dev0) = fetcher_clear() else { return; };
        let target_vidpid = dev0.vid_pid();
        drop(dev0);

        status_clear.set_text("Чтение заводской раскладки…");
        let km = match fetcher_clear() {
            Some(dev) => match dev.get_factory_keymap() {
                Ok(km) => km,
                Err(_) => {
                    // Фоллбек для F75: жёстко заданный заводской дефолт (дамп 10.72)
                    if dev.connection_type() == crate::types::ConnectionType::Usb {
                        status_clear.set_text("Восстановление заводского дефолта F75 (hardcoded)…");
                        crate::devices::aula_f75::factory_keymap_f75()
                    } else {
                        status_clear.set_text("Не удалось прочитать заводскую раскладку: устройство не поддерживает эту функцию");
                        return;
                    }
                }
            },
            None => {
                status_clear.set_text("Устройство не найдено");
                return;
            }
        };

        // Вспомогательная функция: переподключение к тому же VID:PID
        let reconnect = || -> Result<_, String> {
            let Some(dev) = fetcher_clear() else { return Err("устройство не найдено".to_string()); };
            if dev.vid_pid() == target_vidpid { return Ok(dev); }
            // Ищем в fetcher'е
            for _ in 0..20 {
                if let Some(dev) = fetcher_clear() {
                    if dev.vid_pid() == target_vidpid { return Ok(dev); }
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err("устройство не переподключилось".to_string())
        };

        // Восстанавливаем Default-слой (пропускаем залоченные слоты)
        for (k, act) in &km.base {
            let mut dev = match reconnect() { Ok(d) => d, Err(e) => { status_clear.set_text(&e); return; } };
            match dev.remap_key(*k, act.clone()) {
                Ok(_) => {}
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("залочен") || msg.contains("защищено") || msg.contains("запрещена") {
                        continue;
                    }
                    status_clear.set_text(&format!("Ошибка сброса слота {}: {e}", k.0));
                    return;
                }
            }
        }
        // Восстанавливаем FN1-слой (пропускаем залоченные слоты)
        for (k, act) in &km.fn_layer.bindings {
            let mut dev = match reconnect() { Ok(d) => d, Err(e) => { status_clear.set_text(&e); return; } };
            match dev.set_fn_layer(crate::types::FnLayer {
                bindings: vec![(*k, act.clone())],
            }) {
                Ok(_) => {}
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("залочен") || msg.contains("защищено") || msg.contains("запрещена") {
                        continue;
                    }
                    status_clear.set_text(&format!("Ошибка сброса FN1 слота {}: {e}", k.0));
                    return;
                }
            }
        }
        status_clear.set_text("Заводская раскладка восстановлена (Default + FN1).");
        // Финальное переподключение для обновления списка
        if let Ok(dev) = reconnect() {
            fill_keymap_list(dev.as_ref(), &list_clear);
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

/// Очистить ListBox и вывести карту раскладки (Default + FN1).
fn fill_keymap_list(dev: &dyn KeyboardDevice, list: &gtk::ListBox) {
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
                let label = dev
                    .slot_label_name(k.0 as u8)
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
                let label = dev
                    .slot_label_name(k.0 as u8)
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
}

/// Текущее действие для слота (или `None`, если слот не назначен).
fn current_action<'a>(km: &'a crate::types::Keymap, slot: u8) -> Option<&'a KeyAction> {
    km.base
        .iter()
        .find(|(k, _)| k.0 == slot as u16)
        .map(|(_, a)| a)
        .or_else(|| {
            km.fn_layer
                .bindings
                .iter()
                .find(|(k, _)| k.0 == slot as u16)
                .map(|(_, a)| a)
        })
}

/// Распарсить hex usage: с/без префикса 0x.
pub fn parse_hex_usage(s: &str) -> Result<u16, String> {
    let t = s.trim().strip_prefix("0x").unwrap_or(s.trim());
    u16::from_str_radix(t, 16).map_err(|e| e.to_string())
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