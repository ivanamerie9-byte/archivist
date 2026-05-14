#![allow(dead_code)]

use ratatui::style::{Color, Modifier, Style};

pub const BG: Color = Color::Rgb(0x0e, 0x0e, 0x0f);
pub const SURFACE: Color = Color::Rgb(0x16, 0x16, 0x18);
pub const BORDER: Color = Color::Rgb(0x25, 0x25, 0x28);
pub const ACCENT: Color = Color::Rgb(0xe8, 0xc9, 0x7e);
pub const TEXT: Color = Color::Rgb(0xd4, 0xd4, 0xd8);
pub const TEXT_DIM: Color = Color::Rgb(0x71, 0x71, 0x7a);
pub const SUCCESS: Color = Color::Rgb(0x6e, 0xe7, 0xb7);
pub const ERROR: Color = Color::Rgb(0xf8, 0x71, 0x71);

pub fn title_style() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn dim_style() -> Style {
    Style::default().fg(TEXT_DIM)
}

pub fn ok_style() -> Style {
    Style::default().fg(SUCCESS)
}

pub fn err_style() -> Style {
    Style::default().fg(ERROR)
}
