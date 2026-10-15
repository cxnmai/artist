use crate::{Result, initial, metrics, open, reads, settings};
use artist_core::{
    context::{ContextEntry, Conversation},
    journal::{Journal, SessionRecord},
};
use artist_session::Session;
use std::{path::Path, time::Instant};

pub async fn history(root: &Path, count: usize) -> Result<()> {
    let directory = root.join(format!("history-{count}"));
    let start = Instant::now();
    let session = open(&directory, None, count).await?;
    metrics::report("seed_history", count, vec![start.elapsed()]);
    let id = session.id.clone();
    let entry = ContextEntry::User {
        text: "a".repeat(1000),
    };
    metrics::measure("durable_append", count, 30, || async {
        Ok(session
            .append(SessionRecord::Message {
                entry: entry.clone(),
            })
            .await?)
    })
    .await?;
    reads::store(&directory, &session, count).await?;
    drop(session);
    metrics::measure("resume", count, 5, || async {
        open(&directory, Some(&id), 0).await
    })
    .await?;
    let session = open(&directory, Some(&id), 0).await?;
    let checkpoint = SessionRecord::Compaction {
        context: Conversation {
            system_prompt: session.conversation.system_prompt.clone(),
            entries: vec![ContextEntry::Summary {
                text: "short checkpoint".into(),
            }],
            last_usage: None,
        },
        summary: "short checkpoint".into(),
        removed_entries: count,
    };
    session.append(checkpoint).await?;
    drop(session);
    metrics::measure("resume_compacted", count, 5, || async {
        let session = open(&directory, Some(&id), 0).await?;
        assert_eq!(session.conversation.entries.len(), 1);
        Ok(session)
    })
    .await?;
    reads::jsonl(&directory, count)?;
    metrics::file_sizes(&directory, count)?;
    Ok(())
}

pub async fn listing(root: &Path) -> Result<()> {
    let directory = root.join("listing");
    let cwd = std::env::current_dir()?;
    let other_cwd = root.to_owned();
    let start = Instant::now();
    for index in 0..1000 {
        let cwd = if index % 10 == 0 { &cwd } else { &other_cwd };
        drop(Session::open(Some(&directory), None, cwd, settings(), initial(0)).await?);
        if index == 99 || index == 999 {
            metrics::measure("list_matching_10_percent", index + 1, 10, || async {
                let result =
                    artist_session::list(Some(&directory), &std::env::current_dir()?).await?;
                assert_eq!(result.len(), (index + 1) / 10);
                Ok(result)
            })
            .await?;
        }
    }
    metrics::report("create_1000_sessions", 1000, vec![start.elapsed()]);
    Ok(())
}
