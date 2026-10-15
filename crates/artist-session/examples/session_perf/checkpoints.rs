//! Regression probe for the checkpoint fast path versus a canonical index rebuild.
use crate::{Result, settings};
use artist_core::{
    context::{AssistantBlock, ContextEntry, Conversation},
    context_usage::UsageAnchor,
    journal::{Journal, SessionRecord},
};
use artist_session::Session;
use std::path::Path;

#[path = "checkpoint_fixture.rs"]
mod fixture;
use fixture::assistant;

pub async fn run(root: &Path) -> Result<()> {
    let fixture::Fixture {
        directory,
        cwd,
        initial,
        id,
    } = fixture::seed(root).await?;
    let mut session = Session::open(
        Some(&directory),
        Some(&id),
        &cwd,
        settings(),
        initial.clone(),
    )
    .await?;
    let mut switched = settings();
    switched.model = "different-model".into();
    session.set_settings(switched.clone()).await?;
    session
        .append_batch(vec![
            SessionRecord::Message {
                entry: assistant(true),
            },
            SessionRecord::Usage {
                anchor: Some(UsageAnchor {
                    entry_index: 2,
                    tokens: 123,
                }),
            },
        ])
        .await?;
    drop(session); // Unresolved suffix tool call must be recovered, not re-executed.
    let session = Session::open(
        Some(&directory),
        Some(&id),
        &cwd,
        settings(),
        initial.clone(),
    )
    .await?;
    assert_eq!(session.settings, switched);
    assert_eq!(session.conversation.entries.len(), 4);
    assert_eq!(
        session.conversation.last_usage.as_ref().unwrap().tokens,
        123
    );
    assert!(
        matches!(&session.conversation.entries[1], ContextEntry::Assistant { blocks } if blocks.len() == 1)
    );
    assert!(
        matches!(&session.conversation.entries[3], ContextEntry::ToolResult { text, is_error: true, .. } if text.contains("outcome unknown"))
    );
    let expected = serde_json::to_value(&session.conversation)?;
    let transcript = serde_json::to_value(session.transcript().await?)?;
    drop(session);
    artist_session::rebuild_resume_index(Some(&directory), &id).await?;
    let mut session = Session::open(
        Some(&directory),
        Some(&id),
        &cwd,
        settings(),
        initial.clone(),
    )
    .await?;
    assert_eq!(serde_json::to_value(&session.conversation)?, expected);
    assert_eq!(
        serde_json::to_value(session.transcript().await?)?,
        transcript
    );
    switched.reasoning = Some("high".into());
    session.set_settings(switched.clone()).await?;
    drop(session);
    let session = Session::open(
        Some(&directory),
        Some(&id),
        &cwd,
        settings(),
        initial.clone(),
    )
    .await?;
    assert!(session.conversation.last_usage.is_none());
    assert!(
        matches!(&session.conversation.entries[2], ContextEntry::Assistant { blocks } if blocks.iter().any(|block| matches!(block, AssistantBlock::ProviderData { .. })))
    );
    let suffix = ContextEntry::User {
        text: "after checkpoint in batch".into(),
    };
    session
        .append_batch(vec![
            SessionRecord::Compaction {
                context: Conversation {
                    system_prompt: initial.system_prompt.clone(),
                    entries: vec![ContextEntry::Summary {
                        text: "second checkpoint".into(),
                    }],
                    last_usage: None,
                },
                summary: "second checkpoint".into(),
                removed_entries: 4,
            },
            SessionRecord::Message {
                entry: suffix.clone(),
            },
            SessionRecord::Usage {
                anchor: Some(UsageAnchor {
                    entry_index: 1,
                    tokens: 88,
                }),
            },
        ])
        .await?;
    drop(session);
    let session = Session::open(Some(&directory), Some(&id), &cwd, settings(), initial).await?;
    assert_eq!(session.conversation.entries.len(), 2);
    assert_eq!(session.settings, switched);
    assert_eq!(session.conversation.last_usage.as_ref().unwrap().tokens, 88);
    let mut final_transcript = transcript;
    final_transcript
        .as_array_mut()
        .unwrap()
        .push(serde_json::to_value(suffix)?);
    assert_eq!(
        serde_json::to_value(session.transcript().await?)?,
        final_transcript
    );
    println!(
        "{}",
        serde_json::json!({"check": "indexed_checkpoint_settings_usage_tool_recovery_and_rebuild", "passed": true})
    );
    Ok(())
}
