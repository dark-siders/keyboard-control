// --- Визуальный макет клавиш (Layout Rendering) ---
// Рисование раскладки 75%/96% через Cairo на DrawingArea.
// Переиспользуется RGB-студией, редактором клавиш и keymap-редактором.

use gtk::cairo;
use gtk::prelude::*;
use gtk::DrawingArea;
use std::cell::RefCell;
use std::rc::Rc;

use crate::types::{Color, KeyId};

/// Позиция клавиши в условной сетке.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyPos {
    /// Скэнкод клавиши (USB HID).
    pub keycode: u16,
    /// Сетка: позиция в десятых долях клавиши.
    pub col: u16,
    pub row: u16,
    /// Ширина клавиши, в единицах (1 = обычная клавиша, 2 = Enter, и т.д.)
    pub width: f64,
}

/// Раскладка: список позиций + размер сетки.
#[derive(Debug, Clone)]
pub struct Layout {
    pub rows: u16,
    pub cols: u16,
    pub keys: Vec<KeyPos>,
}

impl Layout {
    /// Переключение между 75% и 96%.
    pub fn for_layout(kind: crate::devices::Layout) -> Self {
        match kind {
            crate::devices::Layout::L75 => layout_75(),
            crate::devices::Layout::L96 => layout_96(),
        }
    }

    /// Найти ключ по keycode.
    pub fn find(&self, keycode: u16) -> Option<&KeyPos> {
        self.keys.iter().find(|k| k.keycode == keycode)
    }

    /// Hit-test: клик по раскладке в координатах виджета → keycode клавиши.
    /// Геометрия строго повторяет `draw_layout` (margin=14, gap=3, десятые доли
    /// сетки по col/row). Возвращает None, если клик пришёлся в промежуток.
    pub fn key_at(&self, x: f64, y: f64, w: f64, h: f64) -> Option<u16> {
        let margin = 14.0;
        let gap = 3.0;
        let key_w = (w - margin * 2.0) / self.cols as f64;
        let key_h =
            (h - margin * 2.0 - (self.rows as f64 - 1.0) * gap) / self.rows as f64;
        for key in &self.keys {
            let rx = margin + key.col as f64 * key_w;
            let ry = margin + key.row as f64 * (key_h + gap);
            let rw = key.width * key_w;
            if x >= rx && x <= rx + rw && y >= ry && y <= ry + key_h {
                return Some(key.keycode);
            }
        }
        None
    }
}

/// Вспомогательная функция: добавить строку клавиш в список.
fn push_row(keys: &mut Vec<KeyPos>, row: u16, cols: &[(u16, f64)]) {
    let mut pos: u16 = 0;
    for (keycode, width) in cols {
        keys.push(KeyPos {
            keycode: *keycode,
            col: pos,
            row,
            width: *width,
        });
        pos += (*width * 10.0).round() as u16;
    }
}

/// Строка со сдвигом первой клавиши.
fn push_row_shifted(keys: &mut Vec<KeyPos>, row: u16, shift: f64, cols: &[(u16, f64)]) {
    let mut pos: u16 = (shift * 10.0).round() as u16;
    for (keycode, width) in cols {
        keys.push(KeyPos {
            keycode: *keycode,
            col: pos,
            row,
            width: *width,
        });
        pos += (*width * 10.0).round() as u16;
    }
}

/// Параметры стандартной строки из смежных скэнкодов.
fn range_keys(start: u16, end: u16) -> Vec<(u16, f64)> {
    (start..=end).map(|k| (k, 1.0)).collect()
}

