//! Backend ownership and typed status events; no model generation yet.

use std::path::Path;

use artist_core::context::Conversation;
use artist_core::engine::AgentEvent;
use artist_lua::Runtime;
use artist_lua::model_info::ModelInfo;

use crate::args::Options;

pub struct Backend {
    // Keep the configured runtime alive for future prompt handling.
    _runtime: Runtime,
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
            _runtime: runtime,
            conversation,
            provider,
            model,
            reasoning,
        }))
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
