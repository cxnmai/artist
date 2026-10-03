//! Display state is independent of Lua configuration and terminal rendering.

use std::path::PathBuf;

use artist_core::context_usage::ContextUsage;
use artist_core::engine::AgentEvent;

use crate::actions::Action;
use crate::backend::Backend;
use crate::chat::Chat;
use crate::chat_block::ChatBlock;
use crate::input::InputBox;
use crate::mode::Mode;

pub struct Status {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub reasoning: Option<String>,
    pub context_window: Option<u64>,
    pub context_usage: Option<ContextUsage>,
    pub cwd: PathBuf,
    pub loading: bool,
}

pub struct App {
    pub mode: Mode,
    pub status: Status,
    pub input: InputBox,
    pub chat: Chat,
    backend: Option<Backend>,
}

impl App {
    pub fn new(cwd: PathBuf, loading: bool) -> Self {
        Self {
            mode: Mode::default(),
            input: InputBox::default(),
            chat: Chat::default(),
            backend: None,
            status: Status {
                cwd,
                loading,
                provider: None,
                model: None,
                reasoning: None,
                context_window: None,
                context_usage: None,
            },
        }
    }

    pub fn attach_backend(&mut self, backend: Option<Backend>) {
        self.backend = backend;
        self.status.loading = false;
        if let Some(backend) = &self.backend {
            self.apply_events(backend.initial_events());
        }
    }

    pub fn apply_events(&mut self, events: Vec<AgentEvent>) {
        for event in events {
            match event {
                AgentEvent::ModelInfo {
                    provider,
                    model,
                    reasoning_level,
                    context_window,
                    ..
                } => {
                    self.status.provider = Some(provider);
                    self.status.model = Some(model);
                    self.status.reasoning = reasoning_level;
                    self.status.context_window = context_window;
                    self.status.context_usage = None;
                }
                AgentEvent::ContextUsage { usage } => self.status.context_usage = Some(usage),
                _ => {} // Chat events are outside the ribbon's scope.
            }
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
                if matches!(self.mode, Mode::Insert) {
                    if let Some(text) = self.input.take_draft() {
                        self.chat.push(ChatBlock::UserMessage { text });
                    }
                }
                false
            }
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
