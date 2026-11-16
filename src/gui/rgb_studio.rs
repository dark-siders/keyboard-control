// --- RGB & Light Studio ---
// Управление подсветкой: режим, яркость, скорость, поклавишная окраска,
// пресеты.

use gtk::gdk;
use gtk::glib::clone;
use gtk::prelude::*;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::devices::KeyboardDevice;
use crate::presets::AudioStyle;
use crate::types::{Color, ConnectionType, KeyId, RgbMode};

// --- Фоновое переподключение HID-канала после кика -------------------
// write в память «кикает» клавиатуру: она пересоздаёт hidraw-узлы ~1-2 с.
// Блокирующее hidapi-открытие в главном GTK-потоке на этот период вешало
// UI навсегда (фикс 2026-09-13). Поэтому открытие выполняется фоновым
// потоком, а главный опрашивает канал и лишь кладёт готовое устройство
// в слот (без HID-вызовов).
type ReopenRx = std::sync::mpsc::Receiver<std::result::Result<crate::devices::HidBridge, String>>;
type ReopenStore = Rc<RefCell<Option<ReopenRx>>>;

/// Запустить фоновое переподключение HID-канала (если ещё не бежит).
/// Блокирующий hidapi выполняется в потоке; результат — по каналу.
fn ensure_reopen(store: &ReopenStore, vid: u16, pid: u16) {
    if store.borrow().is_none() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            // После кика узлы появляются в течение ~1-2 с; перебираем до 18 с.
            for _ in 0..60 {
                match crate::devices::open_bridge(vid, pid) {
                    Ok(bridge) => {
                        let _ = tx.send(Ok(bridge));
                        return;
                    }
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(300)),
                }
            }
            let _ = tx.send(Err("не удалось переподключить клавиатуру".to_string()));
        });
        *store.borrow_mut() = Some(rx);
    }
}

/// Принять готовый HID-мост из фонового потока и положить живое устройство
/// в слот. Неблокирующая (try_recv) — вызывается из главного потока.
/// Возвращает имя устройства при успешном переподключении.
fn drain_reopen(
    store: &ReopenStore,
    set_device: &dyn Fn(Box<dyn KeyboardDevice>) -> String,
) -> Option<String> {
    let mut b = store.borrow_mut();
    let Some(rx) = b.as_ref() else {
        return None;
    };
    match rx.try_recv() {
        Ok(Ok(bridge)) => {
            match crate::devices::build_device(bridge) {
                Ok(dev) => {
                    let name = set_device(dev);
                    *b = None;
                    Some(name)
                }
                Err(_) => {
                    *b = None;
                    None
                }
            }
        }
        Ok(Err(_)) => {
            *b = None;
            None
        }
        Err(_) => None,
    }
}

/// ПОЧЕМУ НЕЛЬЗЯ ИСПОЛЬЗОВАТЬ `SourceId::remove()` (фикс 10.37/10.39):
///
/// glib-0.20.12: `SourceId::remove()` возвращает `()` и ПАНИКУЕТ ВНУТРИ себя
/// (`source.rs:41 unwrap`), если source уже завершился сам — например
/// reconnect/keepalive-таймер вернул `ControlFlow::Break`, а в слоте остался
/// его stale-`SourceId`. Повторный `remove()` на таком ID → паника в функции,
/// которая не может unwind (вызвана из сигнального обработчика GTK) → abort.
/// Поэтому `SourceId::remove()` в этом коде НЕ используется вообще: остановку
/// таймера выполняет «поколение кадров» (см. `frame_gen`) — прежние замыкания
/// сами завершаются на следующем тике, сверив несовпавшее поколение.

/// Поколение потокового таймера (кадры/переподключение).
/// Каждый новый запуск кадрового потока инкрементирует его; все прежние
/// замыкания на следующем тике видят несовпадение со своим захваченным
/// поколением и возвращают `ControlFlow::Break` (завершаются сами).
type FrameGen = Rc<Cell<u64>>;

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
        "Аудио-реактив",
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
        7 => RgbMode::Audio,
        _ => RgbMode::Custom,
    }
}

/// Обратное преобразование: режим Enum → индекс в dropdown-списке режимов.
pub fn mode_to_index(mode: RgbMode) -> u32 {
    match mode {
        RgbMode::Static => 0,
        RgbMode::Breathing => 1,
        RgbMode::Wave => 2,
        RgbMode::SpectrumCycle => 3,
        RgbMode::Ripple => 4,
        RgbMode::Rain => 5,
        RgbMode::Reactive => 6,
        RgbMode::Audio => 7,
        RgbMode::Custom => 8,
    }
}

pub fn audio_style_from_index(idx: u32) -> AudioStyle {
    match idx {
        1 => AudioStyle::Bars,
        _ => AudioStyle::Row,
    }
}

pub fn audio_style_to_index(style: AudioStyle) -> u32 {
    match style {
        AudioStyle::Row => 0,
        AudioStyle::Bars => 1,
    }
}

/// Что сохранить в память клавиатуры (выбор делается ДО запуска потока —
/// в обработчик виджетов нельзя класть Rc-виджеты, но enum из u8 — можно).
pub enum SaveKind {
    /// Аппаратный эффект прошивки (селектор `data[10]`) + свет вкл (`a=09`).
    Effect(u8),
    /// Свет выключен: `a=00` во всех слотах эффектов (`data[58..96]`).
    LightOff,
    /// Текущий аппаратный конфиг (свет вкл, эффект не меняется).
    Config,
}

/// Probe последнего состояния для автосохранения при закрытии приложения:
/// возвращает действие, если в памяти надо что-то зафиксировать.
pub type PersistProbe = Rc<dyn Fn() -> Option<SaveKind>>;

/// Задача для ресивера 2.4G: persistent-запись RGB или света в память.
/// Параметры — только примитивные/Send типы (RgbMode/Color/числа), поэтому
/// можно уводить в поток. Устройство в потоке открывается своим честным
/// handle'ом (`open_vidpid`), GUI-слот не трогаем (фикс 10.34).
enum WlTask {
    Rgb {
        mode: RgbMode,
        color: Option<Color>,
        brightness: u8,
        speed: u8,
    },
    Light {
        on: bool,
    },
}

/// Выполнить `WlTask` в отдельном потоке; вернуть канал с результатом
/// `Result<имя_устройства, строка_ошибки>`. Главный поток опрашивает канал
/// коротким одноразовым таймером — UI не блокируется блокирующим hidapi.
fn spawn_wl_task(vid: u16, pid: u16, task: WlTask) -> std::sync::mpsc::Receiver<Result<String, String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let res: Result<String, String> = (|| {
            let dev = crate::devices::open_vidpid(vid, pid)
                .map_err(|e| format!("не удалось открыть устройство: {e}"))?;
            let ok = match task {
                WlTask::Rgb {
                    mode,
                    color,
                    brightness,
                    speed,
                } => dev
                    .apply_rgb_persistent(mode, color, brightness, speed)
                    .map_err(|e| e.to_string())?,
                WlTask::Light { on } => dev
                    .apply_light_persistent(on)
                    .map_err(|e| e.to_string())?,
            };
            match ok {
                true => Ok(dev.name().to_string()),
                false => Err("устройство не поддерживает запись в память".to_string()),
            }
        })();
        let _ = tx.send(res);
    });
    rx
}

