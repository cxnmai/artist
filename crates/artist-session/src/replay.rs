use crate::{
    Settings,
    storage::{self, StoredRecord},
};
use artist_core::{
    context::{AssistantBlock, ContextEntry, Conversation},
    journal::SessionRecord,
};

pub(crate) fn change_settings(conversation: &mut Conversation, old: &Settings, new: &Settings) {
    if old.provider != new.provider || old.model != new.model {
        conversation.entries.retain_mut(|entry| {
            if let ContextEntry::Assistant { blocks } = entry {
                blocks.retain(|block| {
                    !matches!(
                        block,
                        AssistantBlock::ProviderData { .. } | AssistantBlock::Thinking { .. }
                    )
                });
                return !blocks.is_empty();
            }
            true
        });
    }
    if old != new {
        conversation.last_usage = None;
    }
}

pub(crate) fn apply(conversation: &mut Conversation, record: SessionRecord) {
    match record {
        SessionRecord::Message { entry } => conversation.entries.push(entry),
        SessionRecord::Compaction { context, .. } => *conversation = context,
        SessionRecord::Usage { anchor } => conversation.last_usage = anchor,
        SessionRecord::Terminal { .. } => {}
    }
}

pub(crate) async fn load(
    conn: &turso::Connection,
    id: &str,
) -> Result<
    (
        std::path::PathBuf,
        Settings,
        Conversation,
        Vec<StoredRecord>,
    ),
    String,
> {
    let mut header = None;
    let mut conversation = Conversation::default();
    let mut pending: Vec<String> = Vec::new();
    let mut after = 0;
    loop {
        let page = storage::page(conn, id, after, 256).await?;
        if page.is_empty() {
            break;
        }
        for (sequence, record) in page {
            after = sequence;
            match record {
                StoredRecord::Header {
                    cwd,
                    settings,
                    system_prompt,
                } if header.is_none() && sequence == 1 => {
                    conversation.system_prompt = system_prompt;
                    header = Some((cwd, settings));
                }
                StoredRecord::Header { .. } => {
                    return Err("invalid duplicate session header".into());
                }
                StoredRecord::Settings { settings } => {
                    let (_, previous) = header.as_mut().ok_or("settings before session header")?;
                    change_settings(&mut conversation, previous, &settings);
                    *previous = settings;
                }
                StoredRecord::Journal { record } => {
                    if header.is_none() {
                        return Err("record before session header".into());
                    }
                    if let SessionRecord::Message { entry } = &record {
                        match entry {
                            ContextEntry::Assistant { blocks } => {
                                for block in blocks {
                                    if let AssistantBlock::ToolCall { id, .. } = block {
                                        pending.push(id.clone());
                                    }
                                }
                            }
                            ContextEntry::ToolResult { tool_call_id, .. } => {
                                pending.retain(|id| id != tool_call_id)
                            }
                            _ => {}
                        }
                    }
                    apply(&mut conversation, record);
                }
            }
        }
    }
    let (cwd, settings) = header.ok_or("missing session header record")?;
    let recovery = pending
        .into_iter()
        .map(|tool_call_id| StoredRecord::Journal {
            record: SessionRecord::Message {
                entry: ContextEntry::ToolResult {
                    tool_call_id,
                    text:
                        "Tool execution interrupted; outcome unknown. The tool was not re-executed."
                            .into(),
                    is_error: true,
                },
            },
        })
        .collect();
    Ok((cwd, settings, conversation, recovery))
}
