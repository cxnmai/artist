use crate::model_info::{ModelInfo, ModelMetadata};
use crate::prompt::PromptPart;
use crate::provider_config::parse_kind;
use artist_core::tools::ToolDefinition;
use mlua::{Function, Lua, LuaSerdeExt, Result, Table};
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;

pub struct RegisteredTool {
    pub definition: ToolDefinition,
    pub execute: Function,
}

#[derive(Clone, Copy)]
pub enum Protocol {
    ChatCompletions,
    OpenAIResponses,
    AnthropicMessages,
}

#[derive(Clone)]
pub enum ModelKind {
    Lua {
        request: Function,
        response: Function,
    },
    Http {
        protocol: Protocol,
        endpoint: String,
        bearer_env: Option<String>,
        api_key_env: Option<String>,
        headers: Option<Table>,
        options: serde_json::Map<String, Value>,
        stream: bool,
    },
}

#[derive(Clone)]
pub struct Discovery {
    pub endpoint: String,
    pub filter: Option<Function>,
    pub parse: Option<Function>,
}

#[derive(Clone)]
pub struct RegisteredProvider {
    pub name: String,
    pub models: Vec<String>,
    pub model_info: std::collections::BTreeMap<String, ModelMetadata>,
    pub default_model: Option<String>,
    pub discover: Option<Discovery>,
    pub cost: Option<Function>,
    pub kind: ModelKind,
    pub adapters: std::collections::BTreeMap<String, ModelKind>,
}

#[derive(Default)]
pub struct Registry {
    pub tools: Vec<RegisteredTool>,
    pub provider: Option<RegisteredProvider>,
    pub selector: Option<Function>,
    pub compaction: Option<crate::compaction::RegisteredCompaction>,
    pub system_prompt: Option<PromptPart>,
    pub prompt_appends: Vec<PromptPart>,
}

pub fn install(lua: &Lua, registry: Rc<RefCell<Registry>>) -> Result<()> {
    let artist = lua.create_table()?;
    let tools = Rc::clone(&registry);
    artist.set(
        "register_tool",
        lua.create_function(move |lua, table: Table| {
            let name: String = table.get("name")?;
            let definition = ToolDefinition {
                name: name.clone(),
                description: table.get("description")?,
                parameters: lua.from_value(table.get("parameters")?)?,
            };
            let execute = table.get("execute")?;
            let mut registry = tools.borrow_mut();
            if registry
                .tools
                .iter()
                .any(|tool| tool.definition.name == name)
            {
                return Err(mlua::Error::external(format!("duplicate tool: {name}")));
            }
            registry.tools.push(RegisteredTool {
                definition,
                execute,
            });
            Ok(())
        })?,
    )?;
    let removals = Rc::clone(&registry);
    artist.set(
        "unregister_tool",
        lua.create_function(move |_, name: String| {
            let mut registry = removals.borrow_mut();
            let index = registry
                .tools
                .iter()
                .position(|tool| tool.definition.name == name)
                .ok_or_else(|| mlua::Error::external(format!("unknown tool: {name}")))?;
            registry.tools.remove(index);
            Ok(())
        })?,
    )?;
    let providers = Rc::clone(&registry);
    artist.set(
        "provider",
        lua.create_function(move |lua, table: Table| {
            let name: String = table.get("name")?;
            let models: Vec<String> = table
                .get::<Option<Vec<String>>>("models")?
                .unwrap_or_default();
            if models.iter().any(|model| model.is_empty())
                || models.len()
                    != models
                        .iter()
                        .collect::<std::collections::HashSet<_>>()
                        .len()
            {
                return Err(mlua::Error::external(
                    "models must contain unique nonempty IDs",
                ));
            }
            let model_info = table
                .get::<Option<mlua::Value>>("model_info")?
                .map(|value| {
                    lua.from_value::<std::collections::BTreeMap<String, ModelMetadata>>(value)
                })
                .transpose()?
                .unwrap_or_default();
            for (id, info) in &model_info {
                if !models.contains(id) {
                    return Err(mlua::Error::external(format!(
                        "model_info key {id} must be in models"
                    )));
                }
                let mut model = ModelInfo {
                    id: id.clone(),
                    adapter: info.adapter.clone(),
                    context_window: info.context_window,
                    max_output_tokens: info.max_output_tokens,
                    reasoning_levels: info.reasoning_levels.clone(),
                    reasoning_map: info.reasoning_map.clone(),
                    thinking_format: info.thinking_format.clone(),
                    requires_reasoning_content: info.requires_reasoning_content,
                    default_reasoning: info.default_reasoning.clone(),
                };
                model.normalize();
                model.validate().map_err(mlua::Error::external)?;
            }
            let default_model: Option<String> = table.get("default_model")?;
            let cost: Option<Function> = table.get("cost")?;
            let discover: Option<Table> = table.get("discover")?;
            let discover = discover
                .map(|config| {
                    Ok::<_, mlua::Error>(Discovery {
                        endpoint: config.get("endpoint")?,
                        filter: config.get("filter")?,
                        parse: config.get("parse")?,
                    })
                })
                .transpose()?;
            if models.is_empty() && discover.is_none() {
                return Err(mlua::Error::external("provider needs models or discover"));
            }
            if let Some(default) = &default_model {
                if default.is_empty() || (discover.is_none() && !models.contains(default)) {
                    return Err(mlua::Error::external(
                        "default_model must be a configured model ID",
                    ));
                }
            }
            let kind = parse_kind(lua, &table, None)?;
            let mut adapters = std::collections::BTreeMap::new();
            if let Some(configs) = table.get::<Option<Table>>("adapters")? {
                for pair in configs.pairs::<String, Table>() {
                    let (key, config) = pair?;
                    if key.is_empty() {
                        return Err(mlua::Error::external("adapter name cannot be empty"));
                    }
                    adapters.insert(key, parse_kind(lua, &config, Some(&table))?);
                }
            }
            for (id, info) in &model_info {
                if let Some(adapter) = &info.adapter {
                    if !adapters.contains_key(adapter) {
                        return Err(mlua::Error::external(format!(
                            "model {id} refers to unknown adapter {adapter}"
                        )));
                    }
                }
            }
            let mut registry = providers.borrow_mut();
            if registry.provider.is_some() {
                return Err(mlua::Error::external("only one provider can be configured"));
            }
            registry.provider = Some(RegisteredProvider {
                name,
                models,
                model_info,
                default_model,
                discover,
                cost,
                kind,
                adapters,
            });
            Ok(())
        })?,
    )?;
    // Compatibility for existing single-model configurations.
    let provider_registration: Function = artist.get("provider")?;
    artist.set(
        "model",
        lua.create_function(move |_, table: Table| {
            let name: String = table.get("name")?;
            table.set("models", vec![name.clone()])?;
            table.set("default_model", name)?;
            provider_registration.call::<()>(table)
        })?,
    )?;
    artist.set(
        "select_context",
        lua.create_function(move |_, callback: Function| {
            registry.borrow_mut().selector = Some(callback);
            Ok(())
        })?,
    )?;
    lua.globals().set("artist", artist)?;
    Ok(())
}
