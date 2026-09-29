//! One user turn, independent of model providers and tool implementations.

use crate::context::{
    AssistantBlock, ContextEntry, ContextSelection, Conversation, SelectedContext,
};
use crate::context_usage::{ContextUsage, UsageAnchor};
use crate::event::ModelEvent;
use crate::response::ResponseAccumulator;
use crate::tools::{ToolDefinition, ToolExecutor, record_assistant};
use serde::Serialize;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

pub struct TurnRequest {
    pub selection: ContextSelection,
    pub tools: Vec<ToolDefinition>,
}

impl Default for TurnRequest {
    fn default() -> Self {
        Self {
            selection: ContextSelection::default(),
            tools: Vec::new(),
        }
    }
}

/// The provider-specific implementation emits normalized events, not wire-format JSON.
pub trait Model {
    fn generate<'a>(
        &'a mut self,
        context: &'a SelectedContext,
        tools: &'a [ToolDefinition],
        emit: &'a mut dyn FnMut(Vec<ModelEvent>),
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>>;
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    ContextUsage {
        usage: ContextUsage,
    },
    ModelInfo {
        provider: String,
        model: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        context_window: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_output_tokens: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reasoning_levels: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reasoning_level: Option<String>,
    },
    User {
        text: String,
    },
    Model {
        event: ModelEvent,
    },
    Assistant {
        entry: ContextEntry,
    },
    ToolResult {
        entry: ContextEntry,
    },
    Error {
        message: String,
    },
    Done,
}

/// Calls `prepare` before *each* model request; it can select history and tools anew.
pub async fn run_turn(
    conversation: &mut Conversation,
    prompt: String,
    model: &mut dyn Model,
    executor: &dyn ToolExecutor,
    cwd: &Path,
    max_model_calls: usize,
    context_window: Option<u64>,
    mut prepare: impl FnMut(&Conversation) -> Result<TurnRequest, String>,
    mut emit: impl FnMut(Vec<AgentEvent>),
) -> Result<(), String> {
    conversation.entries.push(ContextEntry::User {
        text: prompt.clone(),
    });
    emit(vec![AgentEvent::User { text: prompt }]);
    if let Some(usage) = context_window.and_then(|window| conversation.context_usage(window)) {
        emit(vec![AgentEvent::ContextUsage { usage }]);
    }

    for _ in 0..max_model_calls {
        let request = match prepare(conversation) {
            Ok(request) => request,
            Err(message) => {
                emit(vec![AgentEvent::Error {
                    message: message.clone(),
                }]);
                return Err(message);
            }
        };
        let context = conversation.select(&request.selection);
        let mut response = ResponseAccumulator::new();
        let mut response_error = None;
        let mut on_model_events = |events: Vec<ModelEvent>| {
            let batch = events
                .into_iter()
                .map(|event| {
                    if response_error.is_none() {
                        if let Err(error) = response.push(event.clone()) {
                            response_error = Some(error.to_string());
                        }
                    }
                    AgentEvent::Model { event }
                })
                .collect::<Vec<_>>();
            if !batch.is_empty() {
                emit(batch);
            }
        };
        if let Err(error) = model
            .generate(&context, &request.tools, &mut on_model_events)
            .await
        {
            emit(vec![AgentEvent::Error {
                message: error.clone(),
            }]);
            return Err(error);
        }
        if let Some(error) = response_error {
            emit(vec![AgentEvent::Error {
                message: error.clone(),
            }]);
            return Err(error);
        }
        let usage = response.usage;
        let assistant = match response.finish() {
            Ok(entry) => entry,
            Err(error) => {
                let message = error.to_string();
                emit(vec![AgentEvent::Error {
                    message: message.clone(),
                }]);
                return Err(message);
            }
        };
        let has_calls = matches!(&assistant, ContextEntry::Assistant { blocks } if blocks.iter().any(|block| matches!(block, AssistantBlock::ToolCall { .. })));
        emit(vec![AgentEvent::Assistant {
            entry: assistant.clone(),
        }]);
        let assistant_index = conversation.entries.len();
        record_assistant(conversation, assistant, executor, cwd, &mut |entry| {
            emit(vec![AgentEvent::ToolResult {
                entry: entry.clone(),
            }]);
        })
        .await
        .expect("response accumulator produced an assistant entry");
        if let Some((input, output, total)) = usage {
            conversation.last_usage = Some(UsageAnchor {
                entry_index: assistant_index,
                tokens: total
                    .filter(|tokens| *tokens > 0)
                    .unwrap_or_else(|| input.saturating_add(output)),
            });
        }
        if let Some(usage) = context_window.and_then(|window| conversation.context_usage(window)) {
            emit(vec![AgentEvent::ContextUsage { usage }]);
        }
        if !has_calls {
            emit(vec![AgentEvent::Done]);
            return Ok(());
        }
    }
    let message = "model call limit reached".to_owned();
    emit(vec![AgentEvent::Error {
        message: message.clone(),
    }]);
    Err(message)
}
