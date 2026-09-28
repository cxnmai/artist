//! One user turn, independent of model providers and tool implementations.

use crate::context::{
    AssistantBlock, ContextEntry, ContextSelection, Conversation, SelectedContext,
};
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
pub trait Model: Send {
    fn generate<'a>(
        &'a mut self,
        context: &'a SelectedContext,
        tools: &'a [ToolDefinition],
        emit: &'a mut (dyn FnMut(ModelEvent) + Send),
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    User { text: String },
    Model { event: ModelEvent },
    Assistant { entry: ContextEntry },
    ToolResult { entry: ContextEntry },
    Error { message: String },
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
    mut prepare: impl FnMut(&Conversation) -> TurnRequest,
    mut emit: impl FnMut(AgentEvent) + Send,
) -> Result<(), String> {
    conversation.entries.push(ContextEntry::User {
        text: prompt.clone(),
    });
    emit(AgentEvent::User { text: prompt });

    for _ in 0..max_model_calls {
        let request = prepare(conversation);
        let context = conversation.select(&request.selection);
        let mut response = ResponseAccumulator::new();
        let mut response_error = None;
        let mut on_model_event = |event: ModelEvent| {
            if response_error.is_none() {
                if let Err(error) = response.push(event.clone()) {
                    response_error = Some(error.to_string());
                }
            }
            emit(AgentEvent::Model { event });
        };
        if let Err(error) = model
            .generate(&context, &request.tools, &mut on_model_event)
            .await
        {
            emit(AgentEvent::Error {
                message: error.clone(),
            });
            return Err(error);
        }
        if let Some(error) = response_error {
            emit(AgentEvent::Error {
                message: error.clone(),
            });
            return Err(error);
        }
        let assistant = match response.finish() {
            Ok(entry) => entry,
            Err(error) => {
                let message = error.to_string();
                emit(AgentEvent::Error {
                    message: message.clone(),
                });
                return Err(message);
            }
        };
        let has_calls = matches!(&assistant, ContextEntry::Assistant { blocks } if blocks.iter().any(|block| matches!(block, AssistantBlock::ToolCall { .. })));
        emit(AgentEvent::Assistant {
            entry: assistant.clone(),
        });
        record_assistant(conversation, assistant, executor, cwd, &mut |entry| {
            emit(AgentEvent::ToolResult {
                entry: entry.clone(),
            });
        })
        .await
        .expect("response accumulator produced an assistant entry");
        if !has_calls {
            emit(AgentEvent::Done);
            return Ok(());
        }
    }
    let message = "model call limit reached".to_owned();
    emit(AgentEvent::Error {
        message: message.clone(),
    });
    Err(message)
}
