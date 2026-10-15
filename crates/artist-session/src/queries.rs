use crate::{
    SessionInfo,
    storage::{StoredRecord, check_version, error},
};
use turso::Connection;

pub(crate) async fn info(conn: &Connection, id: &str) -> Result<SessionInfo, String> {
    check_version(conn).await?;
    let row = conn
        .query(
            "SELECT id, cwd, started_at, last_used_at, settings FROM sessions WHERE id = ?1",
            [id],
        )
        .await
        .map_err(error)?
        .next()
        .await
        .map_err(error)?
        .ok_or("missing session header")?;
    decode_info(&row)
}

fn decode_info(row: &turso::Row) -> Result<SessionInfo, String> {
    Ok(SessionInfo {
        id: row.get(0).map_err(error)?,
        cwd: serde_json::from_str(&row.get::<String>(1).map_err(error)?).map_err(error)?,
        started_at: row.get(2).map_err(error)?,
        last_used_at: row.get(3).map_err(error)?,
        settings: serde_json::from_str(&row.get::<String>(4).map_err(error)?).map_err(error)?,
    })
}

pub(crate) async fn page(
    conn: &Connection,
    id: &str,
    after: i64,
    limit: u32,
) -> Result<Vec<(i64, StoredRecord)>, String> {
    let mut rows = conn
        .query(
            "SELECT sequence, data FROM records WHERE session_id = ?1 AND sequence > ?2 ORDER BY sequence LIMIT ?3",
            turso::params![id, after, i64::from(limit.min(1000))],
        )
        .await
        .map_err(error)?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().await.map_err(error)? {
        result.push((
            row.get(0).map_err(error)?,
            serde_json::from_str(&row.get::<String>(1).map_err(error)?).map_err(error)?,
        ));
    }
    Ok(result)
}

pub(crate) async fn list(
    conn: &Connection,
    cwd: &std::path::Path,
) -> Result<Vec<SessionInfo>, String> {
    let mut rows = conn.query(
        "SELECT id, cwd, started_at, last_used_at, settings FROM sessions WHERE cwd = ?1 ORDER BY last_used_at DESC, id",
        [serde_json::to_string(cwd).map_err(error)?],
    ).await.map_err(error)?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().await.map_err(error)? {
        result.push(decode_info(&row)?);
    }
    Ok(result)
}
