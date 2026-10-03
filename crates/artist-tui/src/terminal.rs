//! Restore paste and keyboard protocols as well as the alternate screen on exit.

use std::io;

use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};

pub struct Session {
    pub terminal: ratatui::DefaultTerminal,
}

impl Session {
    pub fn new() -> io::Result<Self> {
        let session = Self {
            terminal: ratatui::init(),
        };
        // Enhanced terminals can distinguish Shift-Enter from ordinary Enter.
        // Legacy terminals ignore the keyboard protocol request.
        crossterm::execute!(
            io::stdout(),
            EnableBracketedPaste,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES),
        )?;
        Ok(session)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = crossterm::execute!(
            io::stdout(),
            PopKeyboardEnhancementFlags,
            DisableBracketedPaste
        );
        ratatui::restore();
    }
}
