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
    active: RefCell<Option<CancellationToken>>,
}

pub fn internal(message: impl ToString) -> Error {
    Error::internal_error().data(serde_json::json!(message.to_string()))
}

pub fn invalid(message: impl ToString) -> Error {
    Error::invalid_params().data(serde_json::json!(message.to_string()))
}

impl Session {
    pub async fn new(options: &Options, cwd: PathBuf) -> Result<Self> {
        if !cwd.is_absolute() || !cwd.is_dir() {
            return Err(invalid("cwd must be an existing absolute directory"));
        }
        let runtime = Runtime::new_with_stderr(options.config.as_deref()).map_err(internal)?;
        let model = runtime
            .select_model(options.model.as_deref())
            .map_err(internal)?;
        let (_, registry) = runtime.model_registry().await.map_err(internal)?;
        let info = registry
            .into_iter()
            .find(|info| info.id == model)
            .ok_or_else(|| internal(format!("model {model} is not in the provider registry")))?;
        let reasoning = options.reasoning.clone().or(info.default_reasoning.clone());
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
        let mut conversation = Conversation::default();
        runtime
            .configure_conversation(&mut conversation, &cwd, &model)
            .map_err(internal)?;
        Ok(Self {
            runtime,
            info,
            reasoning,
            cwd,
            conversation: Mutex::new(conversation),
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
