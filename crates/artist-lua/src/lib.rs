//! Embedded Lua policy and the bridges into Artist's Rust interfaces.

mod model;
mod registry;
mod tools;

use artist_core::context::{ContextSelection, Conversation};
use artist_core::engine::{AgentEvent, TurnRequest, run_turn};
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
        tools::install_native(&lua).map_err(|e| e.to_string())?;
        lua.load(include_str!("../../../lua/default.lua"))
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
        self.registry.borrow().model.is_some()
    }

    pub async fn run(
        &self,
        prompt: String,
        cwd: &Path,
        emit: impl FnMut(AgentEvent),
    ) -> Result<(), String> {
        let callbacks =
            self.registry.borrow().model.clone().ok_or_else(|| {
                "No model configured; a Lua model adapter is required.".to_owned()
            })?;
        let mut model = LuaModel {
            lua: &self.lua,
            callbacks,
            client: self.client.clone(),
        };
        let executor = LuaTools {
            lua: &self.lua,
            registry: Rc::clone(&self.registry),
        };
        let mut conversation = Conversation::default();
        run_turn(
            &mut conversation,
            prompt,
            &mut model,
            &executor,
            cwd,
            20,
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
