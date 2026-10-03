//! Typed backend events update status and response blocks; only tools/debug events use JSON.

use artist_core::cancellation::CancellationToken;
use artist_core::display::{DisplayEntry, DisplayModelEvent};
use artist_core::engine::AgentEvent;
use tokio::sync::mpsc::UnboundedSender;

use crate::app::App;
use crate::backend::Backend;
use crate::backend_worker::Connection;
use crate::chat_block::ChatBlock;
use crate::mode::Mode;
use crate::response_view::OutputKind;

impl App {
    pub fn attach_backend(
        &mut self,
        backend: Option<Backend>,
        events: UnboundedSender<Vec<AgentEvent>>,
    ) {
        self.status.loading = false;
        if let Some(backend) = backend {
            self.apply_events(backend.initial_events());
            self.backend = Some(Connection::start(backend, events));
        }
    }

    pub fn apply_events(&mut self, events: Vec<AgentEvent>) {
        for event in events {
            let json = if matches!(
                &event,
                AgentEvent::User { .. }
                    | AgentEvent::Model {
                        event: DisplayModelEvent::TextDelta { .. }
                            | DisplayModelEvent::ThinkingDelta { .. }
                    }
                    | AgentEvent::Assistant {
                        entry: DisplayEntry::Assistant { .. }
                    }
            ) {
                None
            } else {
                Some(serde_json::to_string(&event).expect("agent events serialize"))
            };
            match event {
                AgentEvent::User { text } => {
                    // The authoritative backend user event creates the band once, not twice.
                    self.response.reset();
                    self.chat.push(ChatBlock::UserMessage { text });
                    continue;
                }
                AgentEvent::Model {
                    event: DisplayModelEvent::TextDelta { text },
                } => {
                    self.response.delta(&mut self.chat, OutputKind::Text, text);
                    continue;
                }
                AgentEvent::Model {
                    event: DisplayModelEvent::ThinkingDelta { text },
                } => {
                    self.response
                        .delta(&mut self.chat, OutputKind::Thinking, text);
                    continue;
                }
                AgentEvent::Assistant {
                    entry: DisplayEntry::Assistant { blocks },
                } => {
                    let tools = self.response.commit(&mut self.chat, blocks);
                    if !tools.is_empty() {
                        let event = AgentEvent::Assistant {
                            entry: DisplayEntry::Assistant { blocks: tools },
                        };
                        self.chat.push(ChatBlock::RawJson {
                            text: serde_json::to_string(&event).expect("tool events serialize"),
                        });
                    }
                    continue;
                }
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
                AgentEvent::Done | AgentEvent::Cancelled | AgentEvent::Error { .. } => {
                    self.active_turn = None;
                    self.status.busy = false;
                    self.response.reset();
                }
                _ => {}
            }
            self.chat.push(ChatBlock::RawJson {
                text: json.expect("unformatted event has a JSON presentation"),
            });
        }
    }

    pub(super) fn submit(&mut self) {
        // Sequential turns: keep the next draft intact until the active turn is terminal.
        if !matches!(self.mode, Mode::Insert) || self.active_turn.is_some() {
            return;
        }
        let Some(text) = self.input.draft() else {
            return;
        };
        let Some(backend) = &self.backend else {
            let message = if self.status.loading {
                "provider is still loading"
            } else {
                "load a provider with --config before sending prompts"
            };
            self.apply_events(vec![AgentEvent::Error {
                message: message.into(),
            }]);
            return;
        };
        let token = CancellationToken::new();
        match backend.submit(text, token.clone()) {
            Ok(()) => {
                self.input.take_draft();
                self.active_turn = Some(token);
                self.status.busy = true;
                self.chat.bottom();
            }
            Err(message) => self.apply_events(vec![AgentEvent::Error { message }]),
        }
    }
}
