// --- Подсистема экрана Ajazz ---
// Конвертация и предпросмотр изображений для TFT-экрана.

pub mod converter;

pub use converter::{convert, convert_rgba, rgb565_to_preview, ConvertedAnimation, Rgb565Frame};