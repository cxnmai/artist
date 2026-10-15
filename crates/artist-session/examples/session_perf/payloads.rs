//! Large native replay blocks are retained on disk but omitted from display pages.
use crate::{Result, metrics, settings};
use artist_core::context::{AssistantBlock, ContextEntry, Conversation};
use artist_session::Session;
use std::path::Path;

pub async fn run(root: &Path) -> Result<()> {
    let directory = root.join("native-payloads");
    let entries = (0..20)
        .map(|_| ContextEntry::Assistant {
            blocks: vec![
                AssistantBlock::Text {
                    text: "visible".into(),
                },
                AssistantBlock::ProviderData {
                    provider: "fixture".into(),
                    data: serde_json::Value::String("n".repeat(256 * 1024)),
                },
            ],
        })
        .collect();
    let session = Session::open(
        Some(&directory),
        None,
        &std::env::current_dir()?,
        settings(),
        Conversation {
            entries,
            ..Conversation::default()
        },
    )
    .await?;
    metrics::measure("transcript_5MiB_native_payload", 20, 10, || async {
        let page = artist_session::transcript(Some(&directory), &session.id, 0, 100).await?;
        assert_eq!(page.items.len(), 20);
        assert!(serde_json::to_vec(&page)?.len() < 10_000);
        Ok(page)
    })
    .await?;
    metrics::measure("metadata_with_5MiB_native_payload", 20, 10, || async {
        Ok(artist_session::metadata(Some(&directory), &session.id).await?)
    })
    .await?;
    metrics::file_sizes(&directory, 20)?;
    Ok(())
}
