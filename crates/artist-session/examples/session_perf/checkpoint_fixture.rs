use crate::{Result, settings};
use artist_core::{
    context::{AssistantBlock, ContextEntry, Conversation},
    journal::{Journal, SessionRecord},
};
use artist_session::Session;
use std::path::{Path, PathBuf};

pub struct Fixture {
    pub directory: PathBuf,
    pub cwd: PathBuf,
    pub initial: Conversation,
    pub id: String,
}

pub fn assistant(tool: bool) -> ContextEntry {
    let mut blocks = vec![
        AssistantBlock::Thinking {
            text: "reasoning".into(),
        },
        AssistantBlock::ProviderData {
            provider: "fixture".into(),
            data: serde_json::json!({"opaque": true}),
        },
        AssistantBlock::Text {
            text: "retained response".into(),
        },
    ];
    if tool {
        blocks.push(AssistantBlock::ToolCall {
            id: "reused-call".into(),
            name: "never_execute".into(),
            arguments: serde_json::json!({}),
        });
    }
    ContextEntry::Assistant { blocks }
}

pub async fn seed(root: &Path) -> Result<Fixture> {
    let directory = root.join("checkpoint-regression");
    let cwd = std::env::current_dir()?;
    let initial = Conversation {
        system_prompt: "stable prompt".into(),
        ..Conversation::default()
    };
    let session = Session::open(Some(&directory), None, &cwd, settings(), initial.clone()).await?;
    let id = session.id.clone();
    session
        .append_batch(vec![
            SessionRecord::Message {
                entry: ContextEntry::User {
                    text: "archived original".into(),
                },
            },
            SessionRecord::Message {
                entry: assistant(true),
            },
            SessionRecord::Message {
                entry: ContextEntry::ToolResult {
                    tool_call_id: "reused-call".into(),
                    text: "completed".into(),
                    is_error: false,
                },
            },
        ])
        .await?;
    session
        .append(SessionRecord::Compaction {
            context: Conversation {
                system_prompt: initial.system_prompt.clone(),
                entries: vec![
                    ContextEntry::Summary {
                        text: "first checkpoint".into(),
                    },
                    assistant(false),
                ],
                last_usage: None,
            },
            summary: "first checkpoint".into(),
            removed_entries: 1,
        })
        .await?;
    drop(session);
    Ok(Fixture {
        directory,
        cwd,
        initial,
        id,
    })
}
