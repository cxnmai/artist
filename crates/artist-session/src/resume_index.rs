use crate::{
    replay,
    replay_state::{ReplayState, ResumePoint},
    storage::{StoredRecord, error},
};
use artist_core::journal::SessionRecord;
use turso::Connection;

fn decode_point(row: &turso::Row) -> Option<ResumePoint> {
    let sequence = row.get::<i64>(0).ok()?;
    if sequence <= 1 {
        return None;
    }
    Some(ResumePoint {
        sequence,
        settings: serde_json::from_str(&row.get::<String>(1).ok()?).ok()?,
        pending: serde_json::from_str(&row.get::<String>(2).ok()?).ok()?,
    })
}

async fn record(
    conn: &Connection,
    id: &str,
    sequence: i64,
) -> Result<Option<StoredRecord>, String> {
    let row = conn
        .query(
            "SELECT data FROM records WHERE session_id = ?1 AND sequence = ?2",
            turso::params![id, sequence],
        )
        .await
        .map_err(error)?
        .next()
        .await
        .map_err(error)?;
    row.map(|row| serde_json::from_str(&row.get::<String>(0).map_err(error)?).map_err(error))
        .transpose()
}

async fn seed(conn: &Connection, id: &str) -> Result<Option<ReplayState>, String> {
    let row = conn
        .query(
            "SELECT sequence, settings, pending FROM resume_points WHERE session_id = ?1",
            [id],
        )
        .await
        .map_err(error)?
        .next()
        .await
        .map_err(error)?;
    let Some(point) = row.as_ref().and_then(decode_point) else {
        return Ok(None);
    };
    // A malformed canonical record is an error, not a cache miss.
    let Some(StoredRecord::Journal {
        record: SessionRecord::Compaction { context, .. },
    }) = record(conn, id, point.sequence).await?
    else {
        return Ok(None);
    };
    let header = record(conn, id, 1)
        .await?
        .ok_or("missing session header record")?;
    let mut state = ReplayState::default();
    state.push(1, header)?;
    let (cwd, _) = state.header.take().ok_or("missing session header record")?;
    state.header = Some((cwd, point.settings.clone()));
    state.conversation = context;
    state.pending = point.pending.clone();
    state.after = point.sequence;
    state.point = Some(point);
    Ok(Some(state))
}

async fn save(conn: &Connection, id: &str, point: Option<&ResumePoint>) -> Result<(), String> {
    if let Some(point) = point {
        conn.execute(
            "INSERT INTO resume_points (session_id, sequence, settings, pending) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (session_id) DO UPDATE SET sequence = excluded.sequence, settings = excluded.settings, pending = excluded.pending",
            turso::params![id, point.sequence, serde_json::to_string(&point.settings).map_err(error)?, serde_json::to_string(&point.pending).map_err(error)?],
        ).await.map_err(error)?;
    } else {
        conn.execute("DELETE FROM resume_points WHERE session_id = ?1", [id])
            .await
            .map_err(error)?;
    }
    Ok(())
}

// Call inside the caller's transaction so canonical appends and this derived
// boundary are committed together. `force` ignores even a valid cached point.
pub(crate) async fn refresh(
    conn: &Connection,
    id: &str,
    force: bool,
) -> Result<ReplayState, String> {
    let state = if force { None } else { seed(conn, id).await? };
    let state = replay::scan(conn, id, state.unwrap_or_default()).await?;
    save(conn, id, state.point.as_ref()).await?;
    Ok(state)
}
