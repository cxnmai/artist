//! Display state is independent of Lua configuration and terminal rendering.

use std::path::PathBuf;

use artist_core::cancellation::CancellationToken;
use artist_core::context_usage::ContextUsage;

use crate::actions::Action;
use crate::backend_worker::Connection;
use crate::chat::Chat;
use crate::input::InputBox;
use crate::mode::Mode;
use crate::response_view::ResponseView;

pub struct Status {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub reasoning: Option<String>,
    pub context_window: Option<u64>,
    pub context_usage: Option<ContextUsage>,
    pub cwd: PathBuf,
    pub loading: bool,
    pub busy: bool,
}

pub struct App {
    pub mode: Mode,
    pub status: Status,
    pub input: InputBox,
    pub chat: Chat,
    pub(super) backend: Option<Connection>,
    pub(super) active_turn: Option<CancellationToken>,
    pub(super) response: ResponseView,
}

impl App {
    pub fn new(cwd: PathBuf, loading: bool) -> Self {
        Self {
            mode: Mode::default(),
            input: InputBox::default(),
            chat: Chat::default(),
            backend: None,
            active_turn: None,
            response: ResponseView::default(),
            status: Status {
                cwd,
                loading,
                busy: false,
                provider: None,
                model: None,
                reasoning: None,
                context_window: None,
                context_usage: None,
            },
        }
    }

    /// Return true when the UI should exit.
    pub fn act(&mut self, action: Action) -> bool {
        match action {
            Action::SetMode(mode) => {
                self.mode = mode;
                self.input.set_active(matches!(mode, Mode::Insert));
                if matches!(mode, Mode::Navigation) {
                    self.chat.enter_navigation();
                }
                false
            }
            Action::EditInput(key) => {
                if matches!(self.mode, Mode::Insert) {
                    self.input.key(key);
                }
                false
            }
            Action::PasteInput(text) => {
                if matches!(self.mode, Mode::Insert) {
                    self.input.paste(text);
                }
                false
            }
            Action::SubmitDraft => {
                self.submit();
                false
            }
            Action::Interrupt => match &self.active_turn {
                Some(token) if !token.is_cancelled() => {
                    token.cancel();
                    false
                }
                _ => true,
            },
            Action::SelectMessage(direction) => {
                self.chat.select(direction);
                false
            }
            Action::ScrollChat(lines) => {
                self.chat.scroll_by(lines);
                false
            }
            Action::PageChat(direction) => {
                self.chat.page(direction);
                false
            }
            Action::ChatTop => {
                self.chat.top();
                false
            }
            Action::ChatBottom => {
                self.chat.bottom();
                false
            }
            Action::Quit => true,
        }
    }
}
