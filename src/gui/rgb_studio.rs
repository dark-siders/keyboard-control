// --- RGB & Light Studio ---
// Управление подсветкой: режим, яркость, скорость, поклавишная окраска,
// пресеты.

use gtk::gdk;
use gtk::glib::clone;
use gtk::prelude::*;

use std::cell::{Cell, RefCell};
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
/// `device_fetcher` отдаёт текущее открытое устройство; `reconnect` переоткрывает
/// его после кика (write заставляет клавиатуру пересоздать hidraw, старый handle
/// умирает) — используется после «Записать в память» и для продолжения гашения.
pub fn build_rgb_studio(
    device_fetcher: impl Fn() -> Option<Rc<dyn KeyboardDevice>> + 'static,
    reconnect: impl Fn() -> anyhow::Result<String> + 'static,
) -> gtk::Box {
    let fetcher: std::rc::Rc<dyn Fn() -> Option<Rc<dyn KeyboardDevice>>> = std::rc::Rc::new(device_fetcher);
    let reconnect: Rc<dyn Fn() -> anyhow::Result<String>> = Rc::new(reconnect);
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
    bright_scale.set_hexpand(true);
    let bright_pct = gtk::Label::new(Some("50%"));
    bright_pct.set_width_chars(4);
    bright_pct.set_xalign(1.0);
    bright_scale.connect_value_changed(clone!(
        #[strong]
        bright_pct,
        move |s| {
            let pb = (s.value() / 255.0 * 100.0).round() as u32;
            bright_pct.set_text(&format!("{pb}%"));
        }
    ));
    let bright_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    bright_row.append(&bright_scale);
    bright_row.append(&bright_pct);

    let speed_label = gtk::Label::new(Some("Скорость"));
    speed_label.set_halign(gtk::Align::Start);
    let speed_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 255.0, 1.0);
    speed_scale.set_value(60.0);
    speed_scale.set_hexpand(true);
    let speed_pct = gtk::Label::new(Some("24%"));
    speed_pct.set_width_chars(4);
    speed_pct.set_xalign(1.0);
    speed_scale.connect_value_changed(clone!(
        #[strong]
        speed_pct,
        move |s| {
            let pb = (s.value() / 255.0 * 100.0).round() as u32;
            speed_pct.set_text(&format!("{pb}%"));
        }
    ));
    let speed_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    speed_row.append(&speed_scale);
    speed_row.append(&speed_pct);

    // --- Пресеты --------------------------------------------------------
    let preset_label = gtk::Label::new(Some("Пресеты"));
    preset_label.set_halign(gtk::Align::Start);
    let preset_dd = gtk::DropDown::builder().build();
    {
        let model = gtk::StringList::new(&["—", "Static", "Дыхание", "Волна", "Спектр", "Дождь"]);
        preset_dd.set_model(Some(&model));
    }

    // --- Аппаратный эффект (в память) ------------------------------------
    // Эффекты прошивки F75: селектор `data[10]`, сохраняются в память самой
    // клавиатуры (Fn+\), поэтому держатся БЕЗ запущенного приложения.
    // Host-эффекты выше (режим/яркость/скорость) в память НЕ пишутся.
    let hw_label = gtk::Label::new(Some("Аппаратный эффект (сохраняется в память)"));
    hw_label.set_halign(gtk::Align::Start);
    let hw_dd = gtk::DropDown::builder().build();
    {
        use crate::devices::aula_f75::HW_EFFECTS;
        let mut names = vec!["— (не менять эффект)"];
        names.extend(HW_EFFECTS.iter().map(|(_, n)| *n));
        let model = gtk::StringList::new(&names);
        hw_dd.set_model(Some(&model));
    }

    // Пометка о сохранении в память: статические цвета и host-эффекты (режим/
    // яркость/скорость/Живой цвет) транслируются приложением в реальном
    // времени и в память клавиатуры НЕ пишутся (во многих прошивках, в т.ч.
    // у AULA, однотонной «статики» в аппаратной таблице нет вообще).
    let hw_note = gtk::Label::new(Some(
        "В память клавиатуры сохраняются только аппаратные эффекты из списка \
         выше — они работают и без запущенного приложения. Статические цвета \
         и эффекты из блоков «Режим/Яркость/Скорость» и «Живой цвет» сохранить \
         в память невозможно: они живут, пока приложение открыто.",
    ));
    hw_note.add_css_class("dim-label");
    hw_note.set_halign(gtk::Align::Start);
    hw_note.set_wrap(true);

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
    // Признак «свет погашен host-кадром»: влияет на «Записать в память» без
    // выбранного аппаратного эффекта — в этом случае пишем свет ВЫКЛЮЧЕННЫМ
    // (data[76]=00), а не «свет вкл» (фикс 10.31).
    let light_off: Rc<Cell<bool>> = Rc::new(Cell::new(false));

    // --- Сборка ------------------------------------------------------------
    panel.append(&mode_label_l);
    panel.append(&mode_dd);
    panel.append(&bright_label);
    panel.append(&bright_row);
    panel.append(&speed_label);
    panel.append(&speed_row);
    panel.append(&preset_label);
    panel.append(&preset_dd);
    panel.append(&hw_label);
    panel.append(&hw_dd);
    panel.append(&hw_note);
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
        #[strong]
        live_entry,
        #[strong]
        live_timer,
        #[strong]
        light_off,
        #[strong]
        fetcher2,
        move |_| {
            if fetcher2().is_some() {
                // Свет включён host-эффектом — снимаем флаг «погашено» (10.31).
                light_off.set(false);
                // Host-движок (10.18): аппаратной эффект-таблицы у F75 нет,
                // поэтому режим/яркость/скорость рендерим НА ХОСТЕ и гоним
                // кадрами через подтверждённый render_frame()-канал с keepalive,
                // пока GUI жив.
                use crate::devices::aula_f75::{host_frame, HostEffectParams};
                let mode = mode_from_index(mode_dd.selected());
                let base = Color::from_hex_checked(live_entry.text().trim())
                    .unwrap_or(Color::new(255, 90, 0));
                let params = HostEffectParams {
                    mode,
                    base,
                    brightness: bright_scale.value().round() as u8,
                    speed: speed_scale.value().round() as u8,
                };
                // ВАЖНО (фикс 10.21): при повторном «Применить» НАДО
                // останавливать прежний keepalive-поток и запускать новый,
                // иначе новые режим/яркость/скорость не применятся (старый
                // таймер продолжал гнать старые кадры).
                let timer_dst = live_timer.clone();
                if let Some(id) = timer_dst.borrow_mut().take() {
                    id.remove();
                }
                let dev_fetch = fetcher2.clone();
                let mut t_ms = 0u32;
                *timer_dst.borrow_mut() = Some(gtk::glib::timeout_add_local(
                    std::time::Duration::from_millis(500),
                    move || {
                        let frame = host_frame(&params, t_ms);
                        t_ms += 500;
                        if let Some(dev) = dev_fetch() {
                            if dev.render_frame(&frame).is_err() {
                                return glib::ControlFlow::Break;
                            }
                        }
                        glib::ControlFlow::Continue
                    },
                ));
            }
        }
    ));

    let fetcher3 = fetcher.clone();
    let status_s = live_status.clone();
    let timer_s = live_timer.clone();
    let color_s = live_color.clone();
    let off_s = light_off.clone();
    let reconnect_s = reconnect.clone();
    save_to_board.connect_clicked(move |_| {
        // Фикс 10.27: СНАЧАЛА останавливаем host-поток кадров, иначе keepalive
        // (каждые 400 мс) перезапишет LED и «съест» применённый аппаратный
        // эффект/выключение. Именно это было: записали — клава потухла и
        // через секунду вернулась к host-статике.
        if let Some(id) = timer_s.borrow_mut().take() {
            id.remove();
        }
        color_s.borrow_mut().take();
        let Some(dev) = fetcher3() else {
            status_s.set_text("Устройство не подключено: откройте его на вкладке «Устройства»");
            return;
        };
        // Если выбран аппаратный эффект — пишем его селектор data[10] в память
        // (AULA F75): эффект + свет вкл (данные[76]=09), держится без приложения.
        // Если эффект не выбран, но свет был погашен host-кадром — сохраняем
        // свет ВЫКЛЮЧЕННЫМ (data[76]=00), а не «свет вкл» (фикс 10.31).
        // Иначе — просто сохраняем текущий конфиг со светом-вкл.
        let sel = hw_dd.selected();
        let was_off = off_s.get();
        let res = if sel > 0 && sel as usize <= crate::devices::aula_f75::HW_EFFECTS.len() {
            let code = crate::devices::aula_f75::HW_EFFECTS[(sel - 1) as usize].0;
            dev.save_hardware_effect(code)
        } else if was_off {
            dev.save_hardware_light(false)
        } else {
            dev.save_profile_to_eeprom(0)
        };
        match res {
            Ok(()) => {
                // write кикает клавиатуру (пересоздание hidraw) — старый handle
                // мёртв. Переоткрываем устройство, чтобы продолжать работать.
                // (фикс 10.31)
                if let Err(e) = reconnect_s() {
                    status_s.set_text(&format!("Конфиг записан, но переоткрытие устройства не удалось: {e}"));
                    return;
                }
                if was_off && sel == 0 {
                    // Свет остаётся погашенным: перезапускаем чёрный keepalive
                    // (после кика клавиатура стартует с RAM-подсветкой; флеш
                    // data[76]=00 применится только при полном отключении).
                    let black = vec![Color::new(0, 0, 0); crate::devices::aula_f75::RGB_LED_COUNT];
                    if let Some(dev) = fetcher3() {
                        let _ = dev.render_frame(&black);
                    }
                    let timer_d = timer_s.clone();
                    let dev_fetch = fetcher3.clone();
                    *timer_d.borrow_mut() = Some(gtk::glib::timeout_add_local(
                        std::time::Duration::from_millis(400),
                        move || {
                            let Some(dev) = dev_fetch() else {
                                return glib::ControlFlow::Break;
                            };
                            if dev.render_frame(&black).is_err() {
                                return glib::ControlFlow::Break;
                            }
                            glib::ControlFlow::Continue
                        },
                    ));
                    status_s.set_text(
                        "Свет выключен и сохранён в память (после перезапуска клавиатуры не \
                         включится). Сейчас подсветка погашена до закрытия приложения.",
                    );
                } else if sel == 0 {
                    status_s.set_text(
                        "Записан текущий аппаратный конфиг (свет вкл). ВАЖНО: статические \
                         цвета и host-эффекты в память не сохраняются — они работают, пока \
                         приложение открыто.",
                    );
                } else {
                    status_s.set_text(
                        "Конфиг записан в память клавиатуры. Эффект/свет включится сам при \
                         старте, без запущенного приложения.",
                    );
                }
            }
            Err(e) => status_s.set_text(&format!("Ошибка записи конфига: {e}")),
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
        let off_a = light_off.clone();
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
            // Свет включён host-цветом — снимаем флаг «погашено» (10.31).
            off_a.set(false);
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

    // Пресеты: выбор в списке выставляет соответствующий режим подсветки
    // (mode_dd), базовый цвет в поле ввода и сразу применяет эффект через
    // host-движок (фикс 10.23: раньше пресет просто красил статичным цветом,
    // не меняя режим).
    {
        let preset_entry = live_entry.clone();
        let preset_mode_dd = mode_dd.clone();
        let preset_apply_btn = apply.clone();
        preset_dd.connect_selected_notify(move |dd| {
            // (индекс режима в mode_dd, базовый цвет) по выбору пресета:
            let sel = match dd.selected() {
                1 => Some((0, "#FFFFFF")),  // Static   → Рeжим 0
                2 => Some((1, "#00A8FF")),  // Дыхание  → Рeжим 1
                3 => Some((2, "#FF8000")),  // Волна    → Рeжим 2
                4 => Some((3, "#FF0066")),  // Спектр   → Рeжим 3
                5 => Some((5, "#78C8FF")),  // Дождь    → Рeжим 5
                _ => None, // «—»: ничего не запускаем
            };
            if let Some((mode_idx, hex)) = sel {
                preset_entry.set_text(hex);
                preset_mode_dd.set_selected(mode_idx);
                preset_apply_btn.emit_clicked();
            }
        });
    }

    let timer_c = live_timer.clone();
    let color_c = live_color.clone();
    let fetcher6 = fetcher.clone();
    let status_off = live_status.clone();
    let off_c = light_off.clone();
    live_off.connect_clicked(move |_| {
        // Останавливаем host-поток (иначе он же и перезажжёт свет).
        if let Some(id) = timer_c.borrow_mut().take() {
            id.remove();
        }
        color_c.borrow_mut().take();
        let Some(dev) = fetcher6() else {
            status_off.set_text("Устройство не подключено: откройте его на вкладке «Устройства»");
            return;
        };
        // Свет погашен host-кадром — запоминаем (10.31): «Записать в память»
        // без выбранного эффекта сохранит свет ВЫКЛЮЧЕННЫМ (data[76]=00).
        off_c.set(true);
        // Чёрный кадр гасит все LED сейчас.
        let black = vec![Color::new(0, 0, 0); frame_capacity];
        if let Err(e) = dev.render_frame(&black) {
            status_off.set_text(&format!("Ошибка отправки кадра: {e}"));
            return;
        }
        // ВАЖНО (фикс 10.30): одиночный чёрный кадр НЕ держится — F75 в direct-
        // режиме «умирает» через ~1 с без потока пакетов и возвращается к
        // аппаратному эффекту (возврат света через ~2 с). Поэтому запускаем
        // ПОСТОЯННЫЙ keepalive с чёрным кадром, пока приложение открыто.
        // Запись же в память делаем отдельной кнопкой («Записать в память»),
        // т.к. write кикает клавиатуру и она стартует с RAM-подсветкой
        // (флеш применяется только при полном отключении питания, 10.28).
        let timer_d = timer_c.clone();
        let dev_fetch = fetcher6.clone();
        *timer_d.borrow_mut() = Some(gtk::glib::timeout_add_local(
            std::time::Duration::from_millis(400),
            move || {
                let Some(dev) = dev_fetch() else {
                    return glib::ControlFlow::Break;
                };
                if dev.render_frame(&black).is_err() {
                    return glib::ControlFlow::Break;
                }
                glib::ControlFlow::Continue
            },
        ));
        status_off.set_text(
            "Свет выключен (host-кадр, свет не включится, пока приложение открыто). ВАЖНО: \
             host-гашение в память не пишется — после выключения/переподключения клавиатуры \
             свет вернётся. Чтобы он не включался после перезапуска — выберите аппаратный \
             эффект и нажмите «Записать в память».",
        );
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