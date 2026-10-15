//! Quantify head-of-line blocking: a large resume versus another session's writes.
use crate::{Result, initial, metrics, open, settings};
use artist_core::{
    context::ContextEntry,
    journal::{Journal, SessionRecord},
};
use artist_session::Session;
use std::{path::Path, time::Duration};

pub async fn run(root: &Path) -> Result<()> {
    let directory = root.join("history-100000");
    if !directory.exists() {
        return Ok(());
    }
    let cwd = std::env::current_dir()?;
    let id = artist_session::list(Some(&directory), &cwd).await?[0]
        .id
        .clone();
    let writer = open(&directory, None, 0).await?;
    let reader = tokio::spawn(async move {
        for _ in 0..3 {
            drop(Session::open(Some(&directory), Some(&id), &cwd, settings(), initial(0)).await?);
        }
        Ok::<_, String>(())
    });
    tokio::time::sleep(Duration::from_millis(5)).await;
    metrics::measure(
        "append_during_100k_compacted_resume",
        100_000,
        30,
        || async {
            Ok(writer
                .append(SessionRecord::Message {
                    entry: ContextEntry::User {
                        text: "other session".into(),
                    },
                })
                .await?)
        },
    )
    .await?;
    reader.await??;
    Ok(())
}
