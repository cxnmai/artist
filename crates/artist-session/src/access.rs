use super::*;

pub async fn metadata(directory: Option<&Path>, id: &str) -> Result<(PathBuf, Settings), String> {
    let directory = paths::directory(directory)?;
    paths::lock_path(&directory, id)?;
    let database = database::open(&directory, false).await?;
    let connection = &database.connection;
    let info = storage::info(connection, id).await?;
    Ok((info.cwd, info.settings))
}

/// List metadata for this exact cwd, most recently used first, using the cwd index.
pub async fn list(directory: Option<&Path>, cwd: &Path) -> Result<Vec<SessionInfo>, String> {
    let directory = paths::directory(directory)?;
    if !directory.join("sessions.db").exists() {
        return Ok(Vec::new());
    }
    let database = database::open(&directory, false).await?;
    let connection = &database.connection;
    storage::list(connection, cwd).await
}

impl Session {
    async fn persist(
        &self,
        records: &[StoredRecord],
        settings: Option<&Settings>,
    ) -> Result<(), String> {
        let mut writer = self.writer.lock().await;
        if writer.failed {
            return Err("session writer stopped after a persistence failure".into());
        }
        // A dropped COMMIT future can already have made records durable. Poison
        // before any await so cancellation also requires reopening and replaying.
        writer.failed = true;
        let result = {
            match database::open(&writer.directory, false).await {
                Ok(mut database) => {
                    storage::append(&mut database.connection, &self.id, records, settings).await
                }
                Err(error) => Err(error),
            }
        };
        if result.is_ok() {
            writer.failed = false;
        }
        result
    }

    pub async fn set_settings(&mut self, settings: Settings) -> Result<(), String> {
        self.persist(
            &[StoredRecord::Settings {
                settings: settings.clone(),
            }],
            Some(&settings),
        )
        .await?;
        replay::change_settings(&mut self.conversation, &self.settings, &settings);
        self.settings = settings;
        Ok(())
    }

    /// Canonical message history, including messages excluded by compaction.
    pub async fn transcript(&self) -> Result<Vec<ContextEntry>, String> {
        let writer = self.writer.lock().await;
        let database = database::open(&writer.directory, false).await?;
        let connection = &database.connection;
        let mut after = 0;
        let mut entries = Vec::new();
        loop {
            let page = storage::page(connection, &self.id, after, 256).await?;
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
    }
}

impl Journal for Session {
    fn append<'a>(
        &'a self,
        record: SessionRecord,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>> {
        Box::pin(async move {
            self.persist(&[StoredRecord::Journal { record }], None)
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
            self.persist(&records, None).await
        })
    }
}
