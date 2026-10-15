//! Run with: cargo run --release -p artist-session --example session_perf -- [new-data-root]
//! Isolated, warm-cache microbenchmarks; never opens the default session store.
#[path = "session_perf/concurrency.rs"]
mod concurrency;
#[path = "session_perf/heartbeat.rs"]
mod heartbeat;
#[path = "session_perf/metrics.rs"]
mod metrics;
#[path = "session_perf/payloads.rs"]
mod payloads;
#[path = "session_perf/reads.rs"]
mod reads;
#[path = "session_perf/scaling.rs"]
mod scaling;

use artist_core::context::{AssistantBlock, ContextEntry, Conversation};
use artist_session::{Session, Settings};
use std::{path::Path, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn settings() -> Settings {
    Settings {
        provider: "perf".into(),
        model: "fixture".into(),
        reasoning: None,
    }
}

fn initial(count: usize) -> Conversation {
    Conversation {
        system_prompt: "Performance fixture".into(),
        entries: (0..count)
            .map(|index| {
                let text = format!("{index}: {}", "x".repeat(1000));
                if index % 2 == 0 {
                    ContextEntry::User { text }
                } else {
                    ContextEntry::Assistant {
                        blocks: vec![AssistantBlock::Text { text }],
                    }
                }
            })
            .collect(),
        last_usage: None,
    }
}

async fn open(directory: &Path, id: Option<&str>, count: usize) -> Result<Session> {
    Ok(Session::open(
        Some(directory),
        id,
        &std::env::current_dir()?,
        settings(),
        initial(count),
    )
    .await?)
}

async fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|arg| arg == "--child") {
        return concurrency::child(Path::new(&args[2]), args[3].to_str().unwrap().parse()?).await;
    }
    let root = args
        .get(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("artist-session-perf-{}", uuid::Uuid::new_v4()))
        });
    std::fs::create_dir(&root)?; // Refuse to overwrite or reuse existing data.
    println!(
        "{}",
        serde_json::json!({"environment": {
            "root": root, "optimized": !cfg!(debug_assertions), "message_bytes": 1000,
            "cache": "warm OS cache; FULL synchronous commits; no cache dropping",
            "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        }})
    );
    let start = Instant::now();
    let max_history: usize = std::env::var("ARTIST_PERF_MAX_HISTORY")
        .unwrap_or_else(|_| "100000".into())
        .parse()?;
    for count in [100, 1000, 10_000, 100_000]
        .into_iter()
        .filter(|count| *count <= max_history)
    {
        scaling::history(&root, count).await?;
    }
    payloads::run(&root).await?;
    scaling::listing(&root).await?;
    concurrency::run(&root).await?;
    println!(
        "{}",
        serde_json::json!({"total_seconds": start.elapsed().as_secs_f64()})
    );
    Ok(())
}

fn main() -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run())
}
