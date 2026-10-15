//! Timer responsiveness on the same current-thread executor as storage calls.
use crate::{Result, metrics};
use artist_core::{
    context::ContextEntry,
    journal::{Journal, SessionRecord},
};
use artist_session::Session;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub async fn measure(session: &Session, count: usize) -> Result<()> {
    let alive = Arc::new(AtomicBool::new(true));
    let running = alive.clone();
    let heartbeat = tokio::spawn(async move {
        let mut samples = Vec::new();
        let mut last = Instant::now();
        while running.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(1)).await;
            samples.push(last.elapsed());
            last = Instant::now();
        }
        samples
    });
    tokio::task::yield_now().await;
    for _ in 0..10 {
        session
            .append(SessionRecord::Message {
                entry: ContextEntry::User {
                    text: "heartbeat".into(),
                },
            })
            .await?;
    }
    alive.store(false, Ordering::Relaxed);
    let samples = heartbeat.await?;
    metrics::report("executor_timer_interval_1ms", count, samples);
    Ok(())
}
