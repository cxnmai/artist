//! Embedded Lua policy and the bridges into Artist's Rust interfaces.

mod catalog;
mod model;
pub mod model_info;
mod prompt;
mod provider_config;
mod registry;
mod tools;

use artist_core::cancellation::CancellationToken;
use artist_core::context::{ContextSelection, Conversation};
use artist_core::engine::{AgentEvent, TurnOutcome, TurnRequest, run_turn};
use artist_model::ModelClient;
use mlua::{Lua, LuaSerdeExt, Value as LuaValue};
use model::LuaModel;
use registry::Registry;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use tools::LuaTools;

pub struct Runtime {
    lua: Lua,
    registry: Rc<RefCell<Registry>>,
    client: ModelClient,
}

impl Runtime {
    pub fn new(config: Option<&Path>) -> Result<Self, String> {
        let lua = Lua::new();
        let registry = Rc::new(RefCell::new(Registry::default()));
        registry::install(&lua, Rc::clone(&registry)).map_err(|e| e.to_string())?;
        prompt::install(&lua, Rc::clone(&registry)).map_err(|e| e.to_string())?;
        tools::install_native(&lua).map_err(|e| e.to_string())?;
        lua.load(include_str!("default.lua"))
            .set_name("artist/default.lua")
            .exec()
            .map_err(|e| e.to_string())?;
        if let Some(path) = config {
            let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            lua.load(&source)
                .set_name(path.to_string_lossy())
                .exec()
                .map_err(|e| e.to_string())?;
        }
        Ok(Self {
            lua,
            registry,
            client: ModelClient::new(),
        })
    }

    pub fn has_model(&self) -> bool {
        self.registry.borrow().provider.is_some()
    }

    pub async fn model_registry(&self) -> Result<(String, Vec<model_info::ModelInfo>), String> {
        let provider = self
            .registry
            .borrow()
            .provider
            .clone()
            .ok_or("no provider configured")?;
        let models = catalog::list(&self.lua, &provider).await?;
        for info in &models {
            if let Some(adapter) = &info.adapter {
                if !provider.adapters.contains_key(adapter) {
                    return Err(format!(
                        "model {} refers to unknown adapter {adapter}",
                        info.id
                    ));
                }
            }
        }
        Ok((provider.name, models))
    }

    pub fn select_model(&self, requested: Option<&str>) -> Result<String, String> {
        let registry = self.registry.borrow();
        let provider = registry.provider.as_ref().ok_or("no provider configured")?;
        let name = requested
            .or(provider.default_model.as_deref())
            .or_else(|| (provider.models.len() == 1).then(|| provider.models[0].as_str()))
            .ok_or("select a model with --model or set default_model")?;
        if name.is_empty()
            || (provider.discover.is_none() && !provider.models.iter().any(|id| id == name))
        {
            return Err(format!(
                "model {name} is not in provider {}'s configured models",
                provider.name
            ));
        }
        Ok(name.into())
    }

    pub fn configure_conversation(
        &self,
        conversation: &mut Conversation,
        cwd: &Path,
        model_name: &str,
    ) -> Result<(), String> {
        conversation.system_prompt =
            prompt::build(&self.lua, &self.registry, cwd, model_name).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn run(
        &self,
        conversation: &mut Conversation,
        prompt: String,
        model_name: &str,
        adapter_name: Option<&str>,
        reasoning_level: Option<&str>,
        thinking_format: Option<&str>,
        requires_reasoning_content: bool,
        context_window: Option<u64>,
        cwd: &Path,
        cancellation: &CancellationToken,
        mut emit: impl FnMut(Vec<AgentEvent>),
    ) -> Result<TurnOutcome, String> {
        let provider = self.registry.borrow().provider.clone();
        let Some(provider) = provider else {
            let message = "No provider configured".to_owned();
            emit(vec![AgentEvent::Error {
                message: message.clone(),
            }]);
            return Err(message);
        };
        let mut model = LuaModel {
            lua: &self.lua,
            provider,
            model_name: model_name.into(),
            adapter_name: adapter_name.map(str::to_owned),
            reasoning_level: reasoning_level.map(str::to_owned),
            thinking_format: thinking_format.map(str::to_owned),
            requires_reasoning_content,
            client: self.client.clone(),
        };
        let executor = LuaTools {
            lua: &self.lua,
            registry: Rc::clone(&self.registry),
        };
        run_turn(
            conversation,
            prompt,
            &mut model,
            &executor,
            cwd,
            20,
            context_window,
            cancellation,
            |history| {
                let registry = self.registry.borrow();
                let selection = if let Some(selector) = &registry.selector {
                    let history = self.lua.to_value(history).map_err(|e| e.to_string())?;
                    let value: LuaValue = selector.call(history).map_err(|e| e.to_string())?;
                    self.lua
                        .from_value::<ContextSelection>(value)
                        .map_err(|e| e.to_string())?
                } else {
                    ContextSelection::default()
                };
                Ok(TurnRequest {
                    selection,
                    tools: registry
                        .tools
                        .iter()
                        .map(|tool| tool.definition.clone())
                        .collect(),
                })
            },
            emit,
        )
        .await
    }
}
