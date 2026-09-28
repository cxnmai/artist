//! Tool execution is an interface so native and future Lua tools use the same path.

use crate::context::{AssistantBlock, ContextEntry, Conversation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

pub struct ToolOutput {
    pub text: String,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: false,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: true,
        }
    }
}

pub trait ToolExecutor {
    fn execute<'a>(
        &'a self,
        name: &'a str,
        arguments: &'a Value,
        cwd: &'a Path,
    ) -> Pin<Box<dyn Future<Output = ToolOutput> + 'a>>;
}

/// Append the assistant response, then run its tool calls in order and append each result.
pub async fn record_assistant(
    conversation: &mut Conversation,
    assistant: ContextEntry,
    executor: &dyn ToolExecutor,
    cwd: &Path,
    on_result: &mut dyn FnMut(&ContextEntry),
) -> Result<(), &'static str> {
    let ContextEntry::Assistant { blocks } = assistant else {
        return Err("expected an assistant entry");
    };
    let calls: Vec<_> = blocks
        .iter()
        .filter_map(|block| {
            if let AssistantBlock::ToolCall {
                id,
                name,
                arguments,
            } = block
            {
                Some((id.clone(), name.clone(), arguments.clone()))
            } else {
                None
            }
        })
        .collect();
    conversation
        .entries
        .push(ContextEntry::Assistant { blocks });
    for (tool_call_id, name, arguments) in calls {
        let output = executor.execute(&name, &arguments, cwd).await;
        let result = ContextEntry::ToolResult {
            tool_call_id,
            text: output.text,
            is_error: output.is_error,
        };
        on_result(&result);
        conversation.entries.push(result);
    }
    Ok(())
}
