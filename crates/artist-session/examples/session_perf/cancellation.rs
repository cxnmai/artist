//! Regression probe: a cancelled waiter must not release an in-flight writer.
use crate::{Result, open};
use artist_core::{
    context::ContextEntry,
    journal::{Journal, SessionRecord},
};
use fs2::FileExt;
use std::{fs::OpenOptions, path::Path, time::Duration};

pub async fn run(root: &Path) -> Result<()> {
    let directory = root.join("cancelled-writer");
    let session = open(&directory, None, 0).await?;
    let id = session.id.clone();
    let global_lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.join("database.lock"))?;
    global_lock.lock_exclusive()?;
    let record = || SessionRecord::Message {
        entry: ContextEntry::User {
            text: "cancelled waiter".into(),
        },
    };
    // Poll once: this queues a blocking job behind our held operation lock.
    let mut append = session.append(record());
    std::future::poll_fn(|cx| {
        assert!(append.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    drop(append);
    assert!(
        session.append(record()).await.is_err(),
        "cancelled writer was not poisoned"
    );
    drop(session);
    let writer_lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.join("locks").join(format!("{id}.lock")))?;
    assert!(
        writer_lock.try_lock_exclusive().is_err(),
        "in-flight operation lost its writer lock"
    );
    global_lock.unlock()?;
    let mut finished = false;
    for _ in 0..1000 {
        if writer_lock.try_lock_exclusive().is_ok() {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert!(finished, "storage worker did not finish after lock release");
    writer_lock.unlock()?;
    let restored = open(&directory, Some(&id), 0).await?;
    assert_eq!(restored.conversation.entries.len(), 1);
    println!(
        "{}",
        serde_json::json!({"check": "cancelled_worker_retains_lock_and_requires_replay", "passed": true})
    );
    Ok(())
}
