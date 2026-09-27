//! Provider-independent conversation history.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Conversation {
    /// Empty unless configured by the caller.
    pub system_prompt: String,
    pub entries: Vec<ContextEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContextEntry {
    User {
        text: String,
    },
    Assistant {
        blocks: Vec<AssistantBlock>,
    },
    ToolResult {
        tool_call_id: String,
        text: String,
        is_error: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssistantBlock {
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
