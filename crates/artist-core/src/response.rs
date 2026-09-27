//! Assemble a model's incremental events into one assistant entry.

use crate::context::{AssistantBlock, ContextEntry};
use crate::event::ModelEvent;
use serde_json::Value;

#[derive(Debug)]
pub enum ResponseError {
    AfterFinish,
    DuplicateCall(String),
    UnknownCall(String),
    ClosedCall(String),
    IncompleteCall(String),
    InvalidArguments(String, serde_json::Error),
}

impl std::fmt::Display for ResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AfterFinish => write!(f, "model event after response finished"),
            Self::DuplicateCall(id) => write!(f, "duplicate tool call: {id}"),
            Self::UnknownCall(id) => write!(f, "unknown tool call: {id}"),
            Self::ClosedCall(id) => write!(f, "tool call already ended: {id}"),
            Self::IncompleteCall(id) => write!(f, "tool call did not end: {id}"),
            Self::InvalidArguments(id, error) => write!(f, "invalid arguments for {id}: {error}"),
        }
    }
}

impl std::error::Error for ResponseError {}

enum PendingBlock {
    Text(String),
    Thinking(String),
    ToolCall {
        id: String,
        name: String,
        arguments: String,
        ended: bool,
    },
}

#[derive(Default)]
pub struct ResponseAccumulator {
    blocks: Vec<PendingBlock>,
    finished: bool,
    pub usage: Option<(u64, u64)>,
}

impl ResponseAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, event: ModelEvent) -> Result<(), ResponseError> {
        if self.finished && !matches!(event, ModelEvent::Usage { .. }) {
            return Err(ResponseError::AfterFinish);
        }
        match event {
            ModelEvent::TextDelta { text } => match self.blocks.last_mut() {
                Some(PendingBlock::Text(previous)) => previous.push_str(&text),
                _ => self.blocks.push(PendingBlock::Text(text)),
            },
            ModelEvent::ThinkingDelta { text } => match self.blocks.last_mut() {
                Some(PendingBlock::Thinking(previous)) => previous.push_str(&text),
                _ => self.blocks.push(PendingBlock::Thinking(text)),
            },
            ModelEvent::ToolCallStart { id, name } => {
                if self.blocks.iter().any(|block| matches!(block, PendingBlock::ToolCall { id: existing, .. } if *existing == id)) {
                    return Err(ResponseError::DuplicateCall(id));
                }
                self.blocks.push(PendingBlock::ToolCall {
                    id,
                    name,
                    arguments: String::new(),
                    ended: false,
                });
            }
            ModelEvent::ToolCallArgumentsDelta { id, text } => {
                let (arguments, ended) = self.call_mut(&id)?;
                if *ended {
                    return Err(ResponseError::ClosedCall(id));
                }
                arguments.push_str(&text);
            }
            ModelEvent::ToolCallEnd { id } => {
                let (_, ended) = self.call_mut(&id)?;
                if *ended {
                    return Err(ResponseError::ClosedCall(id));
                }
                *ended = true;
            }
            ModelEvent::Usage {
                input_tokens,
                output_tokens,
            } => {
                self.usage = Some((input_tokens, output_tokens));
            }
            ModelEvent::Finished { .. } => self.finished = true,
        }
        Ok(())
    }

    fn call_mut(&mut self, id: &str) -> Result<(&mut String, &mut bool), ResponseError> {
        self.blocks
            .iter_mut()
            .find_map(|block| match block {
                PendingBlock::ToolCall {
                    id: current,
                    arguments,
                    ended,
                    ..
                } if current == id => Some((arguments, ended)),
                _ => None,
            })
            .ok_or_else(|| ResponseError::UnknownCall(id.to_owned()))
    }

    /// Call only after all model events have been received; errors leave history unchanged.
    pub fn finish(self) -> Result<ContextEntry, ResponseError> {
        let mut blocks = Vec::with_capacity(self.blocks.len());
        for block in self.blocks {
            blocks.push(match block {
                PendingBlock::Text(text) => AssistantBlock::Text { text },
                PendingBlock::Thinking(text) => AssistantBlock::Thinking { text },
                PendingBlock::ToolCall {
                    id,
                    name,
                    arguments,
                    ended,
                } => {
                    if !ended {
                        return Err(ResponseError::IncompleteCall(id));
                    }
                    let arguments: Value = serde_json::from_str(if arguments.is_empty() {
                        "{}"
                    } else {
                        &arguments
                    })
                    .map_err(|error| ResponseError::InvalidArguments(id.clone(), error))?;
                    AssistantBlock::ToolCall {
                        id,
                        name,
                        arguments,
                    }
                }
            });
        }
        Ok(ContextEntry::Assistant { blocks })
    }
}
