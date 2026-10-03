//! Centralized UI color choices.

use ratatui::style::Color;

pub const BACKGROUND: Color = Color::Rgb(0x4A, 0x63, 0x78);
pub const RIBBON_BACKGROUND: Color = Color::Rgb(0x30, 0x40, 0x4E);
pub const RIBBON_FOREGROUND: Color = Color::Rgb(0xEE, 0xEE, 0xEE);
pub const INPUT_BACKGROUND: Color = BACKGROUND;
pub const INPUT_FOREGROUND: Color = Color::Rgb(0xEE, 0xEE, 0xEE);
pub const INPUT_BORDER: Color = Color::Rgb(0xFF, 0xFF, 0xFF);
pub const USER_MESSAGE_BACKGROUND: Color = RIBBON_BACKGROUND;
pub const USER_MESSAGE_FOREGROUND: Color = RIBBON_FOREGROUND;
pub const USER_MESSAGE_SELECTED_BACKGROUND: Color = USER_MESSAGE_FOREGROUND;
pub const USER_MESSAGE_SELECTED_FOREGROUND: Color = USER_MESSAGE_BACKGROUND;
