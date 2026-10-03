//! Tool execution is an interface so native and future Lua tools use the same path.

use crate::cancellation::CancellationToken;
use crate::context::{AssistantBlock, ContextEntry, Conversation};
use crate::tool_ui::ToolUi;
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
    pub ui: Option<ToolUi>,
}

impl ToolOutput {
    pub fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: false,
            ui: None,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: true,
            ui: None,
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
    cancellation: &CancellationToken,
    on_result: &mut dyn FnMut(&ContextEntry, Option<ToolUi>),
) -> Result<bool, &'static str> {
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
    let mut cancelled = false;
    for (tool_call_id, name, arguments) in calls {
        let output = if cancelled {
            ToolOutput::error("tool not run: turn cancelled")
        } else {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => {
                    cancelled = true;
                    ToolOutput::error("tool interrupted by cancellation; side effects may have occurred")
                }
                output = executor.execute(&name, &arguments, cwd) => output,
            }
        };
        let result = ContextEntry::ToolResult {
            tool_call_id,
            text: output.text,
            is_error: output.is_error,
        };
        on_result(&result, output.ui);
        conversation.entries.push(result);
    }
    Ok(cancelled || cancellation.is_cancelled())
}
