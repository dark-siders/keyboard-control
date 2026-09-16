// --- Screen Manager (Ajazz AK820 Pro) ---
// Предпросмотр и загрузка картинок/GIF на TFT-экран клавиатуры.
//
// Превью рисуем через Cairo из сохранённого RgbaImage; прошивка выполняется
// асинхронно, чтобы не блокировать UI при записи в HID-устройство.

use gtk::cairo;
use gtk::glib;
use gtk::glib::clone;
use gtk::prelude::*;

use std::cell::RefCell;
use std::rc::Rc;

use crate::devices::KeyboardDevice;
use crate::screen::converter::{self, ConvertedAnimation};

/// Состояние превью: сконвертированная анимация.
type AnimState = Rc<RefCell<Option<ConvertedAnimation>>>;

/// Создать панель Screen Manager.
pub fn build_screen_manager(
    device_fetcher: impl Fn() -> Option<Rc<dyn KeyboardDevice>> + 'static,
) -> gtk::Box {
    let fetcher: Rc<dyn Fn() -> Option<Rc<dyn KeyboardDevice>>> = Rc::new(device_fetcher);

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 6);
    panel.set_margin_top(8);
    panel.set_margin_bottom(8);

    // Заголовок
    let title = gtk::Label::new(Some("Screen Manager — Ajazz AK820 Pro"));
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Start);
    panel.append(&title);

    let subtitle = gtk::Label::new(Some("Загрузите изображение или GIF-анимацию для TFT-экрана 0.85\" (128×128)"));
    subtitle.add_css_class("dim-label");
    subtitle.set_halign(gtk::Align::Start);
    panel.append(&subtitle);

    // Кнопка выбора файла
    let file_btn = gtk::Button::with_label("Выбрать изображение…");
    file_btn.add_css_class("suggested-action");
    file_btn.set_halign(gtk::Align::Start);

    // Превью
    let preview = gtk::DrawingArea::builder()
        .width_request(128 * 3)
        .height_request(128 * 3)
        .hexpand(true)
        .vexpand(false)
        .build();
    preview.add_css_class("card");

    // Состояние превью (RgbaImage) — для draw_func
    let preview_img: Rc<RefCell<Option<image::DynamicImage>>> = Rc::new(RefCell::new(None));
    let anim_state: AnimState = Rc::new(RefCell::new(None));

    let img_for_draw = preview_img.clone();
    preview.set_draw_func(move |_, ctx, w, h| {
        if let Some(img) = &*img_for_draw.borrow() {
            draw_preview(ctx, img, w as f64, h as f64);
        }
    });

    // Информация
    let info_label = gtk::Label::new(Some("Файл не выбран"));
    info_label.add_css_class("dim-label");
    info_label.set_wrap(true);
    info_label.set_halign(gtk::Align::Start);

    let anim_label = gtk::Label::new(Some(""));
    anim_label.add_css_class("caption");
    anim_label.set_halign(gtk::Align::Start);

    // Кнопки
    let actions_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let send_btn = gtk::Button::with_label("Прошить на экран");
    send_btn.add_css_class("suggested-action");
    send_btn.set_sensitive(false);
    let time_btn = gtk::Button::with_label("Синхронизировать время");
    time_btn.set_tooltip_text(Some("Установить время/дату ПК на экран клавиатуры"));
    let brightness_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 255.0, 1.0);
    brightness_scale.set_value(200.0);
    brightness_scale.set_width_request(180);

    actions_row.append(&send_btn);
    actions_row.append(&time_btn);

    // --- Сборка ---------------------------------------------------
    panel.append(&file_btn);
    panel.append(&preview);
    panel.append(&info_label);
    panel.append(&anim_label);

    let brightness_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let b_label = gtk::Label::new(Some("Яркость экрана:"));
    b_label.add_css_class("dim-label");
    brightness_row.append(&b_label);
    brightness_row.append(&brightness_scale);
    let b_apply = gtk::Button::with_label("Применить");
    brightness_row.append(&b_apply);
    panel.append(&brightness_row);
    panel.append(&actions_row);

    // --- Обработчики ----------------------------------------------
    // Выбор файла
    file_btn.connect_clicked(clone!(
        #[strong]
        preview,
        #[strong]
        info_label,
        #[strong]
        anim_label,
        #[strong]
        send_btn,
        #[strong]
        preview_img,
        #[strong]
        anim_state,
        move |btn| {
            let dialog = gtk::FileDialog::builder()
                .title("Выберите изображение или GIF")
                .build();
            let parent = btn.root().and_downcast::<gtk::Window>();

            let future = match &parent {
                Some(w) => dialog.open_future(Some(w)),
                None => dialog.open_future(None::<&gtk::Window>),
            };

            glib::MainContext::default().spawn_local(clone!(
                #[strong]
                preview,
                #[strong]
                preview_img,
                #[strong]
                info_label,
                #[strong]
                anim_label,
                #[strong]
                anim_state,
                #[strong]
                send_btn,
                async move {
                    match future.await {
                        Ok(file) => {
                            let Some(path) = file.path() else {
                                info_label.set_text("Не удалось получить путь до файла");
                                return;
                            };
                            match std::fs::read(&path) {
                                Ok(bytes) => match converter::convert(&bytes) {
                                    Ok(anim) => {
                                        let frames = anim.frames.len();
                                        let dur = anim.total_duration_ms();
                                        // Превью 1-го кадра
                                        let frame = anim.first_frame();
                                        if let Some(img) = converter::rgb565_to_preview(&frame.data) {
                                            *preview_img.borrow_mut() = Some(image::DynamicImage::ImageRgba8(img));
                                            preview.queue_draw();
                                        }
                                        if frames > 1 {
                                            info_label.set_text(&format!("Файл: {}", path.display()));
                                            anim_label.set_text(&format!("GIF: {frames} кадров, ~{dur} мс"));
                                        } else {
                                            info_label.set_text(&format!("Статичное изображение: {}", path.display()));
                                            anim_label.set_text("1 кадр");
                                        }
                                        *anim_state.borrow_mut() = Some(anim);
                                        send_btn.set_sensitive(true);
                                    }
                                    Err(e) => {
                                        info_label.set_text("Ошибка конвертации");
                                        anim_label.set_text(&format!("{e}"));
                                    }
                                },
                                Err(e) => {
                                    anim_label.set_text(&format!("Ошибка чтения: {e}"));
                                }
                            }
                        }
                        Err(e) => {
                            info_label.set_text("Выбор файла отменён");
                            anim_label.set_text(&format!("{e}"));
                        }
                    }
                }
            ));
        }
    ));

    // Прошивка на экран
    let fetcher2 = fetcher.clone();
    send_btn.connect_clicked(clone!(
        #[strong]
        anim_state,
        move |_| {
        let anim = anim_state.borrow().clone();
        if let Some(anim) = anim {
            let fetcher3 = fetcher2.clone();
            glib::MainContext::default().spawn_local(clone!(
                #[strong]
                anim,
                async move {
                if let Some(dev) = fetcher3() {
                    // Прошиваем первый кадр (или всю анимацию — по формату экрана)
                    let frame = anim.frames.first().expect("есть кадры");
                    let sf = crate::types::ScreenFrame {
                        width: anim.width,
                        height: anim.height,
                        data_rgb565: frame.data.clone(),
                    };
                    match dev.set_screen_image(sf) {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("ошибка прошивки экрана: {e}");
                        }
                    }
                }
            }));
        }
    }));

    // Синхронизация времени
    let fetcher4 = fetcher.clone();
    time_btn.connect_clicked(move |_| {
        if let Some(dev) = fetcher4() {
            let _ = dev.sync_system_time();
        }
    });

    // Яркость экрана
    let fetcher5 = fetcher.clone();
    b_apply.connect_clicked(clone!(
        #[strong]
        brightness_scale,
        move |_| {
        let level = brightness_scale.value() as u8;
        if let Some(dev) = fetcher5() {
            let _ = dev.set_screen_brightness(level);
        }
    }));

    panel
}

