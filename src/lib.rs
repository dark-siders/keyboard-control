// --- Библиотечный корень Keyboard-Center ---
// Все подсистемы: устройства, GUI, конвертер экрана, профили.

pub mod devices;
pub mod gui;
pub mod profiles;
pub mod screen;

// Переиспользуемые типы-композиты (цвет, клавиша и т.д.)
pub mod types;

// Версия приложения (доступна из GUI и CLI)
pub const VERSION: &str = env!("CARGO_PKG_VERSION");