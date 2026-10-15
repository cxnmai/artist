use crate::{Result, metrics};
use artist_core::context::ContextEntry;
use artist_session::Session;
use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    path::Path,
    time::Instant,
};

pub fn jsonl(directory: &Path, count: usize, entries: &[ContextEntry]) -> Result<()> {
    let path = directory.join("comparison.jsonl");
    let mut writer = BufWriter::new(File::create(&path)?);
    for entry in entries {
        serde_json::to_writer(&mut writer, entry)?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    writer.get_ref().sync_all()?;
    drop(writer);
    for (operation, limit) in [
        ("jsonl_first_100", 100),
        ("jsonl_full_transcript", usize::MAX),
    ] {
        let mut samples = Vec::new();
        for _ in 0..10 {
            let start = Instant::now();
            let entries = BufReader::new(File::open(&path)?)
                .lines()
                .take(limit)
                .map(|line| Ok(serde_json::from_str::<ContextEntry>(&line?)?))
                .collect::<Result<Vec<_>>>()?;
            std::hint::black_box(&entries);
            samples.push(start.elapsed());
        }
        metrics::report(operation, count, samples);
    }
    let mut samples = Vec::new();
    for _ in 0..10 {
        let start = Instant::now();
        let bytes = std::fs::read(&path)?;
        std::hint::black_box(bytes);
        samples.push(start.elapsed());
    }
    metrics::report("jsonl_raw_read_bytes", count, samples);
    Ok(())
}

pub async fn store(directory: &Path, session: &Session, count: usize) -> Result<()> {
    metrics::measure("metadata", count, 10, || async {
        Ok(artist_session::metadata(Some(directory), &session.id).await?)
    })
    .await?;
    metrics::measure("transcript_first_100", count, 10, || async {
        Ok(artist_session::transcript(Some(directory), &session.id, 0, 100).await?)
    })
    .await?;
    metrics::measure("transcript_late_100", count, 10, || async {
        Ok(artist_session::transcript(
            Some(directory),
            &session.id,
            count.saturating_sub(100) as i64,
            100,
        )
        .await?)
    })
    .await?;
    metrics::measure("full_transcript", count, 5, || async {
        Ok(session.transcript().await?)
    })
    .await?;
    Ok(())
}