/// Нарисовать превью на Canvas с сохранением пропорций.
fn draw_preview(ctx: &cairo::Context, img: &image::DynamicImage, w: f64, h: f64) {
    let rgba = img.as_rgba8().unwrap();
    let (iw, ih) = rgba.dimensions();
    if iw == 0 || ih == 0 {
        return;
    }
    // Масштаб
    let scale = (w / iw as f64).min(h / ih as f64);
    let off_x = (w - iw as f64 * scale) / 2.0;
    let off_y = (h - ih as f64 * scale) / 2.0;

    // Пробуем залить пиксели напрямую в ARGB32 (premultiplied).
    let mut surface = match cairo::ImageSurface::create(cairo::Format::ARgb32, iw as i32, ih as i32) {
        Ok(s) => s,
        Err(_) => return,
    };
    let stride = surface.stride() as usize;
    {
        let mut data = match surface.data() {
            Ok(d) => d,
            Err(_) => return,
        };
        for (i, px) in rgba.pixels().enumerate() {
            let a = px[3] as u32;
            let r = ((px[0] as u32) * a) / 255;
            let g = ((px[1] as u32) * a) / 255;
            let b = ((px[2] as u32) * a) / 255;
            let row = i / (iw as usize);
            let col = i % (iw as usize);
            let off = row * stride + col * 4;
            data[off] = b as u8;
            data[off + 1] = g as u8;
            data[off + 2] = r as u8;
            data[off + 3] = a as u8;
        }
    }

    let _ = ctx.save();
    let _ = ctx.set_source_surface(&surface, off_x, off_y);
    let _ = ctx.paint();
    let _ = ctx.restore();
}