// --- 75% раскладка (Ajazz AK820 Pro / AULA F75) ---
fn layout_75() -> Layout {
    let mut keys = Vec::new();

    // F-ряд: Esc + F1..F12
    let f: Vec<(u16, f64)> = (0x29..=0x3D).map(|k| (k, 1.0)).collect();
    push_row(&mut keys, 0, &f);

    // Row 1 (цифры + Backspace)
    let numbers = range_keys(0x1E, 0x27);
    let mut row1 = vec![(0x29, 1.5)]; // Esc/Backquote (заглушка)
    row1.extend(numbers);
    row1.extend([(0x2A, 1.0), (0x2B, 1.5)]);
    push_row(&mut keys, 1, &row1);

    // Row 2 (QWERTY)
    let qwerty = range_keys(0x14, 0x1D);
    let mut row2 = vec![(0xE1, 1.25)]; // CapsLock
    row2.extend(qwerty);
    row2.extend([(0x1A, 1.0), (0x1B, 1.0), (0x2D, 2.0)]); // Backspace
    push_row(&mut keys, 2, &row2);

    // Row 3 (ASDF)
    let asdf = range_keys(0x04, 0x0D);
    let mut row3 = vec![(0xE0, 1.75)]; // Left Shift
    row3.extend(asdf);
    row3.extend([(0x28, 1.0), (0x1C, 1.0), (0x2C, 1.0)]); // Enter
    push_row(&mut keys, 3, &row3);

    // Row 4 (ZXCV)
    let zxcv = range_keys(0x0E, 0x13);
    let mut row4 = vec![(0xE1, 1.25), (0xE0, 1.25)]; // Left Ctrl + Shift
    row4.extend(zxcv);
    row4.extend([(0xE5, 2.0)]); // Right Shift
    push_row(&mut keys, 4, &row4);

    // Пробел
    let mut row5 = vec![(0x2C, 1.0)]; // Alt
    row5.extend([(0x1C, 5.0)]); // Space
    row5.extend([(0x2D, 1.0), (0x2E, 1.0), (0x2F, 2.0)]); // Fn, Menu
    push_row_shifted(&mut keys, 5, 1.5, &row5);

    Layout { rows: 6, cols: 20, keys }
}

// --- 96% раскладка (AULA F99) ---
fn layout_96() -> Layout {
    let mut keys = Vec::new();

    // Полный F-ряд: Esc + 13 клавиш
    let f: Vec<(u16, f64)> = (0x29..=0x46).map(|k| (k, 1.0)).collect();
    push_row(&mut keys, 0, &f);

    // Цифры
    let numbers = range_keys(0x1E, 0x27);
    let mut row1 = vec![(0x29, 1.5)];
    row1.extend(numbers);
    row1.extend([(0x2A, 1.0), (0x2B, 1.5)]);
    push_row(&mut keys, 1, &row1);

    // QWERTY
    let qwerty = range_keys(0x14, 0x1D);
    let mut row2 = vec![(0xE1, 1.25)];
    row2.extend(qwerty);
    row2.extend([(0x1A, 1.0), (0x1B, 1.0), (0x2D, 2.0)]);
    push_row(&mut keys, 2, &row2);

    // ASDF
    let asdf = range_keys(0x04, 0x0D);
    let mut row3 = vec![(0xE0, 1.75)];
    row3.extend(asdf);
    row3.extend([(0x28, 1.0), (0x1C, 1.0), (0x2C, 1.0)]);
    push_row(&mut keys, 3, &row3);

    // ZXCV
    let zxcv = range_keys(0x0E, 0x13);
    let mut row4 = vec![(0xE1, 1.25), (0xE0, 1.25)];
    row4.extend(zxcv);
    row4.extend([(0xE5, 1.5), (0xE4, 1.5)]);
    push_row(&mut keys, 4, &row4);

    // Нижний ряд
    let mut row5 = vec![(0x2C, 1.0)];
    row5.extend([(0x1C, 5.0)]); // Space
    row5.extend([(0x2D, 1.0)]); // AltGr
    push_row_shifted(&mut keys, 5, 1.5, &row5);

    // NumPad (6-я строка)
    let numpad: Vec<(u16, f64)> = vec![
        (0x53, 1.0), (0x54, 1.0), (0x55, 1.0), (0x56, 1.0),
        (0x57, 1.0), (0x58, 1.0), (0x59, 1.0), (0x5A, 1.0),
        (0x5B, 1.0), (0x5C, 1.0),
    ];
    push_row(&mut keys, 6, &numpad);

    Layout { rows: 7, cols: 20, keys }
}

