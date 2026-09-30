//! Portable text-summary preparation, independent of model providers.

use crate::context::{AssistantBlock, ContextEntry, Conversation};
use crate::context_usage::estimate_entry;

/// Walk backwards retaining whole assistant/tool-result groups. The budget is approximate.
pub fn cut_point(context: &Conversation, keep_tokens: u64) -> Option<usize> {
    let mut cut = context.entries.len();
    let mut tokens = 0_u64;
    while cut > 0 {
        let end = cut;
        cut -= 1;
        while cut > 0 && matches!(context.entries[cut], ContextEntry::ToolResult { .. }) {
            cut -= 1;
        }
        for entry in &context.entries[cut..end] {
            tokens = tokens.saturating_add(estimate_entry(entry));
        }
        if tokens >= keep_tokens {
            break;
        }
    }
    if cut == 0
        || context.entries[..cut]
            .iter()
            .all(|entry| matches!(entry, ContextEntry::Summary { .. }))
    {
        None
    } else {
        Some(cut)
    }
}

/// Serialize visible content, never opaque encrypted/signed replay data.
pub fn serialize(entries: &[ContextEntry]) -> String {
    let mut parts = Vec::new();
    for entry in entries {
        match entry {
            ContextEntry::Summary { text } => parts.push(format!("[Previous summary]: {text}")),
            ContextEntry::User { text } => parts.push(format!("[User]: {text}")),
            ContextEntry::ToolResult { text, is_error, .. } => {
                let bounded: String = text.chars().take(2000).collect();
                let marker = if bounded.len() < text.len() {
                    "\n[remaining tool output omitted]"
                } else {
                    ""
                };
                parts.push(format!(
                    "[Tool result, error={is_error}]: {bounded}{marker}"
                ));
            }
            ContextEntry::Assistant { blocks } => {
                for block in blocks {
                    match block {
                        AssistantBlock::Text { text } => parts.push(format!("[Assistant]: {text}")),
                        AssistantBlock::Thinking { text } => {
                            parts.push(format!("[Assistant thinking]: {text}"))
                        }
                        AssistantBlock::ToolCall {
                            name, arguments, ..
                        } => parts.push(format!("[Tool call]: {name}({arguments})")),
                        AssistantBlock::ProviderData { .. } => {}
                    }
                }
            }
        }
    }
    parts.join("\n\n")
}

pub fn replacement(context: &Conversation, cut: usize, summary: &str) -> Conversation {
    let mut entries = vec![ContextEntry::Summary {
        text: format!(
            "The conversation history before this point was compacted into the following summary:\n\n<summary>\n{summary}\n</summary>"
        ),
    }];
    entries.extend(context.entries[cut..].iter().cloned().map(|mut entry| {
        if let ContextEntry::Assistant { blocks } = &mut entry {
            // A rewritten prefix may invalidate signed/native reasoning. Retain normalized
            // text and tool calls, not opaque artifacts tied to the old conversation.
            blocks.retain(|block| {
                !matches!(
                    block,
                    AssistantBlock::ProviderData { .. } | AssistantBlock::Thinking { .. }
                )
            });
        }
        entry
    }));
    entries
        .retain(|entry| !matches!(entry, ContextEntry::Assistant { blocks } if blocks.is_empty()));
    Conversation {
        system_prompt: context.system_prompt.clone(),
        entries,
        last_usage: None,
    }
}

pub const SYSTEM_PROMPT: &str = "You are a context summarization assistant. Summarize the supplied conversation so another assistant can continue the work. Do not continue the conversation, answer its questions, or call tools. Output only the summary.";
pub const INSTRUCTIONS: &str = "Create a concise context checkpoint with these sections: Goal; Constraints & Preferences; Progress (Done, In Progress, Blocked); Key Decisions; Next Steps; Critical Context. Preserve exact file paths, important identifiers, errors, the user's latest request, and read/modified files. If a previous summary is supplied, update it with the new information rather than dropping relevant prior facts. The recent suffix is retained verbatim, so explain any unfinished work needed to understand it.";
