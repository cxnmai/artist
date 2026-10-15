//! Well-typed corruption of derived state must never invent canonical messages.
use crate::checkpoint_fixture as fixture;
use crate::{Result, settings};
use artist_session::Session;
use fs2::FileExt;
use std::{fs::OpenOptions, path::Path};

async fn tamper(directory: &Path, id: &str, field: &str, payload: &str) -> Result<()> {
    let directory = directory.to_owned();
    let id = id.to_owned();
    let field = field.to_owned();
    let payload = payload.to_owned();
    tokio::task::spawn_blocking(move || -> std::result::Result<(), String> {
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(directory.join("database.lock"))
            .map_err(|e| e.to_string())?;
        lock.lock_exclusive().map_err(|e| e.to_string())?;
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?
            .block_on(async {
                let engine =
                    turso::Builder::new_local(directory.join("sessions.db").to_str().unwrap())
                        .build()
                        .await
                        .map_err(|e| e.to_string())?;
                let conn = engine.connect().map_err(|e| e.to_string())?;
                conn.execute("PRAGMA synchronous = FULL", ())
                    .await
                    .map_err(|e| e.to_string())?;
                conn.execute(
                    &format!("UPDATE resume_points SET {field} = ?1 WHERE session_id = ?2"),
                    turso::params![payload, id],
                )
                .await
                .map_err(|e| e.to_string())?;
                Ok(())
            })
    })
    .await??;
    Ok(())
}

pub async fn run(root: &Path) -> Result<()> {
    let fixture::Fixture {
        directory,
        cwd,
        initial,
        id,
    } = fixture::seed(&root.join("integrity-regression")).await?;
    let session = Session::open(
        Some(&directory),
        Some(&id),
        &cwd,
        settings(),
        initial.clone(),
    )
    .await?;
    let expected = serde_json::to_value(&session.conversation)?;
    let transcript = serde_json::to_value(session.transcript().await?)?;
    drop(session);
    let mut corrupt_settings = settings();
    corrupt_settings.model = "wrong-but-well-typed".into();
    for (field, payload) in [
        ("pending", "[\"nonexistent-tool\"]".to_owned()),
        ("settings", serde_json::to_string(&corrupt_settings)?),
        ("checksum", "damaged".to_owned()),
    ] {
        tamper(&directory, &id, field, &payload).await?;
        let session = Session::open(
            Some(&directory),
            Some(&id),
            &cwd,
            settings(),
            initial.clone(),
        )
        .await?;
        assert_eq!(serde_json::to_value(&session.conversation)?, expected);
        assert_eq!(
            serde_json::to_value(session.transcript().await?)?,
            transcript
        );
    }
    println!(
        "{}",
        serde_json::json!({"check": "well_typed_cache_corruption_cannot_modify_canonical_history", "passed": true})
    );
    Ok(())
}
