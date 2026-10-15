use crate::{SessionInfo, Settings};
use artist_core::journal::SessionRecord;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use turso::Connection;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum StoredRecord {
    Header {
        cwd: PathBuf,
        settings: Settings,
        system_prompt: String,
    },
    Settings {
        settings: Settings,
    },
    Journal {
        record: SessionRecord,
    },
}

pub(crate) fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
pub(crate) fn now() -> Result<i64, String> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(error)?
            .as_millis(),
    )
    .map_err(error)
}

pub(crate) async fn configure(conn: &Connection) -> Result<(), String> {
    // turso_core 0.8.3 storage/pager.rs::commit_wal fsyncs before publishing
    // FULL commits. Check both pragmas rather than silently accepting a no-op.
    conn.execute("PRAGMA synchronous = FULL", ())
        .await
        .map_err(error)?;
    if integer_pragma(conn, "PRAGMA synchronous").await? != 2 {
        return Err("Turso did not enable FULL synchronous commits".into());
    }
    let mut rows = conn.query("PRAGMA journal_mode", ()).await.map_err(error)?;
    let mode = rows
        .next()
        .await
        .map_err(error)?
        .ok_or("missing journal mode")?
        .get::<String>(0)
        .map_err(error)?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(format!("unsupported Turso journal mode: {mode}"));
    }

    Ok(())
}

pub(crate) async fn integer_pragma(conn: &Connection, sql: &str) -> Result<i64, String> {
    conn.query(sql, ())
        .await
        .map_err(error)?
        .next()
        .await
        .map_err(error)?
        .ok_or("missing pragma result")?
        .get::<i64>(0)
        .map_err(error)
}

pub(crate) async fn check_version(conn: &Connection) -> Result<(), String> {
    let version = integer_pragma(conn, "PRAGMA user_version").await?;
    if version != 1 {
        return Err(format!("unsupported session schema version: {version}"));
    }
    Ok(())
}

pub(crate) async fn initialize(
    conn: &mut Connection,
    info: &SessionInfo,
    records: &[StoredRecord],
) -> Result<(), String> {
    let tx = conn.transaction().await.map_err(error)?;
    tx.execute(
        "INSERT INTO sessions VALUES (?1, ?2, ?3, ?4, ?5)",
        turso::params![
            info.id.clone(),
            serde_json::to_string(&info.cwd).map_err(error)?,
            info.started_at,
            info.last_used_at,
            serde_json::to_string(&info.settings).map_err(error)?
        ],
    )
    .await
    .map_err(error)?;
    insert_records(&tx, &info.id, records).await?;
    tx.commit().await.map_err(error)
}

async fn insert_records(
    tx: &turso::transaction::Transaction<'_>,
    id: &str,
    records: &[StoredRecord],
) -> Result<(), String> {
    if records.is_empty() {
        return Ok(());
    }
    // Seek the composite primary key once; wrapped MAX scans history in Turso 0.8.3.
    let mut sequence = {
        let mut rows = tx
            .query(
                "SELECT sequence FROM records WHERE session_id = ?1 ORDER BY sequence DESC LIMIT 1",
                [id],
            )
            .await
            .map_err(error)?;
        match rows.next().await.map_err(error)? {
            Some(row) => row.get::<i64>(0).map_err(error)?,
            None => 0,
        }
    };
    let mut insert = tx
        .prepare(
            "INSERT INTO records (session_id, sequence, timestamp, data) VALUES (?1, ?2, ?3, ?4)",
        )
        .await
        .map_err(error)?;
    for record in records {
        sequence = sequence.checked_add(1).ok_or("record sequence overflow")?;
        insert
            .execute(turso::params![
                id,
                sequence,
                now()?,
                serde_json::to_string(record).map_err(error)?
            ])
            .await
            .map_err(error)?;
    }
    Ok(())
}

pub(crate) async fn append(
    conn: &mut Connection,
    id: &str,
    records: &[StoredRecord],
    settings: Option<&Settings>,
) -> Result<(), String> {
    let tx = conn.transaction().await.map_err(error)?;
    insert_records(&tx, id, records).await?;
    tx.execute(
        "UPDATE sessions SET last_used_at = MAX(last_used_at, ?1) WHERE id = ?2",
        turso::params![now()?, id],
    )
    .await
    .map_err(error)?;
    if let Some(settings) = settings {
        tx.execute(
            "UPDATE sessions SET settings = ?1 WHERE id = ?2",
            turso::params![serde_json::to_string(settings).map_err(error)?, id],
        )
        .await
        .map_err(error)?;
    }
    tx.commit().await.map_err(error)
}

pub(crate) use crate::queries::{info, list, page};
