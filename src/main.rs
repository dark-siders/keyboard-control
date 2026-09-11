// --- GUI-бинарник: keyboard-center ---
// Основная точка входа графического приложения.

fn main() -> glib::ExitCode {
    keyboard_center::gui::app::run_gtk()
}