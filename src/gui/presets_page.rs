// --- Расширенные настройки пресетов --------------------------------------
// Отдельная страница: быстрые пресеты + файловые TOML-пресеты
// (`~/.config/keyboard-center/presets`, те же, что у `kbc preset`).
// Работает через колбэки на страницу «Подсветка»: пресеты выставляют
// настройки подсветки и применяют их, сохранение берёт текущие значения.

use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::presets::{AudioParams, ModeParams, Preset, PresetStore};
use crate::types::RgbMode;

/// Мост между страницей пресетов и страницей «Подсветка».
pub struct PresetCallbacks {
    /// Применить набор настроек к клавиатуре (выставляет элементы «Подсветки»
    /// и запускает авто-применение).
    pub apply: Rc<dyn Fn(RgbMode, u8, u8, String, Option<AudioParams>)>,
    /// Текущие настройки «Подсветки»: (режим, яркость, скорость, цвет, аудио).
    pub current: Rc<dyn Fn() -> (RgbMode, u8, u8, String, Option<AudioParams>)>,
}

/// Страница «Расширенные настройки пресетов».
pub fn build_presets_page(cb: PresetCallbacks) -> gtk::Box {
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 6);
    panel.set_margin_top(8);
    panel.set_margin_bottom(8);
    panel.set_margin_start(8);
    panel.set_margin_end(8);

    let title = gtk::Label::new(Some("Расширенные настройки пресетов"));
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Start);
    panel.append(&title);

    let hint = gtk::Label::new(Some(
        "Быстрые пресеты сразу применяют готовую комбинацию эффекта и цвета. \
         Свои комбинации сохраняйте в «Мои пресеты» — они хранятся на компьютере \
         и работают в любой установке Keyboard-Center.",
    ));
    hint.add_css_class("dim-label");
    hint.set_wrap(true);
    hint.set_halign(gtk::Align::Start);
    panel.append(&hint);

    // --- Быстрые пресеты --------------------------------------------------
    let quick_label = gtk::Label::new(Some("Быстрые пресеты"));
    quick_label.add_css_class("heading");
    quick_label.set_halign(gtk::Align::Start);
    panel.append(&quick_label);

    let quick_dd = gtk::DropDown::builder().halign(gtk::Align::Start).build();
    quick_dd.set_model(Some(&gtk::StringList::new(&[
        "— (не применять)",
        "Статика (белая)",
        "Дыхание (голубое)",
        "Волна (оранжевая)",
        "Спектр (розовый)",
        "Дождь (светло-голубой)",
    ])));
    quick_dd.set_selected(0);
    panel.append(&quick_dd);

    let quick_note = gtk::Label::new(Some(
        "Выбор пункта сразу применяется к подключённой клавиатуре.",
    ));
    quick_note.add_css_class("dim-label");
    quick_note.set_halign(gtk::Align::Start);
    panel.append(&quick_note);

    // --- Мои пресеты ------------------------------------------------------
    let pf_label = gtk::Label::new(Some("Мои пресеты"));
    pf_label.add_css_class("heading");
    pf_label.set_halign(gtk::Align::Start);
    panel.append(&pf_label);

    let pf_dd = gtk::DropDown::builder().halign(gtk::Align::Start).build();
    // Модель держим отдельным Rc: gtk::DropDown не отдаёт содержимое своей
    // модели, а по индексу выбранного пресета нам нужно получить его имя.
    let pf_model: Rc<RefCell<Option<gtk::StringList>>> = Rc::new(RefCell::new(None));

    let pf_entry = gtk::Entry::builder()
        .placeholder_text("имя нового пресета")
        .halign(gtk::Align::Start)
        .width_chars(20)
        .build();

    let pf_apply = gtk::Button::with_label("Применить пресет");
    pf_apply.add_css_class("suggested-action");
    pf_apply.set_halign(gtk::Align::Start);

    let pf_save_btn = gtk::Button::with_label("Сохранить как…");
    pf_save_btn.set_halign(gtk::Align::Start);

    let pf_delete = gtk::Button::with_label("Удалить пресет");
    pf_delete.set_halign(gtk::Align::Start);

    let pf_refresh = gtk::Button::with_label("Обновить");
    pf_refresh.set_halign(gtk::Align::Start);

    let pf_note = gtk::Label::new(Some(&format!(
        "Хранилище: {} (создаётся кнопкой «Сохранить как…» или в терминале: \
         `kbc preset save <имя>`)",
        PresetStore::default_dir().display()
    )));
    pf_note.add_css_class("dim-label");
    pf_note.set_halign(gtk::Align::Start);
    pf_note.set_wrap(true);
    panel.append(&pf_note);

    let status = gtk::Label::new(Some(
        "Пресеты применяются к клавиатуре, открытой на вкладке «Устройства».",
    ));
    status.add_css_class("dim-label");
    status.set_halign(gtk::Align::Start);
    status.set_wrap(true);

    // Перечитать список пресетов из хранилища и перестроить dropdown.
    fn refresh_presets(
        dd: &gtk::DropDown,
        store: &Rc<RefCell<Option<gtk::StringList>>>,
    ) -> gtk::StringList {
        let names = PresetStore::open().list();
        let names_s: Vec<&str> = names.iter().map(String::as_str).collect();
        let model = gtk::StringList::new(&names_s);
        let prev = dd.selected() as usize;
        let sel = if names.is_empty() || prev >= names.len() {
            0
        } else {
            prev
        };
        *store.borrow_mut() = Some(model.clone());
        dd.set_model(Some(&model));
        if !names.is_empty() {
            dd.set_selected(sel as u32);
        }
        model
    }
    // Первичное заполнение списка при открытии страницы.
    let _ = refresh_presets(&pf_dd, &pf_model);

    panel.append(&pf_dd);
    panel.append(&pf_entry);
    panel.append(&pf_apply);
    panel.append(&pf_save_btn);
    panel.append(&pf_delete);
    panel.append(&pf_refresh);
    panel.append(&status);

    // --- Обработчики --------------------------------------------------------

    // Быстрые пресеты: выбор сразу применяет готовую комбинацию.
    {
        let cb_apply = cb.apply.clone();
        let cb_current = cb.current.clone();
        let status_q = status.clone();
        quick_dd.connect_selected_notify(move |dd| {
            // (режим, базовый цвет) по выбору пресета:
            let sel: Option<(RgbMode, &str)> = match dd.selected() {
                1 => Some((RgbMode::Static, "#FFFFFF")),
                2 => Some((RgbMode::Breathing, "#00A8FF")),
                3 => Some((RgbMode::Wave, "#FF8000")),
                4 => Some((RgbMode::SpectrumCycle, "#FF0066")),
                5 => Some((RgbMode::Rain, "#78C8FF")),
                _ => None, // «—»: ничего не запускаем
            };
            let Some((mode, hex)) = sel else {
                return;
            };
            let (_m, brightness, speed, _color, audio) = (cb_current)();
            (cb_apply)(mode, brightness, speed, hex.to_string(), audio);
            status_q.set_text(&format!(
                "Быстрый пресет применён: {}",
                crate::presets::mode_name(mode)
            ));
        });
    }

    // Мои пресеты: применение.
    {
        let pf_dd_c = pf_dd.clone();
        let pf_model_c = pf_model.clone();
        let cb_apply = cb.apply.clone();
        let status_c = status.clone();
        pf_apply.connect_clicked(move |_| {
            let idx = pf_dd_c.selected();
            let name = pf_model_c
                .borrow()
                .as_ref()
                .and_then(|m| m.string(idx))
                .map(|g| g.to_string())
                .unwrap_or_default();
            if name.trim().is_empty() {
                status_c.set_text("Пресет не выбран — выберите пресет в списке");
                return;
            }
            match PresetStore::open().load(&name) {
                Ok(preset) => {
                    let audio = if preset.params.r#type == RgbMode::Audio {
                        Some(preset.params.audio_params())
                    } else {
                        None
                    };
                    (cb_apply)(
                        preset.params.r#type,
                        preset.params.brightness,
                        preset.params.speed,
                        preset.params.color.clone(),
                        audio,
                    );
                    status_c.set_text(&format!(
                        "Применён пресет «{}» ({}): цвет {}, яркость {}, скорость {}",
                        preset.name,
                        crate::presets::mode_name(preset.params.r#type),
                        preset.params.color,
                        preset.params.brightness,
                        preset.params.speed,
                    ));
                }
                Err(e) => status_c.set_text(&format!("Не удалось применить пресет: {e}")),
            }
        });
    }

    // Мои пресеты: сохранение.
    {
        let pf_dd_c = pf_dd.clone();
        let pf_model_c = pf_model.clone();
        let pf_entry_c = pf_entry.clone();
        let cb_current = cb.current.clone();
        let status_c = status.clone();
        pf_save_btn.connect_clicked(move |_| {
            let raw = pf_entry_c.text().trim().to_string();
            let name = crate::presets::sanitize(&raw);
            if name == crate::presets::DEFAULT_NAME && !raw.is_empty() {
                status_c.set_text(&format!(
                    "Имя «{raw}» ненормализуемое — пресет будет сохранён как «default»"
                ));
                return;
            }
            let (mode, brightness, speed, color, audio) = (cb_current)();
            let params = ModeParams {
                version: crate::presets::PRESET_FORMAT_VERSION,
                r#type: mode,
                color: if color.is_empty() {
                    "#FF5A00".to_string()
                } else {
                    color
                },
                brightness,
                speed,
                frame_ms: crate::presets::DEFAULT_FRAME_MS,
                audio,
            };
            let preset = Preset { name, params };
            match PresetStore::open().save(&preset) {
                Ok(path) => {
                    let _ = refresh_presets(&pf_dd_c, &pf_model_c);
                    if let Some(model) = pf_model_c.borrow().as_ref() {
                        for i in 0..model.n_items() {
                            if model
                                .string(i)
                                .map(|g| g.as_str() == preset.name)
                                .unwrap_or(false)
                            {
                                pf_dd_c.set_selected(i);
                                break;
                            }
                        }
                    }
                    status_c.set_text(&format!(
                        "Пресет «{}» сохранён: {}",
                        preset.name,
                        path.display()
                    ));
                }
                Err(e) => status_c.set_text(&format!("Не удалось сохранить пресет: {e}")),
            }
        });
    }

    // Мои пресеты: удаление.
    {
        let pf_dd_c = pf_dd.clone();
        let pf_model_c = pf_model.clone();
        let status_c = status.clone();
        pf_delete.connect_clicked(move |_| {
            let idx = pf_dd_c.selected();
            let name = pf_model_c
                .borrow()
                .as_ref()
                .and_then(|m| m.string(idx))
                .map(|g| g.to_string())
                .unwrap_or_default();
            if name.trim().is_empty() {
                status_c.set_text("Пресет не выбран — выберите пресет в списке");
                return;
            }
            if name == crate::presets::DEFAULT_NAME {
                status_c.set_text("Пресет «default» удалить нельзя");
                return;
            }
            if crate::presets::is_builtin(&name) && !PresetStore::open().path(&name).is_file() {
                status_c.set_text(&format!(
                    "Пресет «{name}» встроенный — удалить нельзя (можно переопределить, сохранив свой)"
                ));
                return;
            }
            match PresetStore::open().remove(&name) {
                Ok(()) => {
                    let _ = refresh_presets(&pf_dd_c, &pf_model_c);
                    status_c.set_text(&format!("Пресет «{name}» удалён"));
                }
                Err(e) => status_c.set_text(&format!("Не удалось удалить пресет: {e}")),
            }
        });
    }

    // Мои пресеты: обновление списка.
    {
        let pf_dd_c = pf_dd.clone();
        let pf_model_c = pf_model.clone();
        let status_c = status.clone();
        pf_refresh.connect_clicked(move |_| {
            let model = refresh_presets(&pf_dd_c, &pf_model_c);
            if model.n_items() == 0 {
                status_c.set_text("Пресеты не найдены — создайте через «Сохранить как…» или `kbc preset save`");
            } else {
                status_c.set_text(&format!("Список пресетов обновлён ({} шт.)", model.n_items()));
            }
        });
    }

    panel
}
