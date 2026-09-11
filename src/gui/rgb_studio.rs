// --- RGB & Light Studio ---
// Управление подсветкой: режим, яркость, скорость, поклавишная окраска,
// пресеты.

use gtk::gdk;
use gtk::glib::clone;
use gtk::prelude::*;

use std::cell::RefCell;
use std::rc::Rc;

use crate::devices::KeyboardDevice;
use crate::types::{Color, KeyId, RgbMode};

/// Собрать выпадающий список режимов подсветки.
pub fn populate_modes(dropdown: &gtk::DropDown) {
    let model = gtk::StringList::new(&[
        "Статичная",
        "Дыхание",
        "Волна",
        "Спектр",
        "Риппл",
        "Дождь",
        "Реактивная",
        "Пользовательская",
    ]);
    dropdown.set_model(Some(&model));
    dropdown.set_selected(0);
}

/// Преобразовать индекс режима из GUI в Enum.
pub fn mode_from_index(idx: u32) -> RgbMode {
    match idx {
        0 => RgbMode::Static,
        1 => RgbMode::Breathing,
        2 => RgbMode::Wave,
        3 => RgbMode::SpectrumCycle,
        4 => RgbMode::Ripple,
        5 => RgbMode::Rain,
        6 => RgbMode::Reactive,
        _ => RgbMode::Custom,
    }
}

