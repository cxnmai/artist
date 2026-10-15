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
    conn: &mut turso::Connection,
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
    let tx = conn.transaction().await.map_err(storage::error)?;
    let state = crate::resume_index::refresh(&tx, id, false).await?;
    tx.commit().await.map_err(storage::error)?;
    let recovery = state.recovery();
    let (cwd, settings) = state.header.ok_or("missing session header record")?;
    Ok((cwd, settings, state.conversation, recovery))
}

pub(crate) async fn scan(
    conn: &turso::Connection,
    id: &str,
    mut state: crate::replay_state::ReplayState,
) -> Result<crate::replay_state::ReplayState, String> {
    loop {
        let page = storage::page(conn, id, state.after, 256).await?;
        if page.is_empty() {
            break;
        }
        for (sequence, record) in page {
            state.push(sequence, record)?;
        }
    }
    if state.header.is_none() {
        return Err("missing session header record".into());
    }
    Ok(state)
}
