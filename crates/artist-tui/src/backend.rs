//! Configured backend ownership and typed events shared with the frontend.

use artist_core::cancellation::CancellationToken;
use std::path::{Path, PathBuf};

use artist_core::context::Conversation;
use artist_core::engine::AgentEvent;
use artist_lua::Runtime;
use artist_lua::model_info::ModelInfo;

use crate::args::Options;

pub struct Backend {
    runtime: Runtime,
    cwd: PathBuf,
    conversation: Conversation,
    provider: String,
    model: ModelInfo,
    reasoning: Option<String>,
}

impl Backend {
    pub async fn load(options: &Options, cwd: &Path) -> Result<Option<Self>, String> {
        let runtime = Runtime::new(options.config.as_deref())?;
        if !runtime.has_model() {
            if options.model.is_some() || options.reasoning.is_some() {
                return Err(
                    "configure a provider before choosing a model or reasoning level".into(),
                );
            }
            return Ok(None);
        }
        let (provider, models) = runtime.model_registry().await?;
        let selected = runtime.select_model(options.model.as_deref())?;
        let model = models
            .into_iter()
            .find(|model| model.id == selected)
            .ok_or_else(|| format!("model {selected} is not in the provider registry"))?;
        let reasoning = options
            .reasoning
            .clone()
            .or(model.default_reasoning.clone());
        if let Some(level) = &reasoning {
            if model
                .reasoning_levels
                .as_ref()
                .is_none_or(|levels| !levels.contains(level))
            {
                return Err(format!(
                    "reasoning level {level} is not listed as supported by {selected}"
                ));
            }
        }
        let mut conversation = Conversation::default();
        runtime.configure_conversation(&mut conversation, cwd, &selected)?;
        Ok(Some(Self {
            runtime,
            cwd: cwd.to_owned(),
            conversation,
            provider,
            model,
            reasoning,
        }))
    }

    pub async fn run_prompt(
        &mut self,
        prompt: String,
        cancellation: &CancellationToken,
        emit: impl FnMut(Vec<AgentEvent>),
    ) {
        let mapped = self.reasoning.as_deref().map(|level| {
            self.model
                .reasoning_map
                .as_ref()
                .and_then(|map| map.get(level))
                .map(String::as_str)
                .unwrap_or(level)
        });
        // Runtime emits its terminal event on both success and failure.
        let _ = self
            .runtime
            .run(
                &mut self.conversation,
                prompt,
                &self.model.id,
                self.model.adapter.as_deref(),
                mapped,
                self.model.thinking_format.as_deref(),
                self.model.requires_reasoning_content.unwrap_or(false),
                self.model.context_window,
                &self.cwd,
                cancellation,
                emit,
            )
            .await;
    }

    pub fn initial_events(&self) -> Vec<AgentEvent> {
        let mut events = vec![AgentEvent::ModelInfo {
            provider: self.provider.clone(),
            model: self.model.id.clone(),
            context_window: self.model.context_window,
            max_output_tokens: self.model.max_output_tokens,
            reasoning_levels: self.model.reasoning_levels.clone(),
            reasoning_level: self.reasoning.clone(),
        }];
        if let Some(usage) = self
            .model
            .context_window
            .and_then(|window| self.conversation.context_usage(window))
        {
            events.push(AgentEvent::ContextUsage { usage });
        }
        events
    }
}