// --- Рендер-представление ---
/// Состояние раскраски клавиш (карта keycode → цвет).
pub type KeyColorMap = Rc<RefCell<std::collections::HashMap<KeyId, Color>>>;

pub struct LayoutWidget;

impl LayoutWidget {
    /// Создать DrawingArea с раскладкой.
    /// Возвращает (area, colors): colors можно передавать в set_key_color.
    pub fn new_drawing_area(layout: Layout) -> (DrawingArea, KeyColorMap) {
        let colors: KeyColorMap = Rc::new(RefCell::new(std::collections::HashMap::new()));
        let area = DrawingArea::builder().vexpand(true).hexpand(true).build();

        let draw_colors = colors.clone();
        area.set_draw_func(move |_, ctx, w, h| {
            let colors = &*draw_colors.borrow();
            draw_layout(ctx, &layout, colors, w as f64, h as f64);
        });

        (area, colors)
    }

    /// Установить цвет клавиши и перерисовать.
    pub fn set_key_color(colors: &KeyColorMap, area: &DrawingArea, key: KeyId, color: Color) {
        colors.borrow_mut().insert(key, color);
        area.queue_draw();
    }

    /// Сбросить подсветку всех клавиш.
    pub fn clear_colors(colors: &KeyColorMap, area: &DrawingArea) {
        colors.borrow_mut().clear();
        area.queue_draw();
    }
}

// --- Рисование ---
fn draw_layout(
    ctx: &cairo::Context,
    layout: &Layout,
    colors: &std::collections::HashMap<KeyId, Color>,
    w: f64,
    h: f64,
) {
    let margin = 14.0;
    let gap = 3.0;
    let cols_f = layout.cols as f64;
    // Ширина клавиши по ширине виджета (20 колонок).
    let key_w = (w - margin * 2.0) / cols_f;
    let rows_f = layout.rows as f64;
    let key_h = (h - margin * 2.0 - (rows_f - 1.0) * gap) / rows_f;

    // Тёмный фон корпуса
    ctx.set_source_rgba(0.05, 0.05, 0.07, 1.0);
    let board_w = cols_f * key_w + 8.0;
    let board_h = rows_f * key_h + (rows_f - 1.0) * gap + 8.0;
    round_rect_path(ctx, margin - 4.0, margin - 4.0, board_w, board_h, 14.0);
    let _ = ctx.fill();

    for key in &layout.keys {
        let x = margin + key.col as f64 * key_w;
        let y = margin + key.row as f64 * (key_h + gap);
        let ww = key.width * key_w;

        // Цвет клавиши
        match colors.get(&KeyId(key.keycode)) {
            Some(c) => {
                let (r, g, b) = (c.r as f64 / 255.0, c.g as f64 / 255.0, c.b as f64 / 255.0);
                ctx.set_source_rgb(r, g, b);
            }
            None => ctx.set_source_rgb(0.13, 0.14, 0.17),
        }
        round_rect_path(ctx, x, y, ww, key_h, 5.0);
        let _ = ctx.fill();

        // Светлая верхняя грань (объём)
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.08);
        round_rect_path(ctx, x + 1.0, y + 1.0, ww - 2.0, key_h * 0.4, 4.0);
        let _ = ctx.fill();

        // Контур
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.06);
        round_rect_path(ctx, x, y, ww, key_h, 5.0);
        let _ = ctx.stroke();
    }
}

fn round_rect_path(ctx: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    let half = std::f64::consts::FRAC_PI_2;
    ctx.move_to(x + r, y);
    ctx.arc(x + w - r, y + r, r, half * 3.0, 0.0);
    ctx.arc(x + w - r, y + h - r, r, 0.0, half);
    ctx.arc(x + r, y + h - r, r, half, std::f64::consts::PI);
    ctx.arc(x + r, y + r, r, std::f64::consts::PI, half * 3.0);
    ctx.close_path();
}