//! Frontend-safe projections; provider replay data stays in conversation history.

use crate::context::{AssistantBlock, ContextEntry};
mod model_event;
pub use model_event::DisplayModelEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DisplayBlock {
    Thinking {
        text: String,
    },
    Text {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
}

/// A committed display message. Native protocol replay blocks are never included.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DisplayEntry {
    User {
        text: String,
    },
    Assistant {
        blocks: Vec<DisplayBlock>,
    },
    ToolResult {
        tool_call_id: String,
        text: String,
        is_error: bool,
    },
}

impl From<&ContextEntry> for DisplayEntry {
    fn from(entry: &ContextEntry) -> Self {
        match entry {
            ContextEntry::User { text } => Self::User { text: text.clone() },
            ContextEntry::ToolResult {
                tool_call_id,
                text,
                is_error,
            } => Self::ToolResult {
                tool_call_id: tool_call_id.clone(),
                text: text.clone(),
                is_error: *is_error,
            },
            ContextEntry::Assistant { blocks } => Self::Assistant {
                blocks: blocks
                    .iter()
                    .filter_map(|block| {
                        Some(match block {
                            AssistantBlock::ProviderData { .. } => return None,
                            AssistantBlock::Text { text } => {
                                DisplayBlock::Text { text: text.clone() }
                            }
                            AssistantBlock::Thinking { text } => {
                                DisplayBlock::Thinking { text: text.clone() }
                            }
                            AssistantBlock::ToolCall {
                                id,
                                name,
                                arguments,
                            } => DisplayBlock::ToolCall {
                                id: id.clone(),
                                name: name.clone(),
                                arguments: arguments.clone(),
                            },
                        })
                    })
                    .collect(),
            },
        }
    }
}
