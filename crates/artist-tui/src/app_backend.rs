//! Typed backend events update status and response blocks; only tool events use JSON.

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
            match event {
                AgentEvent::User { text } => {
                    self.response.reset();
                    self.chat.push(ChatBlock::UserMessage { text });
                }
                AgentEvent::Model {
                    event: DisplayModelEvent::TextDelta { text },
                } => {
                    self.response.delta(&mut self.chat, OutputKind::Text, text);
                }
                AgentEvent::Model {
                    event: DisplayModelEvent::ThinkingDelta { text },
                } => {
                    self.response
                        .delta(&mut self.chat, OutputKind::Thinking, text);
                }
                AgentEvent::Assistant {
                    entry: DisplayEntry::Assistant { blocks },
                } => {
                    let tools = self.response.commit(&mut self.chat, blocks);
                    if !tools.is_empty() {
                        self.push_tool_json(AgentEvent::Assistant {
                            entry: DisplayEntry::Assistant { blocks: tools },
                        });
                    }
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
                AgentEvent::Model {
                    event: DisplayModelEvent::Usage { .. } | DisplayModelEvent::Finished { .. },
                } => {}
                event @ AgentEvent::Model { .. } | event @ AgentEvent::ToolResult { .. } => {
                    self.push_tool_json(event)
                }
                AgentEvent::Done => self.end_turn(),
                AgentEvent::Cancelled => {
                    self.end_turn();
                    self.chat.push(ChatBlock::Notice {
                        text: "Cancelled.".into(),
                    });
                }
                AgentEvent::Error { message } => {
                    self.end_turn();
                    self.chat.push(ChatBlock::Notice {
                        text: format!("Error: {message}"),
                    });
                }
                AgentEvent::Compacted { .. } => {
                    self.chat.push(ChatBlock::Notice {
                        text: "Context compacted.".into(),
                    });
                }
                _ => {}
            }
        }
    }

    fn push_tool_json(&mut self, event: AgentEvent) {
        self.chat.push(ChatBlock::RawJson {
            text: serde_json::to_string(&event).expect("tool events serialize"),
        });
    }

    fn end_turn(&mut self) {
        self.active_turn = None;
        self.status.busy = false;
        self.response.reset();
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
