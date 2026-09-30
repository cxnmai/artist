//! Client-side compaction policy and an atomic replacement of active context.

use crate::context::{AssistantBlock, ContextEntry, Conversation};
use crate::display::DisplayModelEvent;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CompactionSettings {
    pub enabled: bool,
    pub reserve_tokens: u64,
    pub keep_recent_tokens: u64,
    pub max_summary_tokens: u64,
    pub instructions: Option<String>,
}

impl Default for CompactionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            reserve_tokens: 16384,
            keep_recent_tokens: 20000,
            max_summary_tokens: 4096,
            instructions: None,
        }
    }
}

impl CompactionSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.reserve_tokens == 0 || self.keep_recent_tokens == 0 || self.max_summary_tokens == 0
        {
            return Err("compaction token budgets must be positive".into());
        }
        Ok(())
    }

    pub fn validate_window(&self, window: u64) -> Result<(), String> {
        self.validate()?;
        if self.reserve_tokens >= window
            || self
                .keep_recent_tokens
                .saturating_add(self.max_summary_tokens)
                >= window.saturating_sub(self.reserve_tokens)
        {
            return Err("compaction budgets do not fit the selected model's context window".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    pub context: Conversation,
    pub summary: String,
    pub removed_entries: usize,
    #[serde(default)]
    pub usage: Vec<DisplayModelEvent>,
}

impl CompactionResult {
    pub fn validate(&self, previous: &Conversation) -> Result<(), String> {
        if self.summary.trim().is_empty() || self.context.entries.is_empty() {
            return Err("compaction returned an empty summary or context".into());
        }
        if self.removed_entries == 0 || self.removed_entries > previous.entries.len() {
            return Err("compaction returned an invalid removed-entry count".into());
        }
        if self.context.system_prompt != previous.system_prompt {
            return Err("compaction cannot change the system prompt".into());
        }
        if self
            .usage
            .iter()
            .any(|event| !matches!(event, DisplayModelEvent::Usage { .. }))
        {
            return Err("compaction usage must contain only usage events".into());
        }
        validate_tool_pairs(&self.context.entries)
    }
}

/// A checkpoint must not split tool calls from their results or leave calls unresolved.
pub fn validate_tool_pairs(entries: &[ContextEntry]) -> Result<(), String> {
    let mut pending = BTreeSet::new();
    for entry in entries {
        if !matches!(entry, ContextEntry::ToolResult { .. }) && !pending.is_empty() {
            return Err("compaction left unresolved tool calls".into());
        }
        match entry {
            ContextEntry::Assistant { blocks } => {
                for block in blocks {
                    if let AssistantBlock::ToolCall { id, .. } = block {
                        if !pending.insert(id.clone()) {
                            return Err("duplicate tool call in compaction".into());
                        }
                    }
                }
            }
            ContextEntry::ToolResult { tool_call_id, .. } => {
                if !pending.remove(tool_call_id) {
                    return Err("orphan tool result in compaction".into());
                }
            }
            _ => {}
        }
    }
    if !pending.is_empty() {
        return Err("compaction left unresolved tool calls".into());
    }
    Ok(())
}
