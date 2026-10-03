use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::mode::Mode;

pub enum Action {
    SetMode(Mode),
    EditInput(KeyEvent),
    PasteInput(String),
    SubmitDraft,
    SelectMessage(i8),
    ScrollChat(i32),
    PageChat(i32),
    ChatTop,
    ChatBottom,
    Quit,
}

pub fn from_key(key: KeyEvent, mode: Mode) -> Option<Action> {
    if key.kind == KeyEventKind::Release {
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
        (Mode::Navigation, KeyCode::Char('k') | KeyCode::Up) if key.modifiers.is_empty() => {
            Some(Action::SelectMessage(-1))
        }
        (Mode::Navigation, KeyCode::Char('j') | KeyCode::Down) if key.modifiers.is_empty() => {
            Some(Action::SelectMessage(1))
        }
        (Mode::Navigation, KeyCode::PageUp) => Some(Action::PageChat(-1)),
        (Mode::Navigation, KeyCode::PageDown) => Some(Action::PageChat(1)),
        (Mode::Navigation, KeyCode::Char('g')) if key.modifiers.is_empty() => Some(Action::ChatTop),
        (Mode::Navigation, KeyCode::Char('G'))
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            Some(Action::ChatBottom)
        }
        (Mode::Insert, KeyCode::Enter) if key.modifiers.is_empty() => Some(Action::SubmitDraft),
        (Mode::Insert, _) => Some(Action::EditInput(key)),
        _ => None,
    }
}
