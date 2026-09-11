// --- Конвертер картинок для TFT-экрана Ajazz AK820 Pro ---
// Принимает GIF / PNG / JPEG / BMP, ресайзит под экран (128×160),
// конвертирует в RGB565 (big-endian), поддерживает анимации GIF.

use image::{AnimationDecoder, DynamicImage, ImageFormat};
use std::io::Cursor;

pub use image::{Frames, Rgba};

/// Разрешение TFT-экрана Ajazz AK820 Pro.
pub const SCREEN_WIDTH: u16 = 128;
pub const SCREEN_HEIGHT: u16 = 160;

/// Один кадр анимации в RGB565.
#[derive(Debug, Clone)]
pub struct Rgb565Frame {
    pub data: Vec<u8>, // SCREEN_WIDTH * SCREEN_HEIGHT * 2 байт
    pub delay_ms: u16,
}

/// Результат конвертации: кадры в RGB565 + метаданные.
#[derive(Debug, Clone)]
pub struct ConvertedAnimation {
    pub frames: Vec<Rgb565Frame>,
    pub width: u16,
    pub height: u16,
}

impl ConvertedAnimation {
    /// Является ли анимацией (более 1 кадра).
    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }

    /// Суммарная длительность (если анимация).
    pub fn total_duration_ms(&self) -> u64 {
        self.frames.iter().map(|f| f.delay_ms as u64).sum()
    }

    /// Первый кадр (для стоп-кадра GIF/картинки).
    pub fn first_frame(&self) -> &Rgb565Frame {
        &self.frames[0]
    }
}

/// Конвертация RGB888 → RGB565.
pub fn rgb888_to_rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | (b as u16 >> 3)
}

/// Ресайзинг под экран с сохранением пропорций + центрирование.
fn fit_to_screen(
    img: &DynamicImage,
    /* target_w, target_h */
) -> DynamicImage {
    img.resize_to_fill(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32, image::imageops::FilterType::Lanczos3)
}

/// Извлечь задержку кадра GIF (в мс), если доступна.
fn frame_delay_ms(frame: &image::Frame) -> u16 {
    // Delay хранит соотношение в миллисекундах: numer/denom.
    let (numer, denom) = frame.delay().numer_denom_ms();
    let denom = denom.max(1);
    // Округляем вверх: ceil(numer/denom), минимум 1 мс.
    let ms = (numer as u64).saturating_add(denom as u64 - 1) / denom as u64;
    ms.clamp(1, u16::MAX as u64) as u16
}

/// Конвертация RgbaImage → RGB565.
fn to_rgb565(img: &image::RgbaImage) -> Vec<u8> {
    let mut out = Vec::with_capacity((SCREEN_WIDTH * SCREEN_HEIGHT) as usize * 2);
    for (_, _, px) in img.enumerate_pixels() {
        let rgb565 = rgb888_to_rgb565(px[0], px[1], px[2]);
        // big-endian формат: старший байт первым
        out.push((rgb565 >> 8) as u8);
        out.push((rgb565 & 0xFF) as u8);
    }
    out
}

/// Конвертация файла (по данным байт) в анимацию для экрана.
/// GIF с несколькими кадрами → анимация; статика & PNG/JPEG → 1 кадр.
pub fn convert(bytes: &[u8]) -> anyhow::Result<ConvertedAnimation> {
    let format = image::guess_format(bytes)?;
    let mut frames: Vec<Rgb565Frame> = Vec::new();

    match format {
        ImageFormat::Gif => {
            let gif = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))?;
            let mut frames_itr = gif.into_frames();
            while let Some(frame) = frames_itr.next() {
                let frame = frame.map_err(|e| anyhow::anyhow!("GIF-кадр: {e}"))?;
                let delay_ms = frame_delay_ms(&frame);
                let buffer = frame.into_buffer();
                let img = image::DynamicImage::ImageRgba8(buffer);
                // Ресайз под экран
                let fitted = fit_to_screen(&img);
                let rgb565 = to_rgb565(fitted.as_rgba8().expect("RGBA8"));
                frames.push(Rgb565Frame {
                    data: rgb565,
                    delay_ms,
                });
            }
            if frames.is_empty() {
                return Err(anyhow::anyhow!("GIF не содержит кадров"));
            }
        }
        _ => {
            // PNG / JPEG / BMP / WEBP и т.д. — статичное изображение
            let img = image::load_from_memory(bytes)?;
            let fitted = fit_to_screen(&img);
            let rgb565 = to_rgb565(fitted.as_rgba8().expect("RGBA8"));
            frames.push(Rgb565Frame {
                data: rgb565,
                delay_ms: 1000,
            });
        }
    }

    Ok(ConvertedAnimation {
        frames,
        width: SCREEN_WIDTH,
        height: SCREEN_HEIGHT,
    })
}

/// Конвертация из RgbaImage (для GUI-preview).
pub fn convert_rgba(img: &image::RgbaImage) -> Vec<u8> {
    to_rgb565(img)
}

/// Создание превью-изображения (RgbaImage) для предпросмотра в GUI.
/// Визуализирует RGB565 кадр обратно в RGB888.
pub fn rgb565_to_preview(data: &[u8]) -> Option<image::RgbaImage> {
    let expected = (SCREEN_WIDTH * SCREEN_HEIGHT) as usize * 2;
    if data.len() < expected {
        return None;
    }
    let mut img = image::RgbaImage::new(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32);
    for y in 0..SCREEN_HEIGHT {
        for x in 0..SCREEN_WIDTH {
            let idx = (y * SCREEN_WIDTH + x) as usize * 2;
            let hi = data[idx] as u16;
            let lo = data[idx + 1] as u16;
            let rgb565 = (hi << 8) | lo;
            let r = (((rgb565 >> 11) & 0x1F) << 3) as u8;
            let g = (((rgb565 >> 5) & 0x3F) << 2) as u8;
            let b = ((rgb565 & 0x1F) << 3) as u8;
            img.put_pixel(x as u32, y as u32, image::Rgba([r, g, b, 255]));
        }
    }
    Some(img)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb888_to_rgb565() {
        // Красный (255,0,0) → сохранить верхние биты 0xF800
        assert_eq!(rgb888_to_rgb565(255, 0, 0), 0xF800);
        // Зелёный — 0x07E0
        assert_eq!(rgb888_to_rgb565(0, 255, 0), 0x07E0);
        // Синий — 0x001F
        assert_eq!(rgb888_to_rgb565(0, 0, 255), 0x001F);
        // Белый — 0xFFFF
        assert_eq!(rgb888_to_rgb565(255, 255, 255), 0xFFFF);
        // Чёрный — 0x0000
        assert_eq!(rgb888_to_rgb565(0, 0, 0), 0x0000);
    }

    #[test]
    fn test_hex_color() {
        let c = crate::types::Color::from_hex_checked("#FF8000").unwrap();
        assert_eq!((c.r, c.g, c.b), (0xFF, 0x80, 0x00));
        assert!(crate::types::Color::from_hex_checked("#GG0000").is_none());
        assert!(crate::types::Color::from_hex_checked("FF").is_none());
    }
}