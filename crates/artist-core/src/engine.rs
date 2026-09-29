//! One user turn, independent of model providers and tool implementations.

use crate::cancellation::CancellationToken;
use crate::context::{
    AssistantBlock, ContextEntry, ContextSelection, Conversation, SelectedContext,
};
use crate::context_usage::{ContextUsage, UsageAnchor};
use crate::display::{DisplayEntry, DisplayModelEvent};
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

/// Frontend-safe events. Protocol replay items are only stored in `Conversation`.
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
        event: DisplayModelEvent,
    },
    Assistant {
        entry: DisplayEntry,
    },
    ToolResult {
        entry: DisplayEntry,
    },
    Error {
        message: String,
    },
    Cancelled,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnOutcome {
    Completed,
    Cancelled,
}

/// Calls `prepare` before *each* model request; it can select history and tools anew.
/// Every normal return emits exactly one terminal event. Cancel through the token,
/// not by dropping this future, to preserve tool-result pairing and notification.
pub async fn run_turn(
    conversation: &mut Conversation,
    prompt: String,
    model: &mut dyn Model,
    executor: &dyn ToolExecutor,
    cwd: &Path,
    max_model_calls: usize,
    context_window: Option<u64>,
    cancellation: &CancellationToken,
    mut prepare: impl FnMut(&Conversation) -> Result<TurnRequest, String>,
    mut emit: impl FnMut(Vec<AgentEvent>),
) -> Result<TurnOutcome, String> {
    let outcome = run_turn_inner(
        conversation,
        prompt,
        model,
        executor,
        cwd,
        max_model_calls,
        context_window,
        cancellation,
        &mut prepare,
        &mut emit,
    )
    .await;
    let terminal = match &outcome {
        Ok(TurnOutcome::Completed) => AgentEvent::Done,
        Ok(TurnOutcome::Cancelled) => AgentEvent::Cancelled,
        Err(message) => AgentEvent::Error {
            message: message.clone(),
        },
    };
    emit(vec![terminal]);
    outcome
}

async fn run_turn_inner(
    conversation: &mut Conversation,
    prompt: String,
    model: &mut dyn Model,
    executor: &dyn ToolExecutor,
    cwd: &Path,
    max_model_calls: usize,
    context_window: Option<u64>,
    cancellation: &CancellationToken,
    prepare: &mut impl FnMut(&Conversation) -> Result<TurnRequest, String>,
    emit: &mut impl FnMut(Vec<AgentEvent>),
) -> Result<TurnOutcome, String> {
    conversation.entries.push(ContextEntry::User {
        text: prompt.clone(),
    });
    emit(vec![AgentEvent::User { text: prompt }]);
    if let Some(usage) = context_window.and_then(|window| conversation.context_usage(window)) {
        emit(vec![AgentEvent::ContextUsage { usage }]);
    }

    for _ in 0..max_model_calls {
        if cancellation.is_cancelled() {
            return Ok(TurnOutcome::Cancelled);
        }
        let request = match prepare(conversation) {
            Ok(request) => request,
            Err(message) => {
                return Err(message);
            }
        };
        if cancellation.is_cancelled() {
            return Ok(TurnOutcome::Cancelled);
        }
        let context = conversation.select(&request.selection);
        let mut response = ResponseAccumulator::new();
        let mut response_error = None;
        let mut on_model_events = |events: Vec<ModelEvent>| {
            let batch = events
                .into_iter()
                .filter_map(|event| {
                    if response_error.is_none() {
                        if let Err(error) = response.push(event.clone()) {
                            response_error = Some(error.to_string());
                        }
                    }
                    DisplayModelEvent::from_model(event).map(|event| AgentEvent::Model { event })
                })
                .collect::<Vec<_>>();
            if !batch.is_empty() {
                emit(batch);
            }
        };
        let generation = tokio::select! {
            biased;
            _ = cancellation.cancelled() => None,
            result = model.generate(&context, &request.tools, &mut on_model_events) => Some(result),
        };
        if generation.is_none() || cancellation.is_cancelled() {
            return Ok(TurnOutcome::Cancelled);
        }
        if let Err(error) = generation.expect("generation completed") {
            return Err(error);
        }
        if let Some(error) = response_error {
            return Err(error);
        }
        let usage = response.usage;
        let assistant = match response.finish() {
            Ok(entry) => entry,
            Err(error) => {
                let message = error.to_string();
                return Err(message);
            }
        };
        if cancellation.is_cancelled() {
            return Ok(TurnOutcome::Cancelled);
        }
        let has_calls = matches!(&assistant, ContextEntry::Assistant { blocks } if blocks.iter().any(|block| matches!(block, AssistantBlock::ToolCall { .. })));
        emit(vec![AgentEvent::Assistant {
            entry: DisplayEntry::from(&assistant),
        }]);
        let assistant_index = conversation.entries.len();
        let interrupted = record_assistant(
            conversation,
            assistant,
            executor,
            cwd,
            cancellation,
            &mut |entry| {
                emit(vec![AgentEvent::ToolResult {
                    entry: DisplayEntry::from(entry),
                }]);
            },
        )
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
        if interrupted || cancellation.is_cancelled() {
            return Ok(TurnOutcome::Cancelled);
        }
        if !has_calls {
            return Ok(TurnOutcome::Completed);
        }
    }
    let message = "model call limit reached".to_owned();
    Err(message)
}
