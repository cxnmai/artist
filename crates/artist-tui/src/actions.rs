use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::mode::Mode;

pub enum Action {
    SetMode(Mode),
    Quit,
}

pub fn from_key(key: KeyEvent, mode: Mode) -> Option<Action> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(Action::Quit);
    }
    match (mode, key.code) {
        (_, KeyCode::Esc) => Some(Action::SetMode(Mode::Navigation)),
        (Mode::Navigation, KeyCode::Char('i')) if key.modifiers.is_empty() => {
            Some(Action::SetMode(Mode::Insert))
        }
        (Mode::Navigation, KeyCode::Char('q')) if key.modifiers.is_empty() => Some(Action::Quit),
        _ => None,
    }
}
