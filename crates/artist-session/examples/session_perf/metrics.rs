use crate::Result;
use std::{
    future::Future,
    time::{Duration, Instant},
};

pub fn report(operation: &str, size: usize, mut samples: Vec<Duration>) {
    samples.sort();
    let micros = |duration: Duration| duration.as_secs_f64() * 1_000_000.0;
    let percentile = |n: usize| micros(samples[(samples.len() - 1) * n / 100]);
    println!(
        "{}",
        serde_json::json!({
            "operation": operation, "size": size, "samples": samples.len(),
            "p50_us": percentile(50), "p95_us": percentile(95),
            "max_us": micros(*samples.last().unwrap()),
            "mean_us": samples.iter().map(|d| micros(*d)).sum::<f64>() / samples.len() as f64,
        })
    );
}

pub async fn measure<T, F: Future<Output = Result<T>>>(
    operation: &str,
    size: usize,
    repetitions: usize,
    mut action: impl FnMut() -> F,
) -> Result<()> {
    let mut samples = Vec::new();
    for _ in 0..repetitions {
        let start = Instant::now();
        let value = action().await?;
        std::hint::black_box(&value);
        samples.push(start.elapsed());
    }
    report(operation, size, samples);
    Ok(())
}

pub fn file_sizes(directory: &std::path::Path, size: usize) -> Result<()> {
    let size_of = |name| {
        std::fs::metadata(directory.join(name))
            .map(|m| m.len())
            .unwrap_or(0)
    };
    println!(
        "{}",
        serde_json::json!({"operation": "file_sizes", "size": size,
            "db_bytes": size_of("sessions.db"), "wal_bytes": size_of("sessions.db-wal"),
            "jsonl_bytes": size_of("comparison.jsonl"),
        })
    );
    Ok(())
}
