//! Context occupancy: latest measured request plus estimated trailing messages.

use crate::context::{AssistantBlock, ContextEntry, Conversation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageAnchor {
    /// Index of the assistant entry whose response reported this usage.
    pub entry_index: usize,
    /// Input (including cache reads/writes) plus output tokens for that request.
    pub tokens: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextUsage {
    pub tokens: u64,
    pub context_window: u64,
    pub percent: f64,
    pub estimated_trailing_tokens: u64,
}

impl Conversation {
    pub fn context_usage(&self, context_window: u64) -> Option<ContextUsage> {
        if context_window == 0 {
            return None;
        }
        let anchor = self.last_usage.as_ref().filter(|a| {
            matches!(
                self.entries.get(a.entry_index),
                Some(ContextEntry::Assistant { .. })
            )
        });
        let (measured, start) = match anchor {
            Some(anchor) => (anchor.tokens, anchor.entry_index + 1),
            None => (estimate_chars(text_chars(&self.system_prompt)), 0),
        };
        let trailing = self.entries[start..]
            .iter()
            .fold(0u64, |sum, entry| sum.saturating_add(estimate_entry(entry)));
        let tokens = measured.saturating_add(trailing);
        Some(ContextUsage {
            tokens,
            context_window,
            percent: (tokens as f64 * 100.0) / context_window as f64,
            estimated_trailing_tokens: trailing,
        })
    }
}

fn text_chars(text: &str) -> usize {
    text.encode_utf16().count()
}

fn estimate_chars(chars: usize) -> u64 {
    (chars as u64).saturating_add(3) / 4
}

pub fn estimate_entry(entry: &ContextEntry) -> u64 {
    match entry {
        ContextEntry::Summary { text }
        | ContextEntry::User { text }
        | ContextEntry::ToolResult { text, .. } => estimate_chars(text_chars(text)),
        ContextEntry::Assistant { blocks } => {
            let chars: usize = blocks
                .iter()
                .map(|block| match block {
                    AssistantBlock::Text { text } | AssistantBlock::Thinking { text } => {
                        text_chars(text)
                    }
                    AssistantBlock::ToolCall {
                        name, arguments, ..
                    } => text_chars(name) + text_chars(&arguments.to_string()),
                    // Raw protocol items duplicate the normalized blocks; do not count twice.
                    AssistantBlock::ProviderData { .. } => 0,
                })
                .sum();
            estimate_chars(chars)
        }
    }
}
