use super::*;

pub async fn metadata(directory: Option<&Path>, id: &str) -> Result<(PathBuf, Settings), String> {
    let directory = paths::directory(directory)?;
    paths::lock_path(&directory, id)?;
    let id = id.to_owned();
    blocking::run(move || async move {
        let database = database::open(&directory, false).await?;
        let info = storage::info(&database.connection, &id).await?;
        Ok((info.cwd, info.settings))
    })
    .await
}

/// List metadata for this exact cwd, most recently used first, using the cwd index.
pub async fn list(directory: Option<&Path>, cwd: &Path) -> Result<Vec<SessionInfo>, String> {
    let directory = paths::directory(directory)?;
    let cwd = cwd.to_owned();
    blocking::run(move || async move {
        if !directory.join("sessions.db").exists() {
            return Ok(Vec::new());
        }
        let database = database::open(&directory, false).await?;
        storage::list(&database.connection, &cwd).await
    })
    .await
}

/// Rebuild the derived resume index by replaying canonical history in full.
/// This never changes messages or acquires the session's lifetime writer lock.
pub async fn rebuild_resume_index(directory: Option<&Path>, id: &str) -> Result<(), String> {
    let directory = paths::directory(directory)?;
    paths::lock_path(&directory, id)?;
    let id = id.to_owned();
    blocking::run(move || async move {
        let mut database = database::open(&directory, false).await?;
        storage::info(&database.connection, &id).await?;
        let tx = database
            .connection
            .transaction()
            .await
            .map_err(storage::error)?;
        crate::resume_index::refresh(&tx, &id, true).await?;
        tx.commit().await.map_err(storage::error)
    })
    .await
}

impl Session {
    async fn persist(
        &self,
        records: Vec<StoredRecord>,
        settings: Option<Settings>,
    ) -> Result<(), String> {
        let mut writer = self.writer.lock().await;
        if writer.failed {
            return Err("session writer stopped after a persistence failure".into());
        }
        // A dropped COMMIT future can already have made records durable. Poison
        // before any await so cancellation also requires reopening and replaying.
        writer.failed = true;
        let directory = writer.directory.clone();
        let id = self.id.clone();
        let lock = Arc::clone(&self._writer_lock);
        let result = blocking::run(move || async move {
            // Keep the session locked through completion even if the awaiting
            // caller drops its future or the entire Session in the meantime.
            let _lock = lock;
            let mut database = database::open(&directory, false).await?;
            storage::append(&mut database.connection, &id, &records, settings.as_ref()).await
        })
        .await;
        if result.is_ok() {
            writer.failed = false;
        }
        result
    }

    pub async fn set_settings(&mut self, settings: Settings) -> Result<(), String> {
        self.persist(
            vec![StoredRecord::Settings {
                settings: settings.clone(),
            }],
            Some(settings.clone()),
        )
        .await?;
        replay::change_settings(&mut self.conversation, &self.settings, &settings);
        self.settings = settings;
        Ok(())
    }

    /// Canonical message history, including messages excluded by compaction.
    pub async fn transcript(&self) -> Result<Vec<ContextEntry>, String> {
        let writer = self.writer.lock().await;
        let directory = writer.directory.clone();
        let id = self.id.clone();
        blocking::run(move || async move {
            let database = database::open(&directory, false).await?;
            let connection = &database.connection;
            let mut after = 0;
            let mut entries = Vec::new();
            loop {
                let page = storage::page(connection, &id, after, 256).await?;
                if page.is_empty() {
                    break;
                }
                for (sequence, record) in page {
                    after = sequence;
                    if let StoredRecord::Journal {
                        record: SessionRecord::Message { entry },
                    } = record
                    {
                        entries.push(entry);
                    }
                }
            }
            Ok(entries)
        })
        .await
    }
}

impl Journal for Session {
    fn append<'a>(
        &'a self,
        record: SessionRecord,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>> {
        Box::pin(async move {
            self.persist(vec![StoredRecord::Journal { record }], None)
                .await
        })
    }
    fn append_batch<'a>(
        &'a self,
        records: Vec<SessionRecord>,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>> {
        Box::pin(async move {
            let records = records
                .into_iter()
                .map(|record| StoredRecord::Journal { record })
                .collect::<Vec<_>>();
            self.persist(records, None).await
        })
    }
}
