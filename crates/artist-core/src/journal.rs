//! Durable canonical records, separate from frontend event projections.
use crate::context::{ContextEntry, Conversation};
use crate::context_usage::UsageAnchor;
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionRecord {
    Message { entry: ContextEntry },
    /// A context projection checkpoint. Original message records remain untouched.
    /// The storage sequence immediately preceding this record is its history boundary.
    Compaction {
        context: Conversation,
        summary: String,
        removed_entries: usize,
    },
    Usage { anchor: Option<UsageAnchor> },
    Terminal { outcome: String },
}

pub trait Journal {
    /// Return only once the record is durably committed. Failure stops further work.
    fn append<'a>(
        &'a self,
        record: SessionRecord,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>>;

    /// Stores supporting transactions should override this to commit the batch atomically.
    fn append_batch<'a>(
        &'a self,
        records: Vec<SessionRecord>,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>> {
        Box::pin(async move {
            for record in records {
                self.append(record).await?;
            }
            Ok(())
        })
    }
}

pub async fn append_batch(journal: Option<&dyn Journal>, records: Vec<SessionRecord>) -> Result<(), String> {
    if let Some(journal) = journal {
        journal.append_batch(records).await?;
    }
    Ok(())
}

pub async fn append(journal: Option<&dyn Journal>, record: SessionRecord) -> Result<(), String> {
    if let Some(journal) = journal {
        journal.append(record).await?;
    }
    Ok(())
}