/// Собрать панель RGB-студии.
pub fn build_rgb_studio(
    device_fetcher: impl Fn() -> Option<Rc<dyn KeyboardDevice>> + 'static,
) -> gtk::Box {
    let fetcher: std::rc::Rc<dyn Fn() -> Option<Rc<dyn KeyboardDevice>>> = std::rc::Rc::new(device_fetcher);
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 6);
    panel.set_margin_top(8);

    let title = gtk::Label::new(Some("RGB & Light Studio"));
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Start);
    panel.append(&title);

    // --- Режим ---------------------------------------------------------
    let mode_label_l = gtk::Label::new(Some("Режим подсветки"));
    mode_label_l.set_halign(gtk::Align::Start);
    let mode_dd = gtk::DropDown::builder().build();
    populate_modes(&mode_dd);

    // --- Яркость и скорость --------------------------------------------
    let bright_label = gtk::Label::new(Some("Яркость"));
    bright_label.set_halign(gtk::Align::Start);
    let bright_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 255.0, 1.0);
    bright_scale.set_value(128.0);

    let speed_label = gtk::Label::new(Some("Скорость"));
    speed_label.set_halign(gtk::Align::Start);
    let speed_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 255.0, 1.0);
    speed_scale.set_value(100.0);

    // --- Пресеты --------------------------------------------------------
    let preset_label = gtk::Label::new(Some("Пресеты"));
    preset_label.set_halign(gtk::Align::Start);
    let preset_dd = gtk::DropDown::builder().build();
    {
        let model = gtk::StringList::new(&["—", "Static", "Дыхание", "Волна", "Спектр", "Дождь"]);
        preset_dd.set_model(Some(&model));
    }

    // --- Кнопки -----------------------------------------------------------
    let apply = gtk::Button::with_label("Применить");
    apply.add_css_class("suggested-action");
    apply.set_halign(gtk::Align::Start);

    let save_to_board = gtk::Button::with_label("Записать в память");
    save_to_board.add_css_class("suggested-action");
    save_to_board.set_halign(gtk::Align::Start);

    // --- Живой цвет (host-rendering) ------------------------------------
    // AULA F75 отображает цвета только пока идёт поток кадров (keepalive).
    let live_label = gtk::Label::new(Some("Живой цвет (host-rendering)"));
    live_label.add_css_class("heading");
    live_label.set_halign(gtk::Align::Start);

    let live_entry = gtk::Entry::builder()
        .text("#FF0000")
        .placeholder_text("#RRGGBB")
        .halign(gtk::Align::Start)
        .width_chars(9)
        .build();

    // Палитра быстрого выбора (без системного цветопикера/portal).
    let palette_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    palette_row.set_halign(gtk::Align::Start);

    let live_on = gtk::Button::with_label("Включить цвет");
    live_on.add_css_class("suggested-action");
    live_on.set_halign(gtk::Align::Start);

    let live_off = gtk::Button::with_label("Выключить свет");
    live_off.set_halign(gtk::Align::Start);

    let live_status = gtk::Label::new(Some("Подключите клавиатуру: Устройства → Открыть"));
    live_status.add_css_class("dim-label");
    live_status.set_halign(gtk::Align::Start);
    live_status.set_wrap(true);

    // Текущий таймер потока кадров + текущий цвет.
    let live_timer: Rc<RefCell<Option<gtk::glib::SourceId>>> = Rc::new(RefCell::new(None));
    let live_color: Rc<RefCell<Option<Color>>> = Rc::new(RefCell::new(None));

    // --- Сборка ------------------------------------------------------------
    panel.append(&mode_label_l);
    panel.append(&mode_dd);
    panel.append(&bright_label);
    panel.append(&bright_scale);
    panel.append(&speed_label);
    panel.append(&speed_scale);
    panel.append(&preset_label);
    panel.append(&preset_dd);
    panel.append(&apply);
    panel.append(&save_to_board);
    panel.append(&live_label);
    panel.append(&live_entry);
    panel.append(&palette_row);
    panel.append(&live_on);
    panel.append(&live_off);
    panel.append(&live_status);

    // --- Обработчики ---------------------------------------------------------
    let fetcher2 = fetcher.clone();
    apply.connect_clicked(clone!(
        #[strong]
        mode_dd,
        #[strong]
        bright_scale,
        #[strong]
        speed_scale,
        move |_| {
            if let Some(dev) = fetcher2() {
                let mode = mode_from_index(mode_dd.selected());
                let brightness = bright_scale.value().round() as u8;
                let speed = speed_scale.value().round() as u8;
                let _ = dev.set_rgb_mode(mode);
                let _ = dev.set_rgb_brightness(brightness);
                let _ = dev.set_rgb_speed(speed);
            }
        }
    ));

    let fetcher3 = fetcher.clone();
    save_to_board.connect_clicked(move |_| {
        if let Some(dev) = fetcher3() {
            let _ = dev.save_profile_to_eeprom(0);
        }
    });

    // --- Живой цвет: обработчики ---------------------------------------
    let frame_capacity = crate::devices::aula_f75::RGB_LED_COUNT;

    // Общий «включить текущий цвет из поля ввода» (палитра и кнопка).
    let apply_cur: Rc<dyn Fn()> = {
        let entry_a = live_entry.clone();
        let fetcher_a = fetcher.clone();
        let timer_a = live_timer.clone();
        let color_a = live_color.clone();
        let status_a = live_status.clone();
        Rc::new(move || {
            let text = entry_a.text().to_string();
            let Some(color) = Color::from_hex_checked(text.trim()) else {
                status_a.set_text(&format!("Неверный цвет: «{text}» (ожидается #RRGGBB)"));
                return;
            };
            let Some(dev) = fetcher_a() else {
                status_a.set_text("Устройство не подключено: откройте его на вкладке «Устройства»");
                return;
            };
            // Останавливаем предыдущий поток.
            if let Some(id) = timer_a.borrow_mut().take() {
                id.remove();
            }
            let frame = vec![color; frame_capacity];
            if let Err(e) = dev.render_frame(&frame) {
                status_a.set_text(&format!("Ошибка отправки кадра: {e}"));
                return;
            }
            *color_a.borrow_mut() = Some(color);
            status_a.set_text(&format!("Кадр #{:06X} отправлен ({frame_capacity} LED)…", color.to_argb() & 0xFF_FFFF));
            // Запускаем keepalive: поток кадров каждые 400 мс.
            let timer_b = timer_a.clone();
            let color_b = color_a.clone();
            let dev_fetch = fetcher_a.clone();
            *timer_b.borrow_mut() = Some(gtk::glib::timeout_add_local(
                std::time::Duration::from_millis(400),
                move || {
                    let Some(c) = color_b.borrow().as_ref().copied() else {
                        return glib::ControlFlow::Break;
                    };
                    let Some(dev) = dev_fetch() else {
                        return glib::ControlFlow::Break;
                    };
                    if dev.render_frame(&vec![c; frame_capacity]).is_err() {
                        return glib::ControlFlow::Break;
                    }
                    glib::ControlFlow::Continue
                },
            ));
        })
    };

    let apply_cur2 = apply_cur.clone();
    live_on.connect_clicked(move |_| apply_cur2());

    // Палитра: кнопка ставит цвет в поле и сразу применяет.
    for (rgb, name) in [
        (0xFF0000u32, "Красный"),
        (0x00FF00u32, "Зелёный"),
        (0x0000FFu32, "Синий"),
        (0xFFFFFFu32, "Белый"),
        (0xFF8000u32, "Оранжевый"),
    ] {
        let btn = gtk::Button::with_label(name);
        btn.add_css_class("flat");
        let entry_b = live_entry.clone();
        let apply_b = apply_cur.clone();
        let hex = format!("#{:06X}", rgb & 0xFF_FFFF);
        btn.connect_clicked(move |_| {
            entry_b.set_text(&hex);
            apply_b();
        });
        palette_row.append(&btn);
    }

    // Пресеты: выбор в списке сразу ставит цвет в поле ввода и применяет.
    {
        let preset_entry = live_entry.clone();
        let preset_apply = apply_cur.clone();
        preset_dd.connect_selected_notify(move |dd| {
            let hex = match dd.selected() {
                1 => Some("#FFFFFF"), // Static
                2 => Some("#00A8FF"), // Дыхание
                3 => Some("#FF8000"), // Волна
                4 => Some("#FF0066"), // Спектр
                5 => Some("#78C8FF"), // Дождь
                _ => None, // «—»
            };
            if let Some(h) = hex {
                preset_entry.set_text(h);
                preset_apply();
            }
        });
    }

    let timer_c = live_timer.clone();
    let color_c = live_color.clone();
    let fetcher6 = fetcher.clone();
    live_off.connect_clicked(move |_| {
        if let Some(id) = timer_c.borrow_mut().take() {
            id.remove();
        }
        color_c.borrow_mut().take();
        if let Some(dev) = fetcher6() {
            // Чёрный кадр гасит все LED.
            let _ = dev.render_frame(&vec![Color::new(0, 0, 0); frame_capacity]);
        }
    });

    panel
}

