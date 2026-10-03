//! Draft editing; the app handles submission without invoking the backend.

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::Style;
use ratatui_textarea::{TextArea, WrapMode};

use crate::{colors, input_layout};

pub struct InputBox {
    pub editor: TextArea<'static>,
    pub active: bool,
    pub viewport_top: Cell<usize>,
}

impl Default for InputBox {
    fn default() -> Self {
        let mut editor = TextArea::default();
        editor.set_style(Self::style());
        editor.set_cursor_line_style(Style::default());
        editor.set_wrap_mode(WrapMode::Glyph);
        let mut input = Self {
            editor,
            active: true,
            viewport_top: Cell::new(0),
        };
        input.set_active(true);
        input
    }
}

impl InputBox {
    pub fn style() -> Style {
        Style::default()
            .bg(colors::INPUT_BACKGROUND)
            .fg(colors::INPUT_FOREGROUND)
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
        self.editor.set_cursor_style(if active {
            Style::default()
                .bg(colors::INPUT_FOREGROUND)
                .fg(colors::INPUT_BACKGROUND)
        } else {
            Self::style()
        });
    }

    pub fn key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        if key.code == KeyCode::Char('j') && key.modifiers == KeyModifiers::CONTROL {
            self.editor.insert_newline();
            return;
        }
        if key.code == KeyCode::Enter {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                self.editor.insert_newline();
            }
            return;
        }
        if matches!(key.code, KeyCode::PageUp | KeyCode::PageDown) {
            return;
        }
        self.editor.input(key);
    }

    pub fn paste(&mut self, text: String) {
        self.editor
            .insert_str(text.replace("\r\n", "\n").replace('\r', "\n"));
    }

    pub fn take_draft(&mut self) -> Option<String> {
        let text = self.editor.lines().join("\n");
        if text.trim().is_empty() {
            return None;
        }
        let active = self.active;
        *self = Self::default();
        self.set_active(active);
        Some(text)
    }

    pub fn height(&self, width: u16, available: u16) -> u16 {
        input_layout::height(
            self.editor.lines(),
            width.saturating_sub(3), // Two borders and the end-of-line caret cell.
            self.editor.tab_length(),
        )
        .saturating_add(2) // One editable line plus top/bottom borders initially.
        .min(available)
    }
}
