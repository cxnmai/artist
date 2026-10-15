use crate::{
    database, paths,
    storage::{self, StoredRecord},
};
use artist_core::{display::DisplayEntry, journal::SessionRecord};
use serde::Serialize;
use std::path::Path;

#[derive(Clone, Debug, Serialize)]
pub struct TranscriptItem {
    pub sequence: i64,
    pub entry: DisplayEntry,
}

#[derive(Clone, Debug, Serialize)]
pub struct TranscriptPage {
    pub items: Vec<TranscriptItem>,
    /// Continue after this record sequence; None means the end was reached.
    /// A page may be empty when its scanned records contain only metadata/events.
    pub next_after: Option<i64>,
}

/// Read safe canonical messages without acquiring the session's lifetime writer lock.
/// `limit` bounds scanned records (clamped to 1..=1000), not just display messages.
pub async fn transcript(
    directory: Option<&Path>,
    id: &str,
    after: i64,
    limit: u32,
) -> Result<TranscriptPage, String> {
    let directory = paths::directory(directory)?;
    paths::lock_path(&directory, id)?;
    let database = database::open(&directory, false).await?;
    storage::info(&database.connection, id).await?;
    let limit = limit.clamp(1, 1000);
    let records = storage::page(&database.connection, id, after.max(0), limit).await?;
    let next_after = if records.len() == limit as usize {
        records.last().map(|(sequence, _)| *sequence)
    } else {
        None
    };
    let items = records
        .into_iter()
        .filter_map(|(sequence, record)| {
            if let StoredRecord::Journal {
                record: SessionRecord::Message { entry },
            } = record
            {
                Some(TranscriptItem {
                    sequence,
                    entry: DisplayEntry::from(&entry),
                })
            } else {
                None
            }
        })
        .collect();
    Ok(TranscriptPage { items, next_after })
}
