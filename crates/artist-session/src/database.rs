use crate::{
    paths,
    storage::{self, error},
};
use fs2::FileExt;
use std::{fs::File, path::Path};

pub(crate) struct Database {
    pub connection: turso::Connection,
    // Keep the database open before releasing the OS lock.
    _engine: turso::Database,
    _lock: File,
}

pub(crate) async fn open(directory: &Path, create: bool) -> Result<Database, String> {
    if create {
        paths::private_dir(directory)?;
    } else {
        paths::restrict(directory, true)?;
    }
    let directory = std::fs::canonicalize(directory).map_err(error)?;
    let lock_path = directory.join("database.lock");
    // All callers run the complete operation on a blocking storage worker.
    let lock = paths::private_file(&lock_path, false)?;
    lock.lock_exclusive()
        .map_err(|e| format!("cannot lock session database: {e}"))?;
    let path = directory.join("sessions.db");
    if !path.exists() {
        if !create {
            return Err("session database does not exist".into());
        }
        paths::private_file(&path, true)?;
    }
    paths::restrict(&path, false)?;
    // Turso otherwise creates its WAL with ambient umask permissions. Precreate
    // it privately while holding the operation lock, before opening the engine.
    paths::private_file(&directory.join("sessions.db-wal"), false)?;
    // Default Turso WAL is single-process. Experimental multiprocess WAL is
    // deliberately not enabled: this short OS lock covers every reader and writer.
    let engine =
        turso::Builder::new_local(path.to_str().ok_or("session database path is not UTF-8")?)
            .build()
            .await
            .map_err(error)?;
    let mut connection = engine.connect().map_err(error)?;
    storage::configure(&connection).await?;
    schema(&mut connection).await?;
    checkpoint(&connection, &directory.join("sessions.db-wal")).await?;
    paths::private_dir(&directory.join("locks"))?;
    paths::sync_dir(&directory)?;
    if let Some(parent) = directory.parent() {
        paths::sync_dir(parent)?;
    }
    Ok(Database {
        connection,
        _engine: engine,
        _lock: lock,
    })
}

async fn checkpoint(conn: &turso::Connection, wal: &Path) -> Result<(), String> {
    // Reopening Turso 0.8.3 resets its in-memory backfill count. Default PASSIVE
    // auto-checkpoints backfill pages but cannot bound this reopen-per-op WAL.
    // Reclaim at 4 MiB before serving the next operation, under the global lock;
    // small reads only stat the WAL. One large write can exceed this threshold
    // until the next open. Keep FULL synchronous mode for checkpoint durability.
    const WAL_CHECKPOINT_BYTES: u64 = 4 * 1024 * 1024;
    if std::fs::metadata(wal).map_err(error)?.len() < WAL_CHECKPOINT_BYTES {
        return Ok(());
    }
    let mut rows = conn
        .query("PRAGMA wal_checkpoint(TRUNCATE)", ())
        .await
        .map_err(error)?;
    let row = rows
        .next()
        .await
        .map_err(error)?
        .ok_or("missing Turso WAL checkpoint result")?;
    // Turso reports checkpoint errors as busy=1 (other columns can be NULL).
    if row.get::<i64>(0).map_err(error)? != 0 {
        return Err("Turso WAL checkpoint failed or was busy".into());
    }
    let log = row.get::<i64>(1).map_err(error)?;
    let checkpointed = row.get::<i64>(2).map_err(error)?;
    // Unlike SQLite, Turso 0.8.3 can report pre-truncation frame counts here.
    if log < 0 || checkpointed != log {
        return Err(format!(
            "incomplete Turso WAL checkpoint: log={log}, checkpointed={checkpointed}"
        ));
    }
    if rows.next().await.map_err(error)?.is_some() {
        return Err("unexpected Turso WAL checkpoint result".into());
    }
    if std::fs::metadata(wal).map_err(error)?.len() != 0 {
        return Err("Turso WAL checkpoint did not truncate the WAL".into());
    }
    Ok(())
}

async fn schema(conn: &mut turso::Connection) -> Result<(), String> {
    // The operation OS lock serializes this short schema transaction.
    let version = storage::integer_pragma(conn, "PRAGMA user_version").await?;
    match version {
        2 => return Ok(()),
        0 | 1 => {}
        version => return Err(format!("unsupported session schema version: {version}")),
    }
    let tx = conn.transaction().await.map_err(error)?;
    if version == 0 {
        tx.execute("CREATE TABLE sessions (id TEXT PRIMARY KEY, cwd TEXT NOT NULL, started_at INTEGER NOT NULL, last_used_at INTEGER NOT NULL, settings TEXT NOT NULL)", ()).await.map_err(error)?;
        tx.execute(
            "CREATE INDEX sessions_cwd_last_used ON sessions (cwd, last_used_at DESC)",
            (),
        )
        .await
        .map_err(error)?;
        tx.execute("CREATE TABLE records (session_id TEXT NOT NULL, sequence INTEGER NOT NULL, timestamp INTEGER NOT NULL, data TEXT NOT NULL, PRIMARY KEY (session_id, sequence))", ()).await.map_err(error)?;
    }
    // v1 archives are indexed lazily; migration never scans canonical history.
    tx.execute("CREATE TABLE resume_points (session_id TEXT PRIMARY KEY, sequence INTEGER NOT NULL, settings TEXT NOT NULL, pending TEXT NOT NULL)", ()).await.map_err(error)?;
    tx.execute("PRAGMA user_version = 2", ())
        .await
        .map_err(error)?;
    tx.commit().await.map_err(error)
}
