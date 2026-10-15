use crate::{Options, events::map_events};
use agent_client_protocol::{
    Client, Error, PromptResponse, Result, SessionId, SessionNotification, StopReason,
};
use artist_core::{cancellation::CancellationToken, context::Conversation, engine::TurnOutcome};
use artist_lua::{Runtime, model_info::ModelInfo};
use std::{cell::RefCell, path::PathBuf};
use tokio::sync::{Mutex, mpsc};

pub struct Session {
    runtime: Runtime,
    info: ModelInfo,
    reasoning: Option<String>,
    cwd: PathBuf,
    conversation: Mutex<Conversation>,
    pub store: artist_session::Session,
    active: RefCell<Option<CancellationToken>>,
}

pub fn internal(message: impl ToString) -> Error {
    Error::internal_error().data(serde_json::json!(message.to_string()))
}

pub fn invalid(message: impl ToString) -> Error {
    Error::invalid_params().data(serde_json::json!(message.to_string()))
}

impl Session {
    pub async fn new(options: &Options, cwd: PathBuf, resume: Option<&str>) -> Result<Self> {
        if !cwd.is_absolute() || !cwd.is_dir() {
            return Err(invalid("cwd must be an existing absolute directory"));
        }
        let cwd = std::fs::canonicalize(cwd).map_err(internal)?;
        let runtime = Runtime::new_with_stderr(options.config.as_deref()).map_err(internal)?;
        let directory = runtime.sessions_directory();
        let saved = if let Some(id) = resume {
            Some(artist_session::metadata(directory.as_deref(), id).await.map_err(internal)?)
        } else {
            None
        };
        let (provider, registry) = runtime.model_registry().await.map_err(internal)?;
        if options.model.is_none() && saved.as_ref().is_some_and(|(_, settings)| settings.provider != provider) {
            return Err(invalid("saved provider differs from configuration; select --model explicitly"));
        }
        let requested = options.model.as_deref().or_else(|| saved.as_ref().map(|(_, settings)| settings.model.as_str()));
        let model = runtime.select_model(requested).map_err(internal)?;
        let info = registry
            .into_iter()
            .find(|info| info.id == model)
            .ok_or_else(|| internal(format!("model {model} is not in the provider registry")))?;
        let inherited_reasoning = if options.model.is_none() {
            saved.as_ref().and_then(|(_, settings)| settings.reasoning.clone())
        } else {
            None
        };
        let reasoning = options.reasoning.clone().or(inherited_reasoning).or(info.default_reasoning.clone());
        if let Some(level) = &reasoning {
            if info
                .reasoning_levels
                .as_ref()
                .is_none_or(|levels| !levels.contains(level))
            {
                return Err(invalid(format!(
                    "reasoning level {level} is not supported by {model}"
                )));
            }
        }
        let mut initial = Conversation::default();
        if resume.is_none() {
            runtime.configure_conversation(&mut initial, &cwd, &model).map_err(internal)?;
        }
        let settings = artist_session::Settings { provider, model, reasoning: reasoning.clone() };
        let mut store = artist_session::Session::open(directory.as_deref(), resume, &cwd, settings.clone(), initial).await.map_err(internal)?;
        store.set_settings(settings).await.map_err(internal)?;
        let conversation = std::mem::take(&mut store.conversation);
        Ok(Self {
            runtime,
            info,
            reasoning,
            cwd,
            conversation: Mutex::new(conversation),
            store,
            active: RefCell::new(None),
        })
    }

    pub fn cancel(&self) {
        if let Some(token) = self.active.borrow().as_ref() {
            token.cancel();
        }
    }

    pub async fn prompt(
        &self,
        id: SessionId,
        prompt: String,
        client: &impl Client,
    ) -> Result<PromptResponse> {
        let mut conversation = self
            .conversation
            .try_lock()
            .map_err(|_| invalid("session already has an active prompt"))?;
        let cancellation = CancellationToken::new();
        *self.active.borrow_mut() = Some(cancellation.clone());
        let _active = ActiveTurn(&self.active);
        let reasoning = self.reasoning.as_deref().map(|level| {
            self.info
                .reasoning_map
                .as_ref()
                .and_then(|map| map.get(level))
                .map(String::as_str)
                .unwrap_or(level)
        });
        let (tx, mut rx) = mpsc::unbounded_channel();
        let run = self.runtime.run(
            &mut conversation,
            prompt,
            &self.info.id,
            self.info.adapter.as_deref(),
            reasoning,
            self.info.thinking_format.as_deref(),
            self.info.requires_reasoning_content.unwrap_or(false),
            self.info.context_window,
            &self.cwd,
            &cancellation,
            Some(&self.store),
            move |events| {
                let _ = tx.send(events);
            },
        );
        let forward = async {
            let mut error = None;
            while let Some(events) = rx.recv().await {
                for update in map_events(events) {
                    if error.is_none() {
                        if let Err(err) = client
                            .session_notification(SessionNotification::new(id.clone(), update))
                            .await
                        {
                            cancellation.cancel();
                            error = Some(err);
                        }
                    }
                }
            }
            error.map_or(Ok(()), Err)
        };
        // Drain notifications before replying, and cancel cooperatively on transport failure.
        let (outcome, forwarded) = tokio::join!(run, forward);
        forwarded?;
        Ok(PromptResponse::new(match outcome.map_err(internal)? {
            TurnOutcome::Completed => StopReason::EndTurn,
            TurnOutcome::Cancelled => StopReason::Cancelled,
        }))
    }
}

struct ActiveTurn<'a>(&'a RefCell<Option<CancellationToken>>);

impl Drop for ActiveTurn<'_> {
    fn drop(&mut self) {
        self.0.borrow_mut().take();
    }
}
