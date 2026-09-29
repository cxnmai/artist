//! Provider-independent conversation history.

use crate::context_usage::UsageAnchor;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Conversation {
    /// Empty unless configured by the caller.
    pub system_prompt: String,
    pub entries: Vec<ContextEntry>,
    #[serde(default)]
    pub last_usage: Option<UsageAnchor>,
}

/// Indices refer to the original conversation, not the filtered result.
/// None means all entries; an absent block map entry means all its blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextSelection {
    pub include_system_prompt: bool,
    pub included_entries: Option<BTreeSet<usize>>,
    pub included_blocks: BTreeMap<usize, BTreeSet<usize>>,
}

impl Default for ContextSelection {
    fn default() -> Self {
        Self {
            include_system_prompt: true,
            included_entries: None,
            included_blocks: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SelectedContext {
    pub system_prompt: String,
    pub entries: Vec<ContextEntry>,
}

impl Conversation {
    /// Build one model request's context without changing stored history.
    pub fn select(&self, selection: &ContextSelection) -> SelectedContext {
        let entries = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                if selection
                    .included_entries
                    .as_ref()
                    .is_some_and(|indices| !indices.contains(&index))
                {
                    return None;
                }
                match entry {
                    ContextEntry::Assistant { blocks } => {
                        let blocks = blocks
                            .iter()
                            .enumerate()
                            .filter_map(|(block_index, block)| {
                                selection
                                    .included_blocks
                                    .get(&index)
                                    .is_none_or(|indices| indices.contains(&block_index))
                                    .then(|| block.clone())
                            })
                            .collect::<Vec<_>>();
                        (!blocks.is_empty()).then_some(ContextEntry::Assistant { blocks })
                    }
                    _ => Some(entry.clone()),
                }
            })
            .collect();
        SelectedContext {
            system_prompt: if selection.include_system_prompt {
                self.system_prompt.clone()
            } else {
                String::new()
            },
            entries,
        }
    }
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
    /// Provider-owned response item, replayed unchanged by that provider.
    ProviderData {
        provider: String,
        data: Value,
    },
}
