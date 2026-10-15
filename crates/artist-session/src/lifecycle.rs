use super::*;

impl Session {
    pub async fn open(
        directory: Option<&Path>,
        resume: Option<&str>,
        cwd: &Path,
        settings: Settings,
        initial: Conversation,
    ) -> Result<Self, String> {
        let directory = paths::directory(directory)?;
        let id = resume
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let lock_path = paths::lock_path(&directory, &id)?;
        if resume.is_none() {
            paths::private_dir(&directory)?;
        } else {
            paths::restrict(&directory, true)?;
        }
        paths::private_dir(&directory.join("locks"))?;
        let lock = paths::writer_lock(&lock_path)?;
        let mut database = database::open(&directory, resume.is_none()).await?;
        let connection = &mut database.connection;
        let (cwd, settings, conversation) =
            if resume.is_some() {
                let info = storage::info(connection, &id).await?;
                let (saved_cwd, saved_settings, mut conversation, recovery) =
                    replay::load(connection, &id).await?;
                if info.id != id
                    || saved_cwd != cwd
                    || info.cwd != saved_cwd
                    || info.settings != saved_settings
                {
                    return Err(
                        "session metadata mismatch or session belongs to a different cwd".into(),
                    );
                }
                storage::append(connection, &id, &recovery, None).await?;
                for record in recovery {
                    if let StoredRecord::Journal { record } = record {
                        replay::apply(&mut conversation, record);
                    }
                }
                (saved_cwd, saved_settings, conversation)
            } else {
                let timestamp = storage::now()?;
                let info = SessionInfo {
                    id: id.clone(),
                    cwd: cwd.to_owned(),
                    started_at: timestamp,
                    last_used_at: timestamp,
                    settings: settings.clone(),
                };
                let mut records = vec![StoredRecord::Header {
                    cwd: cwd.to_owned(),
                    settings: settings.clone(),
                    system_prompt: initial.system_prompt.clone(),
                }];
                records.extend(initial.entries.iter().cloned().map(|entry| {
                    StoredRecord::Journal {
                        record: SessionRecord::Message { entry },
                    }
                }));
                records.push(StoredRecord::Journal {
                    record: SessionRecord::Usage {
                        anchor: initial.last_usage.clone(),
                    },
                });
                storage::initialize(connection, &info, &records).await?;
                (cwd.to_owned(), settings, initial)
            };
        drop(database);
        Ok(Self {
            id,
            cwd,
            settings,
            conversation,
            writer: Mutex::new(Writer {
                directory,
                failed: false,
            }),
            _writer_lock: lock,
        })
    }
}