/// Цветовой пикер: покраска выбранных клавиш в макете одним цветом.
pub fn build_color_picker(
    colors: &crate::gui::keyboard_layout::KeyColorMap,
    area: &gtk::DrawingArea,
    device_fetcher: impl Fn() -> Option<Rc<dyn KeyboardDevice>> + 'static,
) -> gtk::Box {
    let panel = gtk::Box::new(gtk::Orientation::Horizontal, 6);

    let hint = gtk::Label::new(Some("Цвет для клавиш:"));
    hint.add_css_class("dim-label");

    let color_btn = gtk::ColorDialogButton::builder().build();
    color_btn.set_rgba(&gdk::RGBA::new(1.0, 0.0, 0.0, 1.0));

    let apply = gtk::Button::with_label("Применить цвет");
    apply.connect_clicked(clone!(
        #[strong]
        colors,
        #[strong]
        area,
        #[strong]
        color_btn,
        move |_| {
        let rgba = color_btn.rgba();
        let color = Color::new(
            (rgba.red() * 255.0).round() as u8,
            (rgba.green() * 255.0).round() as u8,
            (rgba.blue() * 255.0).round() as u8,
        );
        // Пример: красим все клавиши (в продакшене — только выбранные)
        let keys: Vec<KeyId> = {
            let map = colors.borrow();
            map.iter().map(|(k, _)| *k).collect()
        };
        if keys.is_empty() {
            // Закрашиваем как минимум пробел и несколько соседних
            for k in [0x2C, 0x1C, 0x2D, 0x2F, 0x2E] {
                colors.borrow_mut().insert(KeyId(k), color);
            }
        } else {
            for k in keys {
                colors.borrow_mut().insert(k, color);
            }
        }
        area.queue_draw();
        if let Some(dev) = device_fetcher() {
            let _ = dev.set_rgb_mode(RgbMode::Custom);
        }
    }));

    panel.append(&hint);
    panel.append(&color_btn);
    panel.append(&apply);
    panel
}

/// Slick: показать загрузочное сообщение-заглушку (не блокируя стек).
pub fn build_presets_list() -> gtk::Box {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 4);
    box_.append(&gtk::Label::new(Some("Сохранённые пресеты появятся здесь")));
    box_
}