/// Собрать панель RGB-студии.
/// `device_fetcher` отдаёт текущее открытое устройство; `set_device` кладёт
/// в слот устройство, построенное из HID-моста фонового переподключения
/// (без блокирующих hidapi-вызовов в главном потоке — фикс 2026-09-13);
/// `last_vidpid` отдаёт последний открытый VID/PID — по ним фоновый поток
/// открывает независимый handle для записи в память (фикс 10.34).
/// Возвращает панель и `PersistProbe` — по нему app.rs при закрытии окна
/// автосохраняет последнее состояние (выключенный свет / выбранный эффект).
pub fn build_rgb_studio(
    device_fetcher: impl Fn() -> Option<Rc<dyn KeyboardDevice>> + 'static,
    set_device: impl Fn(Box<dyn KeyboardDevice>) -> String + 'static,
    last_vidpid: impl Fn() -> Option<(u16, u16)> + 'static,
) -> (gtk::Box, PersistProbe, super::presets_page::PresetCallbacks) {
    let fetcher: std::rc::Rc<dyn Fn() -> Option<Rc<dyn KeyboardDevice>>> = std::rc::Rc::new(device_fetcher);
    let set_device: Rc<dyn Fn(Box<dyn KeyboardDevice>) -> String> = Rc::new(set_device);
    let vidpid: Rc<dyn Fn() -> Option<(u16, u16)>> = Rc::new(last_vidpid);
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 6);
    panel.set_margin_top(8);

    let title = gtk::Label::new(Some("Подсветка"));
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Start);
    panel.append(&title);

    // --- Эффект ---------------------------------------------------------
    let mode_label_l = gtk::Label::new(Some("Эффект"));
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

    // --- Базовый цвет (общий для всех режимов, в т.ч. аудио) ---------------
    // Цвет выбирается из выпадающего списка (имя + hex), рядом — образец
    // цвета (свотч). Техническое поле ввода hex скрыто, но остаётся
    // источником истины для внутренней логики (пресеты/аудио/палитра).
    let live_entry = gtk::Entry::builder()
        .text("#FF0000")
        .placeholder_text("#RRGGBB")
        .halign(gtk::Align::Start)
        .width_chars(9)
        .build();
    live_entry.set_visible(false);

    // Расширенная палитра: (имя, hex). Клавиатура показывает любой 24-битный
    // цвет, поэтому набор широкий — по группам оттенков.
    const QUICK_COLORS: &[(&str, &str)] = &[
        // Красные / розовые
        ("Красный", "#FF0000"),
        ("Алый", "#FF2400"),
        ("Малиновый", "#DC143C"),
        ("Бордовый", "#800000"),
        ("Томатный", "#FF6347"),
        ("Коралловый", "#FF7F50"),
        ("Лососёвый", "#FA8072"),
        ("Розовый", "#FFC0CB"),
        ("Горячий розовый", "#FF69B4"),
        ("Пурпурный", "#FF0066"),
        ("Маджента", "#FF00FF"),
        // Оранжевые / жёлтые
        ("Оранжевый", "#FF8000"),
        ("Тёмно-оранжевый", "#FF8C00"),
        ("Морковный", "#ED9121"),
        ("Янтарный", "#FFBF00"),
        ("Золотой", "#FFD700"),
        ("Жёлтый", "#FFFF00"),
        ("Лимонный", "#FFF44F"),
        ("Кремовый", "#FFFDD0"),
        ("Персиковый", "#FFDAB9"),
        // Зелёные
        ("Зелёный", "#00FF00"),
        ("Лаймовый", "#32CD32"),
        ("Весенний", "#00FF7F"),
        ("Изумрудный", "#00C864"),
        ("Мятный", "#98FF98"),
        ("Морская волна", "#2E8B57"),
        ("Тёмно-зелёный", "#006400"),
        ("Оливковый", "#808000"),
        ("Хаки", "#C3B091"),
        // Циановые
        ("Циан", "#00FFFF"),
        ("Бирюзовый", "#40E0D0"),
        ("Аквамарин", "#7FFFD4"),
        ("Тёмный циан", "#008B8B"),
        // Голубые / синие
        ("Голубой", "#00A8FF"),
        ("Небесно-голубой", "#87CEEB"),
        ("Лазурный", "#007FFF"),
        ("Синий", "#0000FF"),
        ("Королевский синий", "#4169E1"),
        ("Стальной", "#4682B4"),
        ("Тёмно-синий", "#00008B"),
        ("Индиго", "#4B0082"),
        // Фиолетовые / сиреневые
        ("Фиолетовый", "#8000FF"),
        ("Орхидея", "#DA70D6"),
        ("Сливовый", "#DDA0DD"),
        ("Лавандовый", "#E6E6FA"),
        ("Сиреневый", "#C8A2C8"),
        // Коричневые / бежевые
        ("Коричневый", "#A52A2A"),
        ("Шоколадный", "#D2691E"),
        ("Карамельный", "#B8860B"),
        ("Бежевый", "#F5F5DC"),
        // Белые / серые / чёрный
        ("Белый", "#FFFFFF"),
        ("Снежный", "#FFFAFA"),
        ("Серебристый", "#C0C0C0"),
        ("Серый", "#808080"),
        ("Графитовый", "#696969"),
        ("Чёрный", "#000000"),
    ];

    // Образец выбранного цвета (квадрат 24×24 рядом со списком).
    let swatch_color: Rc<RefCell<String>> = Rc::new(RefCell::new("#FF0000".to_string()));
    let swatch = gtk::DrawingArea::new();
    swatch.set_content_width(24);
    swatch.set_content_height(24);
    {
        let swatch_c = swatch_color.clone();
        swatch.set_draw_func(move |_, cr, w, h| {
            let hex = swatch_c.borrow().clone();
            if let Some(c) = Color::from_hex_checked(&hex) {
                cr.set_source_rgb(
                    f64::from(c.r) / 255.0,
                    f64::from(c.g) / 255.0,
                    f64::from(c.b) / 255.0,
                );
                // Круглый образец цвета (не квадрат).
                let cx = w as f64 / 2.0;
                let cy = h as f64 / 2.0;
                let rad = (w.min(h) as f64) / 2.0 - 1.0;
                cr.arc(cx, cy, rad, 0.0, 2.0 * std::f64::consts::PI);
                cr.fill().ok();
            }
        });
    }

    // Выпадающий список цветов: «Имя  #RRGGBB».
    let color_dd = gtk::DropDown::builder().halign(gtk::Align::Start).build();
    {
        let names: Vec<String> = QUICK_COLORS
            .iter()
            .map(|(n, h)| format!("{n}  {h}"))
            .collect();
        let names_s: Vec<&str> = names.iter().map(String::as_str).collect();
        color_dd.set_model(Some(&gtk::StringList::new(&names_s)));
        color_dd.set_selected(0);
    }

    // Фабрика строк списка: цветной кружок + имя (и в самом меню, и в
    // закрытом поле выбора). Hex-код в строках модели остаётся только для
    // разбора цвета — пользователю показывается имя без кода.
    let color_factory = gtk::SignalListItemFactory::new();
    color_factory.connect_setup(move |_, item: &glib::Object| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let dot = gtk::DrawingArea::new();
        dot.set_content_width(14);
        dot.set_content_height(14);
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_hexpand(true);
        row.append(&dot);
        row.append(&label);
        item.set_child(Some(&row));
    });
    color_factory.connect_bind(move |_, item: &glib::Object| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(obj) = item.item().and_downcast::<gtk::StringObject>() else {
            return;
        };
        let s = obj.string().to_string();
        let (name, hex) = match s.rsplit_once("  ") {
            Some((n, h)) if h.starts_with('#') && h.len() == 7 => {
                (n.to_string(), h.to_string())
            }
            _ => (s.clone(), "#FFFFFF".to_string()),
        };
        let Some(row) = item.child().and_downcast::<gtk::Box>() else {
            return;
        };
        if let Some(l) = row.last_child().and_downcast::<gtk::Label>() {
            l.set_text(&name);
        }
        if let Some(d) = row.first_child().and_downcast::<gtk::DrawingArea>() {
            let hex_d = hex;
            d.set_draw_func(move |_, cr, w, h| {
                let (r, g, b) = Color::from_hex_checked(&hex_d)
                    .map(|c| (c.r, c.g, c.b))
                    .unwrap_or((255u8, 255u8, 255u8));
                cr.set_source_rgb(
                    f64::from(r) / 255.0,
                    f64::from(g) / 255.0,
                    f64::from(b) / 255.0,
                );
                let cx = w as f64 / 2.0;
                let cy = h as f64 / 2.0;
                let rad = (w.min(h) as f64) / 2.0 - 1.0;
                cr.arc(cx, cy, rad, 0.0, 2.0 * std::f64::consts::PI);
                cr.fill().ok();
            });
        }
    });
    color_dd.set_factory(Some(&color_factory));
    // Синхронизация UI (список + свотч) с заданным hex: цвет из палитры —
    // выбираем его; незнакомый (например из пресета) — добавляем пунктом.
    let sync_color_ui: Rc<dyn Fn(&str)> = {
        let color_dd_c = color_dd.clone();
        let swatch_c = swatch_color.clone();
        let swatch_w = swatch.clone();
        Rc::new(move |hex| {
            *swatch_c.borrow_mut() = hex.trim().to_uppercase();
            swatch_w.queue_draw();
            let upper = hex.trim().to_uppercase();
            if let Some(idx) = QUICK_COLORS.iter().position(|(_, h)| *h == upper) {
                color_dd_c.set_selected(idx as u32);
            } else {
                let mut names: Vec<String> = QUICK_COLORS
                    .iter()
                    .map(|(n, h)| format!("{n}  {h}"))
                    .collect();
                names.push(format!("Свой цвет  {upper}"));
                let names_s: Vec<&str> = names.iter().map(String::as_str).collect();
                color_dd_c.set_model(Some(&gtk::StringList::new(&names_s)));
                color_dd_c.set_selected((names.len() - 1) as u32);
            }
        })
    };
    // Выбор в списке → обновляем hex-поле и образец.
    {
        let live_entry_c = live_entry.clone();
        let sync_c = sync_color_ui.clone();
        color_dd.connect_selected_notify(move |dd| {
            let idx = dd.selected() as usize;
            let hex = QUICK_COLORS.get(idx).map(|(_, h)| *h).unwrap_or("#FF0000");
            live_entry_c.set_text(hex);
            sync_c(hex);
        });
    }

    // Строка «список цветов + образец».
    let color_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    color_row.set_halign(gtk::Align::Start);
    color_row.append(&color_dd);
    color_row.append(&swatch);

    // --- Аудио-реактив (USB): настройки визуализатора --------------------
    let audio_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    audio_box.set_visible(false);
    let audio_label = gtk::Label::new(Some("Аудио-реактив (USB)"));
    audio_label.add_css_class("heading");
    audio_label.set_halign(gtk::Align::Start);
    let audio_colors_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    audio_colors_row.set_halign(gtk::Align::Start);
    // Базовый цвет (фон) — синхронизирован с полем «Живой цвет» ниже.
    let audio_base_entry = gtk::Entry::builder()
        .text("#FF5A00")
        .placeholder_text("#RRGGBB")
        .halign(gtk::Align::Start)
        .width_chars(9)
        .build();
    let audio_low_entry = gtk::Entry::builder()
        .text("#33FF33")
        .placeholder_text("низы #RRGGBB")
        .halign(gtk::Align::Start)
        .width_chars(9)
        .build();
    let audio_high_entry = gtk::Entry::builder()
        .text("#3333FF")
        .placeholder_text("верхи #RRGGBB")
        .halign(gtk::Align::Start)
        .width_chars(9)
        .build();
    audio_colors_row.append(&gtk::Label::new(Some("Базовый:")));
    audio_colors_row.append(&audio_base_entry);
    audio_colors_row.append(&gtk::Label::new(Some("Низы:")));
    audio_colors_row.append(&audio_low_entry);
    audio_colors_row.append(&gtk::Label::new(Some("Верхи:")));
    audio_colors_row.append(&audio_high_entry);

    // Синхронизация базового цвета: audio_base_entry <-> live_entry
    // Защита от рекурсии: Rc<Cell<bool>> флаг, чтобы не зациклиться на set_text -> changed.
    let sync_guard = Rc::new(Cell::new(false));
    let sync_guard1 = sync_guard.clone();
    let live_entry_clone = live_entry.clone();
    audio_base_entry.connect_changed(move |entry| {
        if sync_guard1.get() { return; }
        let text = entry.text().trim().to_string();
        if crate::types::Color::from_hex_checked(&text).is_some() {
            sync_guard1.set(true);
            live_entry_clone.set_text(&text);
            sync_guard1.set(false);
        }
    });
    let audio_base_entry_clone = audio_base_entry.clone();
    let sync_guard2 = sync_guard.clone();
    live_entry.connect_changed(move |entry| {
        if sync_guard2.get() { return; }
        let text = entry.text().trim().to_string();
        if crate::types::Color::from_hex_checked(&text).is_some() {
            sync_guard2.set(true);
            audio_base_entry_clone.set_text(&text);
            sync_guard2.set(false);
        }
    });
    let audio_sens_label = gtk::Label::new(Some("Чувствительность"));
    audio_sens_label.set_halign(gtk::Align::Start);
    let audio_sens_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.1, 2.0, 0.05);
    audio_sens_scale.set_value(0.75);
    audio_sens_scale.set_hexpand(true);
    let audio_sens_val = gtk::Label::new(Some("0.75"));
    audio_sens_val.set_width_chars(5);
    audio_sens_val.set_xalign(1.0);
    audio_sens_scale.connect_value_changed(clone!(
        #[strong]
        audio_sens_val,
        move |s| {
            audio_sens_val.set_text(&format!("{:.2}", s.value()));
        }
    ));
    let audio_sens_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    audio_sens_row.append(&audio_sens_scale);
    audio_sens_row.append(&audio_sens_val);
    let audio_style_label = gtk::Label::new(Some("Стиль визуализации"));
    audio_style_label.set_halign(gtk::Align::Start);
    let audio_style_dd = gtk::DropDown::builder().build();
    let audio_style_model = gtk::StringList::new(&["Верхний ряд", "Столбики (как cava)"]);
    audio_style_dd.set_model(Some(&audio_style_model));
    audio_style_dd.set_selected(0);
    let audio_note = gtk::Label::new(Some("Спектр системного звука по USB: верхний ряд или столбики по колонкам; буквы получают базовый цвет. В память клавиатуры не пишется."));
    audio_note.add_css_class("dim-label");
    audio_note.set_wrap(true);
    audio_note.set_halign(gtk::Align::Start);
    audio_box.append(&audio_label);
    audio_box.append(&audio_colors_row);
    audio_box.append(&audio_sens_label);
    audio_box.append(&audio_sens_row);
    audio_box.append(&audio_style_label);
    audio_box.append(&audio_style_dd);
    audio_box.append(&audio_note);

    // Показывать/скрывать аудио-секцию в зависимости от выбранного режима.
    {
        let audio_box_c = audio_box.clone();
        mode_dd.connect_selected_notify(move |dd| {
            audio_box_c.set_visible(mode_from_index(dd.selected()) == RgbMode::Audio);
        });
    }

    // --- Эффект для памяти клавиатуры ------------------------------------
    // Эффекты прошивки F75: селектор `data[10]`, сохраняются в память самой
    // клавиатуры (Fn+\), поэтому держатся БЕЗ запущенного приложения.
    // Host-эффекты выше (режим/яркость/скорость) в память НЕ пишутся.
    let hw_label = gtk::Label::new(Some("Эффект для памяти клавиатуры (работает без приложения)"));
    hw_label.set_halign(gtk::Align::Start);
    let hw_dd = gtk::DropDown::builder().build();
    // Клон для live_off: там сбрасываем выбор эффекта на «—» (фикс 10.35).
    let hw_dd_for_off = hw_dd.clone();
    // Клон для PersistProbe: hw_dd уезжает в кнопочное замыкание (move), а probe
    // живёт дальше и должен читать выбор эффекта при закрытии окна (автосейв).
    let hw_dd_for_probe = hw_dd.clone();
    // Какая модель эффектов сейчас в списке «Аппаратный эффект»: false = USB
    // (HW_EFFECTS, селектор data[10]), true = ресивер (WL_EFFECTS, led[10]).
    // Список подстраивается под подключённое устройство (см. rebuild при
    // отображении панели); коды НЕ синонимичны, поэтому клики сохраняют
    // код из таблицы, соответствующей текущей модели.
    let hw_is_wl: Rc<Cell<bool>> = Rc::new(Cell::new(false));

    // Перестроить список эффектов под ресивер/USB: модели HW_EFFECTS (USB,
    // селектор data[10]) и WL_EFFECTS (ресивер, led[10]) НЕ синонимичны, поэтому
    // список «Аппаратный эффект» подстраивается под подключённое устройство.
    fn rebuild_hw_model(dd: &gtk::DropDown, is_wl: bool) {
        let effects: &[(u8, &str)] = if is_wl {
            crate::devices::aula_f75::WL_EFFECTS
        } else {
            crate::devices::aula_f75::HW_EFFECTS
        };
        let mut names = vec!["— (не менять эффект)"];
        names.extend(effects.iter().map(|(_, n)| *n));
        let model = gtk::StringList::new(&names);
        dd.set_model(Some(&model));
        dd.set_selected(0);
    }
    {
        use crate::devices::aula_f75::HW_EFFECTS;
        let mut names = vec!["— (не менять эффект)"];
        names.extend(HW_EFFECTS.iter().map(|(_, n)| *n));
        let model = gtk::StringList::new(&names);
        hw_dd.set_model(Some(&model));
    }

    // Пометка о сохранении в память: статические цвета и host-эффекты (режим/
    // яркость/скорость/живой цвет) транслируются приложением в реальном
    // времени и в память клавиатуры НЕ пишутся (во многих прошивках, в т.ч.
    // у AULA, однотонной «статики» в аппаратной таблице нет вообще).
    let hw_note = gtk::Label::new(Some(
        "На USB в память сохраняются только эффекты из списка выше — они работают \
         без запущенного приложения. Статические цвета и эффекты «Эффект»/«Яркость»/\
         «Скорость» живут, пока приложение открыто. На 2.4G и Ajazz настройки \
         сохраняются в клавиатуре автоматически. Все изменения применяются сразу, \
         без кнопки; при включённом «Сохранить в клавиатуру» память записывается \
         сама через пару секунд после последнего изменения.",
    ));
    hw_note.add_css_class("dim-label");
    hw_note.set_halign(gtk::Align::Start);
    hw_note.set_wrap(true);

    // --- Кнопки -----------------------------------------------------------
    let apply = gtk::Button::with_label("Применить");
    apply.add_css_class("suggested-action");
    apply.set_halign(gtk::Align::Start);
    // Кнопка скрыта: все настройки применяются АВТОМАТИЧЕСКИ при изменении
    // (выпадающие списки — сразу, слайдеры — через 300 мс после последнего
    // изменения). Её обработчик запускается сигналами через emit_clicked().
    apply.set_visible(false);

    // Переключатель «Сохранить в клавиатуру»: при включённом тумблере
    // настройки дополнительно сохраняются в память клавиатуры
    // (для USB — через список «Эффект для памяти клавиатуры»; для 2.4G/Ajazz
    // настройки сохраняются и так). Кнопка «Записать в память» ниже скрыта:
    // её логика переиспользуется через emit_clicked().
    let persist_switch = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .active(true)
        .build();
    let persist_label = gtk::Label::new(Some("Сохранить в клавиатуру"));
    persist_label.set_halign(gtk::Align::Start);
    let persist_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    persist_row.set_halign(gtk::Align::Start);
    persist_row.append(&persist_switch);
    persist_row.append(&persist_label);

    let save_to_board = gtk::Button::with_label("Записать в память");
    save_to_board.add_css_class("suggested-action");
    save_to_board.set_halign(gtk::Align::Start);
    // Скрыта: сохранение теперь управляется переключателем и происходит
    // автоматически после применения (с задержкой, чтобы серия изменений
    // дала одну запись, а не «кик» клавиатуры на каждый тик слайдера).
    save_to_board.set_visible(false);

    // Авто-применение слайдеров/полей с защитой от дребезга: последний
    // сигнал запускает одно применение после затихания. Для 2.4G задержка
    // больше (1.5 с): запись на ресивер AULA — двухфазная («flash and
    // revert»), частые записи дёргают подсветку/ресивер (жалоба владельца
    // 2026-11-16).
    let schedule_apply: Rc<dyn Fn()> = {
        let apply_c = apply.clone();
        let fetcher_d = fetcher.clone();
        let gen = Rc::new(Cell::new(0u64));
        Rc::new(move || {
            let delay_ms = if fetcher_d()
                .map(|d| d.connection_type() == ConnectionType::Wireless2_4G)
                .unwrap_or(false)
            {
                1500u64
            } else {
                300u64
            };
            gen.set(gen.get().wrapping_add(1));
            let my = gen.get();
            let gen_t = gen.clone();
            let apply_t = apply_c.clone();
            gtk::glib::timeout_add_local(std::time::Duration::from_millis(delay_ms), move || {
                if gen_t.get() != my {
                    return glib::ControlFlow::Break; // появился более свежий сигнал
                }
                apply_t.emit_clicked();
                glib::ControlFlow::Break
            });
        })
    };

    // Минимальный интервал между записями на ресивер 2.4G — 2 с. Защищает
    // от спама при быстрых кликах по спискам (эффект/цвет применяются сразу,
    // без дребезга): каждая запись AULA — двухфазная, ресивер не любит поток.
    // Пока не прошло 2 с — отправку откладываем.
    let last_wl_write: Rc<Cell<u64>> = Rc::new(Cell::new(0));
    let wl_allowed: Rc<dyn Fn() -> bool> = {
        let last = last_wl_write.clone();
        Rc::new(move || {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let prev = last.get();
            if prev != 0 && now.saturating_sub(prev) < 2000 {
                return false;
            }
            last.set(now);
            true
        })
    };

    // Авто-сохранение в память с более длинной задержкой (1.5 с): серия
    // изменений = одна запись во флеш, без «кика» на каждый чих.
    let schedule_save: Rc<dyn Fn()> = {
        let save_c = save_to_board.clone();
        let gen = Rc::new(Cell::new(0u64));
        Rc::new(move || {
            gen.set(gen.get().wrapping_add(1));
            let my = gen.get();
            let gen_t = gen.clone();
            let save_t = save_c.clone();
            gtk::glib::timeout_add_local(std::time::Duration::from_millis(1500), move || {
                if gen_t.get() != my {
                    return glib::ControlFlow::Break;
                }
                save_t.emit_clicked();
                glib::ControlFlow::Break
            });
        })
    };

    // --- Авто-применение: сигналы -----------------------------------------
    // Выпадающие списки применяются сразу, слайдеры и поля — через
    // schedule_apply (300 мс после последнего изменения).
    {
        let apply_m = apply.clone();
        mode_dd.connect_selected_notify(move |_| apply_m.emit_clicked());
    }
    {
        let apply_c = apply.clone();
        color_dd.connect_selected_notify(move |_| apply_c.emit_clicked());
    }
    {
        let sched = schedule_apply.clone();
        bright_scale.connect_value_changed(move |_| sched());
    }
    {
        let sched = schedule_apply.clone();
        speed_scale.connect_value_changed(move |_| sched());
    }
    {
        let sched = schedule_apply.clone();
        audio_sens_scale.connect_value_changed(move |_| sched());
    }
    {
        let sched = schedule_apply.clone();
        audio_style_dd.connect_selected_notify(move |_| sched());
    }
    {
        let sched = schedule_apply.clone();
        audio_low_entry.connect_changed(move |_| sched());
    }
    {
        let sched = schedule_apply.clone();
        audio_high_entry.connect_changed(move |_| sched());
    }

    // --- Цвет подсветки (host-rendering) --------------------------------
    // AULA F75 отображает цвета только пока идёт поток кадров (keepalive).
    let live_label = gtk::Label::new(Some("Цвет подсветки"));
    live_label.add_css_class("heading");
    live_label.set_halign(gtk::Align::Start);

    let live_on = gtk::Button::with_label("Включить подсветку");
    live_on.add_css_class("suggested-action");
    live_on.set_halign(gtk::Align::Start);

    let live_off = gtk::Button::with_label("Выключить подсветку");
    live_off.set_halign(gtk::Align::Start);

    let live_status = gtk::Label::new(Some("Подключите клавиатуру: вкладка «Устройства» → «Подключить»"));
    live_status.add_css_class("dim-label");
    live_status.set_halign(gtk::Align::Start);
    live_status.set_wrap(true);

    // Поколение потоковых таймеров (кадры, keepalive, reconnect).
    // Каждый новый запуск потока инкрементирует его; активные замыкания
    // сверяют захваченное поколение на каждом тике и сами завершаются
    // (ControlFlow::Break), если поколение устарело. Это заменяет удаление
    // `SourceId::remove()`, которое в glib-0.20.12 ПАНИКУЕТ на уже
    // завершившемся таймере (source.rs:41 unwrap) и роняет GUI (10.39).
    let frame_gen: FrameGen = Rc::new(Cell::new(0));
    let live_color: Rc<RefCell<Option<Color>>> = Rc::new(RefCell::new(None));
    // Признак «свет погашен host-кадром»: влияет на «Записать в память» без
    // выбранного аппаратного эффекта — в этом случае пишем свет ВЫКЛЮЧЕННЫМ
    // (a=00 во всех слотах эффектов data[58..96]), а не «свет вкл».
    let light_off: Rc<Cell<bool>> = Rc::new(Cell::new(false));
    // Stop-флаг фонового аудио-потока (аудио-реактив): GUI не может читать
    // монитор звука в главном потоке (блокирующий read подвиснет на тишине),
    // поэтому кадры гонит отдельный поток, а кнопки/гашение его останавливают
    // этим флагом.
    let audio_stop: Rc<RefCell<Option<Arc<AtomicBool>>>> = Rc::new(RefCell::new(None));

    // --- Сборка ------------------------------------------------------------
    // Логичный порядок: эффект → цвет → яркость/скорость → аудио (если
    // выбран) → сохранение в память → кнопки → статус.
    panel.append(&mode_label_l);
    panel.append(&mode_dd);
    panel.append(&live_label);
    panel.append(&color_row);
    panel.append(&bright_label);
    panel.append(&bright_row);
    panel.append(&speed_label);
    panel.append(&speed_row);
    panel.append(&audio_box);
    panel.append(&persist_row);
    panel.append(&hw_label);
    panel.append(&hw_dd);
    panel.append(&hw_note);
    panel.append(&apply);
    panel.append(&live_on);
    panel.append(&live_off);
    panel.append(&save_to_board);
    panel.append(&live_status);

    // --- Перестройка списка эффектов под подключённое устройство -----------
    // USB (HW_EFFECTS, селектор data[10]) и ресивер (WL_EFFECTS, led[10]) —
    // РАЗНЫЕ наборы кодов с разными названиями. При показе RGB-страницы
    // сверяемся с подключённым устройством и перестраиваем список.
    let hw_dd_sync = hw_dd.clone();
    let hw_flag_sync = hw_is_wl.clone();
    let fetcher_sync = fetcher.clone();
    panel.connect_map(move |_| {
        let is_wl = fetcher_sync()
            .map(|d| d.connection_type() == ConnectionType::Wireless2_4G)
            .unwrap_or(false);
        if is_wl != hw_flag_sync.get() {
            hw_flag_sync.set(is_wl);
            rebuild_hw_model(&hw_dd_sync, is_wl);
        }
    });

    // --- Обработчики ---------------------------------------------------------
    let fetcher2 = fetcher.clone();
    let vidpid_a = vidpid.clone();
    let status_a0 = live_status.clone();
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
        audio_low_entry,
        #[strong]
        audio_high_entry,
        #[strong]
        audio_sens_scale,
        #[strong]
        audio_style_dd,
        #[strong]
        frame_gen,
        #[strong]
        audio_stop,
        #[strong]
        light_off,
        #[strong]
        persist_switch,
        #[strong]
        schedule_save,
        #[strong]
        apply,
        #[strong]
        wl_allowed,
        #[strong]
        fetcher2,
        move |_| {
            if let Some(dev) = fetcher2() {
                // Свет включён host-эффектом/цветом — снимаем флаг «погашено» (10.31).
                light_off.set(false);
                // Любое новое «Применить» останавливает прежний аудио-поток
                // (иначе он продолжил бы гнать кадры поверх нового режима).
                if let Some(s) = audio_stop.borrow_mut().take() {
                    s.store(true, Ordering::Relaxed);
                }
                let mode = mode_from_index(mode_dd.selected());
                let base = Color::from_hex_checked(live_entry.text().trim())
                    .unwrap_or(Color::new(255, 90, 0));
                let brightness = bright_scale.value().round() as u8;
                let speed = speed_scale.value().round() as u8;
                // Ресивер 2.4G: эффект/цвет/яркость/скорость пишутся СРАЗУ во
                // флеш (протокол 0x13) — host-keepalive не нужен, эффект
                // держится без приложения. Запись выполняем в отдельном потоке
                // (persistent-хук через независимый handle), чтобы блокирующий
                // hidapi не вешал GUI, а результат читаем коротким опросом.
                if dev.connection_type() == ConnectionType::Wireless2_4G {
                    // Защита ресивера от потока записей (жалоба владельца
                    // 2026-11-16): не чаще раза в 2 с. Если рано — повторяем
                    // применение через 2.2 с.
                    if !wl_allowed() {
                        let apply_r = apply.clone();
                        status_a0.set_text(
                            "Ресивер занят (защита от частых записей) — повторю \
                             через пару секунд…",
                        );
                        gtk::glib::timeout_add_local(
                            std::time::Duration::from_millis(2200),
                            move || {
                                apply_r.emit_clicked();
                                glib::ControlFlow::Break
                            },
                        );
                        return;
                    }
                    let Some((vid, pid)) = vidpid_a() else {
                        status_a0.set_text(
                            "Устройство не подключено: откройте его на вкладке «Устройства»",
                        );
                        return;
                    };
                    frame_gen.set(frame_gen.get().wrapping_add(1));
                    let st = status_a0.clone();
                    let gen = frame_gen.clone();
                    let my_gen = gen.get();
                    let rx = spawn_wl_task(
                        vid,
                        pid,
                        WlTask::Rgb {
                            mode,
                            color: Some(base),
                            brightness,
                            speed,
                        },
                    );
                    // Опрос канала короткими тиками (одноразовый, по поколению).
                    gtk::glib::timeout_add_local(
                        std::time::Duration::from_millis(150),
                        move || {
                            if gen.get() != my_gen {
                                return glib::ControlFlow::Break;
                            }
                            match rx.try_recv() {
                                Ok(Ok(name)) => {
                                    // Донгл Ajazz (0C45:FDFD) — честное сообщение:
                                    // команда ушла, применение подтверждает владелец.
                                    if name.contains("Ajazz") {
                                        st.set_text(&format!(
                                            "2.4G (Ajazz): {mode:?}, яркость={brightness}, \
                                             скорость={speed} отправлены через донгл — \
                                             проверьте подсветку визуально"
                                        ));
                                    } else {
                                        st.set_text(&format!(
                                            "Ресивер 2.4G: {mode:?}, яркость={brightness}, \
                                             скорость={speed} сохранены в память — держатся \
                                             без приложения"
                                        ));
                                    }
                                    glib::ControlFlow::Break
                                }
                                Ok(Err(e)) => {
                                    st.set_text(&format!(
                                        "{e} (клавиатура спит на линке — нажмите клавишу и \
                                         повторите)"
                                    ));
                                    glib::ControlFlow::Break
                                }
                                Err(std::sync::mpsc::TryRecvError::Empty) => {
                                    glib::ControlFlow::Continue
                                }
                                Err(_) => {
                                    st.set_text("Сбой потока записи (канал закрыт)");
                                    glib::ControlFlow::Break
                                }
                            }
                        },
                    );
                    return;
                }
                // USB Ajazz (VID 0x0C45): подсветка применяется АППАРАТНО через
                // рабочий control-канал 0xFF13 (apply_rgb_persistent → Ok(true)),
                // host-рендеринг не нужен и не работает (per-key 0xFF68 мёртв,
                // журнал 10.114). Запись — в фоновом потоке, чтобы блокирующий
                // hidapi не вешал GUI (тот же механизм, что и у 2.4G).
                if dev.vid_pid().0 == 0x0C45 {
                    if mode == RgbMode::Audio {
                        status_a0.set_text(
                            "Аудио-реактив на Ajazz недоступен (per-key канал 0xFF68 не работает)",
                        );
                        return;
                    }
                    let Some((vid, pid)) = vidpid_a() else {
                        status_a0.set_text(
                            "Устройство не подключено: откройте его на вкладке «Устройства»",
                        );
                        return;
                    };
                    frame_gen.set(frame_gen.get().wrapping_add(1));
                    let st = status_a0.clone();
                    let gen = frame_gen.clone();
                    let my_gen = gen.get();
                    let rx = spawn_wl_task(
                        vid,
                        pid,
                        WlTask::Rgb {
                            mode,
                            color: Some(base),
                            brightness,
                            speed,
                        },
                    );
                    gtk::glib::timeout_add_local(
                        std::time::Duration::from_millis(150),
                        move || {
                            if gen.get() != my_gen {
                                return glib::ControlFlow::Break;
                            }
                            match rx.try_recv() {
                                Ok(Ok(_name)) => {
                                    st.set_text(&format!(
                                        "Ajazz: режим {mode:?} применён аппаратно \
                                         (канал 0xFF13) — держится без приложения"
                                    ));
                                    glib::ControlFlow::Break
                                }
                                Ok(Err(e)) => {
                                    st.set_text(&e);
                                    glib::ControlFlow::Break
                                }
                                Err(std::sync::mpsc::TryRecvError::Empty) => {
                                    glib::ControlFlow::Continue
                                }
                                Err(_) => {
                                    st.set_text("Сбой потока записи (канал закрыт)");
                                    glib::ControlFlow::Break
                                }
                            }
                        },
                    );
                    return;
                }
                // USB: host-rendering (аппаратной эффект-таблицы у F75/F99 нет,
                // latch не найден) — режим/яркость/скорость рендерим НА ХОСТЕ
                // и гоним кадрами через render_frame()-канал с keepalive.
                use crate::devices::aula_f75::{host_frame_with, HostEffectParams};
                let params = HostEffectParams {
                    mode,
                    base,
                    brightness,
                    speed,
                };
                let n_leds = dev.led_count();
                let n_cols = dev.led_cols();
                // Аудио-реактив (USB): фоновый поток читает монитор звука и
                // гонит кадры. В главном потоке блокирующий read монитора
                // подвесил бы GUI (в тишине данных нет) — поэтому отдельный
                // поток, остановка — stop-флаг (audio_stop).
if mode == RgbMode::Audio {
                    let Some((vid, pid)) = vidpid_a() else {
                        status_a0.set_text(
                            "Устройство не подключено: откройте его на вкладке «Устройства»",
                        );
                        return;
                    };
                    // Парсим аудио-цвета и чувствительность из GUI.
                    let low = Color::from_hex_checked(audio_low_entry.text().trim())
                        .unwrap_or(Color::new(51, 255, 51));
                    let high = Color::from_hex_checked(audio_high_entry.text().trim())
                        .unwrap_or(Color::new(51, 51, 255));
                    let sens = audio_sens_scale.value() as f32;
                    let style = audio_style_from_index(audio_style_dd.selected());
                    // Проверяем доступность монитора звука синхронно с параметрами.
                    if let Err(e) = crate::audio::AudioReactive::open_with(sens, speed) {
                        status_a0.set_text(&format!("Аудио-реактив недоступен: {e}"));
                        return;
                    }
                    // Останавливаем прежний host-таймер (иначе он продолжит
                    // гнать старый эффект поверх аудио — «эффект без музыки»).
                    frame_gen.set(frame_gen.get().wrapping_add(1));
                    let stop = Arc::new(AtomicBool::new(false));
                    *audio_stop.borrow_mut() = Some(stop.clone());
                    let active_leds = crate::devices::audio_active_row_leds(n_leds);
                    std::thread::spawn(move || {
                        let mut dev = match crate::devices::open_vidpid(vid, pid) {
                            Ok(d) => d,
                            Err(_) => return,
                        };
                        let geometry = crate::devices::audio_led_geometry(n_leds);
                        let mut au = match crate::audio::AudioReactive::open_with(sens, speed) {
                            Ok(a) => a,
                            Err(_) => return,
                        };
                        while !stop.load(Ordering::Relaxed) {
                            match au.read_vu_bands() {
                                Ok(vu) => {
                                    let frame = crate::audio::audio_frame(
                                        base, brightness, &vu, n_leds, active_leds, low, high, style, geometry,
                                    );
                                    if dev.render_frame(&frame).is_err() {
                                        // Возможен кик канала — переоткрываем.
                                        if let Ok(d) = crate::devices::open_vidpid(vid, pid) {
                                            dev = d;
                                        }
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    });
                    let style_name = match style {
                        AudioStyle::Row => "верхний ряд (Esc+F1..F12)",
                        AudioStyle::Bars => "столбики по колонкам",
                    };
                    status_a0.set_text(&format!(
                        "Аудио-реактив запущен (USB): {style_name} = спектр системного звука; \
                         остальные LED — базовый цвет. «Применить»/гашение останавливает поток."
                    ));
                    return;
                }
                // ВАЖНО (фикс 10.21): при повторном «Применить» НАДО
                // останавливать прежний keepalive-поток и запускать новый,
                // иначе новые режим/яркость/скорость не применятся (старый
                // таймер продолжал гнать старые кадры). Остановку делаем через
                // инкремент поколения — прежний таймер завершится сам на
                // следующем тике (удаление SourceId паникует, фикс 10.39).
                frame_gen.set(frame_gen.get().wrapping_add(1));
                let my_gen = frame_gen.get();
                let dev_fetch = fetcher2.clone();
                let gen_f = frame_gen.clone();
                let mut t_ms = 0u32;
                gtk::glib::timeout_add_local(
                    std::time::Duration::from_millis(40),
                    move || {
                        // Поколение устарело — была нажата другая кнопка/запущен
                        // новый поток: этот таймер больше не нужен, завершаемся.
                        if gen_f.get() != my_gen {
                            return glib::ControlFlow::Break;
                        }
                        let frame = host_frame_with(&params, t_ms, n_leds, n_cols);
                        t_ms += 40;
                        if let Some(dev) = dev_fetch() {
                            if dev.render_frame(&frame).is_err() {
                                return glib::ControlFlow::Break;
                            }
                        }
                        glib::ControlFlow::Continue
                    },
                );
                // Переключатель «Сохранить в клавиатуру»: на USB сохраняем
                // выбранный аппаратный эффект (или текущий конфиг) в память —
                // отложенно (1.5 с после последнего изменения), чтобы серия
                // правок дала одну запись, а не «кик» на каждый сигнал.
                if persist_switch.is_active() {
                    schedule_save();
                }
            }
        }
    ));

    let fetcher3 = fetcher.clone();
    let status_s = live_status.clone();
    let frame_gen_s = frame_gen.clone();
    let color_s = live_color.clone();
    let off_s = light_off.clone();
    let set_dev = set_device.clone();
    let vidpid_f = vidpid.clone();
    let hw_wl_save = hw_is_wl.clone();
    // Клоны слайдеров для move-замыкания сохранения: оригиналы нужны ниже
    // (колбэки страницы пресетов).
    let bright_save = bright_scale.clone();
    let speed_save = speed_scale.clone();
    save_to_board.connect_clicked(move |_| {
        // Фикс 10.27: СНАЧАЛА останавливаем host-поток кадров, иначе keepalive
        // (каждые 400 мс) перезапишет LED и «съест» сохранённый аппаратный
        // эффект/выключение. Остановка — через инкремент поколения (прежние
        // таймеры завершатся сами; удаление SourceId паникует, фикс 10.39).
        frame_gen_s.set(frame_gen_s.get().wrapping_add(1));
        color_s.borrow_mut().take();

        // Что сохраняем: аппаратный эффект (USB: селектор data[10]; ресивер:
        // led[10]) / выключенный свет (слои эффектов a=00) / текущий конфиг.
        // Тип выбираем ДО потока — здесь используются GUI-виджеты (Rc), их
        // нельзя уводить в thread. Список кодов — по текущей модели списка
        // «Аппаратный эффект» (hw_is_wl: ресивер vs USB).
        let sel = hw_dd.selected();
        let was_off = off_s.get();
        let is_wl = hw_wl_save.get();
        // Значения слайдеров яркости/скорости — для сохранения ВМЕСТЕ с эффектом
        // (ресивер пишет их в слот активного эффекта; журнал 2026-09-15).
        let bri = bright_save.value() as u8;
        let spd = speed_save.value() as u8;
        let effects: &[(u8, &str)] = if is_wl {
            crate::devices::aula_f75::WL_EFFECTS
        } else {
            crate::devices::aula_f75::HW_EFFECTS
        };
        let kind = if sel > 0 && (sel as usize) <= effects.len() {
            SaveKind::Effect(effects[(sel - 1) as usize].0)
        } else if was_off {
            SaveKind::LightOff
        } else {
            SaveKind::Config
        };

        let Some((vid, pid)) = vidpid_f() else {
            status_s.set_text("Устройство не подключено: откройте его на вкладке «Устройства»");
            return;
        };

        // Ресивер 2.4G: если не выбран эффект и свет не погашен — сохранять
        // нечего (0x13 не пишет «текущий конфиг», в отличие от USB data[10]).
        if is_wl && matches!(kind, SaveKind::Config) {
            status_s.set_text(
                "Ресивер 2.4G: выберите аппаратный эффект (или «Выключить подсветку»), \
                 а затем «Применить» — запись текущего конфига на ресивере \
                 не поддерживается.",
            );
            frame_gen_s.set(frame_gen_s.get().wrapping_add(1));
            return;
        }

        // Фикс 10.34: запись в память содержит блокирующие hidapi-вызовы
        // (read_config + write_config + кик клавиатуры) — выполняем их в
        // ОТДЕЛЬНОМ потоке через независимый handle (open_vidpid), чтобы
        // главный GTK-поток не зависал. GUI-слот не трогаем.
        status_s.set_text("Запись в память… клавиатура может переподключиться");

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let res: Result<String, String> = (|| {
                let dev = crate::devices::open_vidpid(vid, pid)
                    .map_err(|e| format!("не удалось открыть устройство: {e}"))?;
                match kind {
                    SaveKind::Effect(code) => {
                        // Слайдеры яркости/скорости применяем вместе с эффектом
                        // (для ресивера — слот активного эффекта в LED config;
                        // для USB — слот эффект-таблицы).
                        dev.save_effect_with_style(code, bri, spd)
                            .map_err(|e| e.to_string())?;
                    }
                    SaveKind::LightOff => {
                        dev.save_hardware_light(false).map_err(|e| e.to_string())?
                    }
                    SaveKind::Config => {
                        dev.save_profile_to_eeprom(0).map_err(|e| e.to_string())?
                    }
                }
                Ok(dev.name().to_string())
            })();
            let _ = tx.send(res);
        });

        // Ждём результат из потока, не блокируя UI (одноразовый таймер опрашивает
        // mpsc-канал короткими паузами). Таймер живёт по поколению: нажатие любой
        // другой кнопки инкрементит frame_gen — прежний опрос/keepalive/reconnect
        // завершатся сами на следующем тике (без паникующего SourceId::remove).
        let st = status_s.clone();
        let gen_r = frame_gen_s.clone();
        let dev_fetch = fetcher3.clone();
        let need_off = was_off && sel == 0;
        // Отдельный клон set_device для вложенных timer-замыканий: Rc не Copy,
        // move-внутрь таймера нельзя прямо из внешнего Fn-closeure (E0507).
        // Каждый вызов внешнего клика даёт свежий клон — внутренние таймеры
        // получают собственный Rc и не «съедают» внешний.
        let sd = set_dev.clone();
        let my_gen = gen_r.get();
        gtk::glib::timeout_add_local(
            std::time::Duration::from_millis(150),
            move || {
                // Поколение устарело — была нажата другая кнопка: останавливаем
                // и текущий опрос, и всё, что он мог запустить ниже.
                if gen_r.get() != my_gen {
                    return glib::ControlFlow::Break;
                }
                let res = match rx.try_recv() {
                    Ok(r) => r,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        return glib::ControlFlow::Continue
                    }
                    Err(_) => {
                        st.set_text("Сбой потока записи (канал закрыт)");
                        return glib::ControlFlow::Break;
                    }
                };
                match res {
                    Err(e) => {
                        st.set_text(&format!("Ошибка записи в память: {e}"));
                        return glib::ControlFlow::Break;
                    }
                    Ok(_name) => {
                        // Запись успешна. write кикает клавиатуру (пересоздание
                        // hidraw) — старый handle мёртв, новые узлы появляются
                        // ~1–2 с. 
                        if need_off {
                            // Свет сохраняется погашенным: включаем ПОСТОЯННЫЙ
                            // чёрный keepalive с авто-восстановлением канала —
                            // каждый тик шлёт чёрный кадр; при ошибке (канал
                            // пересоздаётся/открылся не тот интерфейс) сразу
                            // переоткрываем устройство и продолжаем. Пока ПК и
                            // приложение открыты — свет не вернётся. Флеш
                            // (a=00 во всех слотах данных[58..96]) применится
                            // сам при полном отключении.
                            st.set_text(
                                "Свет выключен и сохранён в память. Удерживаю гашение \
                                 авто-переподключением… ВАЖНО: клавиатура применяет \
                                 сохранённый конфиг только при полном отключении от \
                                 питания (отключите USB на пару секунд/перезагрузите ПК) \
                                 — тогда свет не включится и без приложения.",
                            );
                            let fetch_k = dev_fetch.clone();
                            let set_k = sd.clone();
                            let rx_store = Rc::new(RefCell::new(None));
                            let rx_s = rx_store.clone();
                            let gen_k = gen_r.clone();
                            gtk::glib::timeout_add_local(
                                std::time::Duration::from_millis(400),
                                move || {
                                    if gen_k.get() != my_gen {
                                        return glib::ControlFlow::Break;
                                    }
                                    // Фоновое переподключение: забираем готовый
                                    // HID-мост и кладём живое устройство в слот.
                                    drain_reopen(&rx_s, &*set_k);
                                    match fetch_k() {
                                        Some(dev) => {
                                            // Чёрный кадр строим по размеру матрицы
                                            // устройства (F75=90, F99=113) — хардкод
                                            // убран при параметризации 10.55.
                                            let black =
                                                vec![Color::new(0, 0, 0); dev.led_count()];
                                            if dev.render_frame(&black).is_err() {
                                                // канал переживает пересоздание либо
                                                // открылся не тот интерфейс — фон
                                                // сам переоткроет и гаснуть продолжим
                                                ensure_reopen(&rx_s, vid, pid);
                                            }
                                        }
                                        None => ensure_reopen(&rx_s, vid, pid),
                                    }
                                    glib::ControlFlow::Continue
                                },
                            );
                        } else {
                            // Эффект/конфиг (свет вкл): переподключение через
                            // фоновый поток — блокирующее hidapi в главный поток
                            // не попадает, UI не вешается (фикс 2026-09-13).
                            st.set_text(
                                "Конфиг записан в память (эффект/свет вкл после \
                                 переподключения клавиатуры). Переподключаюсь…",
                            );
                            let set_k = sd.clone();
                            let rx_store = Rc::new(RefCell::new(None));
                            ensure_reopen(&rx_store, vid, pid);
                            let rx_s = rx_store.clone();
                            let st_k = st.clone();
                            let attempts = Rc::new(Cell::new(0u32));
                            let gen_r2 = gen_r.clone();
                            gtk::glib::timeout_add_local(
                                std::time::Duration::from_millis(600),
                                move || {
                                    if gen_r2.get() != my_gen {
                                        return glib::ControlFlow::Break;
                                    }
                                    if attempts.get() >= 20 {
                                        st_k.set_text(
                                            "Конфиг записан в память. Переподключение не \
                                             удалось — нажмите «Устройства → Открыть».",
                                        );
                                        return glib::ControlFlow::Break;
                                    }
                                    attempts.set(attempts.get() + 1);
                                    match drain_reopen(&rx_s, &*set_k) {
                                        Some(_name) => {
                                            st_k.set_text(
                                                "Конфиг записан в память клавиатуры. \
                                                 Эффект/свет включится сам при старте.",
                                            );
                                            glib::ControlFlow::Break
                                        }
                                        None => glib::ControlFlow::Continue,
                                    }
                                },
                            );
                        }
                        if sel == 0 && !was_off {
                            st.set_text(
                                "Записан текущий аппаратный конфиг (свет вкл). ВАЖНО: \
                                 статические цвета и host-эффекты в память не сохраняются \
                                 — они работают, пока приложение открыто.",
                            );
                        }
                        glib::ControlFlow::Break
                    }
                }
            },
        );
    });

    // --- Живой цвет: обработчики ---------------------------------------
    // Размер кадра берём из устройства (F75=90, F99=113) — хардкод
    // RGB_LED_COUNT убран при параметризации (журнал 10.55).

    // Общий «включить текущий цвет из поля ввода» (палитра и кнопка).
    let apply_cur: Rc<dyn Fn()> = {
        let entry_a = live_entry.clone();
        let fetcher_a = fetcher.clone();
        let gen_c = frame_gen.clone();
        let color_a = live_color.clone();
        let status_a = live_status.clone();
        let off_a = light_off.clone();
        let vidpid_c = vidpid.clone();
        let wl_a = wl_allowed.clone();
        let sched_a = schedule_apply.clone();
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
            // Останавливаем предыдущий поток через инкремент поколения — прежний
            // keepalive завершится сам на следующем тике (удаление SourceId
            // паникует в glib-0.20.12, фикс 10.39).
            gen_c.set(gen_c.get().wrapping_add(1));
            let my_gen = gen_c.get();
            // Ресивер 2.4G: статичный цвет пишется СРАЗУ в память (0x13) — без
            // host-keepalive, держится без приложения. Запись — отдельным
            // потоком (persistent-хук), результат — коротким опросом.
            if dev.connection_type() == ConnectionType::Wireless2_4G {
                // Защита ресивера от потока записей (см. wl_allowed): не чаще
                // раза в 2 с; если рано — повторяем через пару секунд.
                if !wl_a() {
                    let sched = sched_a.clone();
                    status_a.set_text(
                        "Ресивер занят (защита от частых записей) — повторю \
                         через пару секунд…",
                    );
                    gtk::glib::timeout_add_local(std::time::Duration::from_millis(2200), move || {
                        sched();
                        glib::ControlFlow::Break
                    });
                    return;
                }
                let Some((vid, pid)) = vidpid_c() else {
                    status_a.set_text(
                        "Устройство не подключено: откройте его на вкладке «Устройства»",
                    );
                    return;
                };
                let rx = spawn_wl_task(
                    vid,
                    pid,
                    WlTask::Rgb {
                        mode: RgbMode::Static,
                        color: Some(color),
                        brightness: 255,
                        speed: 1,
                    },
                );
                let st = status_a.clone();
                let gen = gen_c.clone();
                let my_gen = gen.get();
                gtk::glib::timeout_add_local(
                    std::time::Duration::from_millis(150),
                    move || {
                        if gen.get() != my_gen {
                            return glib::ControlFlow::Break;
                        }
                        match rx.try_recv() {
                            Ok(Ok(_name)) => {
                                st.set_text(&format!(
                                    "Ресивер 2.4G: статичный цвет #{:06X} сохранён в память \
                                     (держится без приложения)",
                                    color.to_argb() & 0xFF_FFFF
                                ));
                                glib::ControlFlow::Break
                            }
                            Ok(Err(e)) => {
                                st.set_text(&format!(
                                    "{e} (клавиатура спит на линке — нажмите клавишу и \
                                     повторите)"
                                ));
                                glib::ControlFlow::Break
                            }
                            Err(std::sync::mpsc::TryRecvError::Empty) => {
                                glib::ControlFlow::Continue
                            }
                            Err(_) => {
                                st.set_text("Сбой потока записи (канал закрыт)");
                                glib::ControlFlow::Break
                            }
                        }
                    },
                );
                return;
            }
            let n_leds = dev.led_count();
            let frame = vec![color; n_leds];
            if let Err(e) = dev.render_frame(&frame) {
                status_a.set_text(&format!("Ошибка отправки кадра: {e}"));
                return;
            }
            *color_a.borrow_mut() = Some(color);
            status_a.set_text(&format!("Кадр #{:06X} отправлен ({n_leds} LED)…", color.to_argb() & 0xFF_FFFF));
            // Запускаем keepalive: поток кадров каждые 400 мс.
            let gen_k = gen_c.clone();
            let color_b = color_a.clone();
            let dev_fetch = fetcher_a.clone();
            gtk::glib::timeout_add_local(
                std::time::Duration::from_millis(400),
                move || {
                    if gen_k.get() != my_gen {
                        return glib::ControlFlow::Break;
                    }
                    let Some(c) = color_b.borrow().as_ref().copied() else {
                        return glib::ControlFlow::Break;
                    };
                    let Some(dev) = dev_fetch() else {
                        return glib::ControlFlow::Break;
                    };
                    if dev.render_frame(&vec![c; dev.led_count()]).is_err() {
                        return glib::ControlFlow::Break;
                    }
                    glib::ControlFlow::Continue
                },
            );
        })
    };

    let apply_cur2 = apply_cur.clone();
    live_on.connect_clicked(move |_| apply_cur2());

    let gen_off = frame_gen.clone();
    let color_c = live_color.clone();
    let fetcher6 = fetcher.clone();
    let status_off = live_status.clone();
    let off_c = light_off.clone();
    let hw_dd_off = hw_dd_for_off.clone();
    let vidpid_off = vidpid.clone();
    let audio_stop_off = audio_stop.clone();
    live_off.connect_clicked(move |_| {
        // Свет погашен host-кадром — запоминаем (10.31): «Записать в память»
        // без выбранного эффекта сохранит свет ВЫКЛЮЧЕННЫМ (a=00 во всех слотах
        // эффектов data[58..96]).
        off_c.set(true);
        // Останавливаем и аудио-поток, если он бежал (иначе он перезажжёт свет).
        if let Some(s) = audio_stop_off.borrow_mut().take() {
            s.store(true, Ordering::Relaxed);
        }
        // Гашение = чистое «выключено». Сбрасываем выбор аппаратного эффекта,
        // иначе «Записать в память» сохранит эффект со светом вкл (все a=09)
        // и свет вернётся после перезагрузки клавиатуры (фикс 10.35).
        hw_dd_off.set_selected(0);
        // Останавливаем host-поток (иначе он же и перезажжёт свет) через
        // инкремент поколения — прежний keepalive завершится сам (10.39).
        gen_off.set(gen_off.get().wrapping_add(1));
        let my_gen = gen_off.get();
        color_c.borrow_mut().take();
        let Some(dev) = fetcher6() else {
            status_off.set_text("Устройство не подключено: откройте его на вкладке «Устройства»");
            return;
        };
        // Ресивер 2.4G: свет выключается АППАРАТНО в память (яркость LED
        // config = 0, протокол 0x13) — держится без приложения, host-keepalive
        // не нужен. Запись — отдельным потоком (persistent-хук), результат —
        // коротким опросом.
        if dev.connection_type() == ConnectionType::Wireless2_4G {
            let Some((vid, pid)) = vidpid_off() else {
                status_off.set_text(
                    "Устройство не подключено: откройте его на вкладке «Устройства»",
                );
                return;
            };
            let rx = spawn_wl_task(vid, pid, WlTask::Light { on: false });
            let st = status_off.clone();
            let gen = gen_off.clone();
            let my_gen = gen.get();
            gtk::glib::timeout_add_local(
                std::time::Duration::from_millis(150),
                move || {
                    if gen.get() != my_gen {
                        return glib::ControlFlow::Break;
                    }
                    match rx.try_recv() {
                        Ok(Ok(_name)) => {
                            st.set_text(
                                "Ресивер 2.4G: свет ВЫКЛЮЧЕН и сохранён в память (держится \
                                 без приложения). Включить обратно: «Применить» или \
                                 «Включить цвет».",
                            );
                            glib::ControlFlow::Break
                        }
                        Ok(Err(e)) => {
                            st.set_text(&format!(
                                "{e} (клавиатура спит на линке — нажмите клавишу и \
                                 повторите)"
                            ));
                            glib::ControlFlow::Break
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                        Err(_) => {
                            st.set_text("Сбой потока записи (канал закрыт)");
                            glib::ControlFlow::Break
                        }
                    }
                },
            );
            return;
        }
        // Чёрный кадр гасит все LED сейчас.
        let black = vec![Color::new(0, 0, 0); dev.led_count()];
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
        let gen_k = gen_off.clone();
        let dev_fetch = fetcher6.clone();
        let black_ka = black.clone();
        gtk::glib::timeout_add_local(
            std::time::Duration::from_millis(400),
            move || {
                if gen_k.get() != my_gen {
                    return glib::ControlFlow::Break;
                }
                let Some(dev) = dev_fetch() else {
                    return glib::ControlFlow::Break;
                };
                // Кадр строим по актуальному размеру матрицы устройства.
                let black = if black_ka.len() == dev.led_count() {
                    black_ka.clone()
                } else {
                    vec![Color::new(0, 0, 0); dev.led_count()]
                };
                if dev.render_frame(&black).is_err() {
                    return glib::ControlFlow::Break;
                }
                glib::ControlFlow::Continue
            },
        );
        status_off.set_text(
            "Подсветка выключена (не включится, пока приложение открыто). Выбор \
             аппаратного эффекта сброшен — при закрытии приложения состояние «свет \
             выключен» сохранится автоматически (а=00 во всех слотах эффектов). \
             Хотите эффект в память — выберите его в списке «Эффект для памяти \
             клавиатуры»: при включённом «Сохранить в клавиатуру» он сохранится \
             автоматически.",
        );
    });

    // --- Probe для автосохранения при закрытии окна (app.rs) ---------------
    // Возвращает действие, которое нужно зафиксировать во флеш, если пользователь
    // закрыл окно: свет выключен (а=00 во всех слотах) или выбран аппаратный
    // эффект (data[10] + свет вкл). Ничего не возвращает, если нечего
    // сохранять (свет горит и эффект не выбирался — менять память не нужно).
    let probe_hw = hw_dd_for_probe.clone();
    let probe_off = light_off.clone();
    let probe_wl = hw_is_wl.clone();
    let probe: PersistProbe = Rc::new(move || {
        if probe_off.get() {
            return Some(SaveKind::LightOff);
        }
        let sel = probe_hw.selected();
        // Список кодов эффектов соответствует текущей модели списка
        // (ресивер = WL_EFFECTS/led[10], USB = HW_EFFECTS/data[10]).
        let effects: &[(u8, &str)] = if probe_wl.get() {
            crate::devices::aula_f75::WL_EFFECTS
        } else {
            crate::devices::aula_f75::HW_EFFECTS
        };
        if sel > 0 && (sel as usize) <= effects.len() {
            return Some(SaveKind::Effect(effects[(sel - 1) as usize].0));
        }
        None
    });

    // --- Колбэки для страницы «Расширенные настройки пресетов» --------------
    // Страница пресетов выставляет настройки «Подсветки» и применяет их
    // (apply), а при сохранении берёт текущие значения (current).
    use crate::presets::AudioParams;
    use crate::types::RgbMode;
    let preset_callbacks = super::presets_page::PresetCallbacks {
        apply: {
            let mode_dd_c = mode_dd.clone();
            let bright_c = bright_scale.clone();
            let speed_c = speed_scale.clone();
            let entry_c = live_entry.clone();
            let sync_c = sync_color_ui.clone();
            let audio_sens_c = audio_sens_scale.clone();
            let audio_low_c = audio_low_entry.clone();
            let audio_high_c = audio_high_entry.clone();
            let audio_style_c = audio_style_dd.clone();
            let apply_c = apply.clone();
            Rc::new(move |mode: RgbMode, brightness: u8, speed: u8, color: String, audio: Option<AudioParams>| {
                mode_dd_c.set_selected(mode_to_index(mode));
                bright_c.set_value(brightness as f64);
                speed_c.set_value(speed as f64);
                entry_c.set_text(&color);
                sync_c(&color);
                if let Some(a) = audio {
                    audio_sens_c.set_value(a.sensitivity as f64);
                    audio_low_c.set_text(&a.color_low);
                    audio_high_c.set_text(&a.color_high);
                    audio_style_c.set_selected(audio_style_to_index(a.style));
                }
                // Авто-применение: сигналы уже сработали бы на изменениях, но
                // если значения совпали с текущими — применяем явно.
                apply_c.emit_clicked();
            })
        },
        current: {
            let mode_dd_c = mode_dd.clone();
            let bright_c = bright_scale.clone();
            let speed_c = speed_scale.clone();
            let entry_c = live_entry.clone();
            let audio_sens_c = audio_sens_scale.clone();
            let audio_low_c = audio_low_entry.clone();
            let audio_high_c = audio_high_entry.clone();
            let audio_style_c = audio_style_dd.clone();
            Rc::new(move || {
                let mode = mode_from_index(mode_dd_c.selected());
                let audio = if mode == RgbMode::Audio {
                    Some(AudioParams {
                        sensitivity: audio_sens_c.value() as f32,
                        color_low: audio_low_c.text().trim().to_string(),
                        color_high: audio_high_c.text().trim().to_string(),
                        style: audio_style_from_index(audio_style_c.selected()),
                    })
                } else {
                    None
                };
                (
                    mode,
                    bright_c.value().round() as u8,
                    speed_c.value().round() as u8,
                    entry_c.text().trim().to_string(),
                    audio,
                )
            })
        },
    };

    (panel, probe, preset_callbacks)
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