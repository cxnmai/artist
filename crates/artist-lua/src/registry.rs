use crate::prompt::PromptPart;
use artist_core::tools::ToolDefinition;
use mlua::{Function, Lua, LuaSerdeExt, Result, Table};
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;

pub struct RegisteredTool {
    pub definition: ToolDefinition,
    pub execute: Function,
}

#[derive(Clone)]
pub enum ModelKind {
    Lua {
        request: Function,
        response: Function,
    },
    ChatCompletions {
        endpoint: String,
        bearer_env: Option<String>,
        headers: Option<Table>,
        options: serde_json::Map<String, Value>,
    },
}

#[derive(Clone)]
pub struct Discovery {
    pub endpoint: String,
    pub filter: Option<Function>,
}

#[derive(Clone)]
pub struct RegisteredProvider {
    pub name: String,
    pub models: Vec<String>,
    pub default_model: Option<String>,
    pub discover: Option<Discovery>,
    pub kind: ModelKind,
}

#[derive(Default)]
pub struct Registry {
    pub tools: Vec<RegisteredTool>,
    pub provider: Option<RegisteredProvider>,
    pub selector: Option<Function>,
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
            let default_model: Option<String> = table.get("default_model")?;
            let discover: Option<Table> = table.get("discover")?;
            let discover = discover
                .map(|config| {
                    Ok::<_, mlua::Error>(Discovery {
                        endpoint: config.get("endpoint")?,
                        filter: config.get("filter")?,
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
            let kind = match table.get::<Option<String>>("adapter")? {
                Some(adapter) if adapter == "chat_completions" => {
                    if table.contains_key("request")? || table.contains_key("response")? {
                        return Err(mlua::Error::external(
                            "use either adapter or request/response callbacks",
                        ));
                    }
                    let auth: Option<Table> = table.get("auth")?;
                    let bearer_env = auth
                        .as_ref()
                        .map(|auth| auth.get::<String>("bearer_env"))
                        .transpose()?;
                    let options = match table.get::<Option<mlua::Value>>("options")? {
                        Some(value) => lua.from_value::<serde_json::Map<String, Value>>(value)?,
                        None => serde_json::Map::new(),
                    };
                    ModelKind::ChatCompletions {
                        endpoint: table
                            .get::<Option<String>>("endpoint")?
                            .unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".into()),
                        bearer_env,
                        headers: table.get("headers")?,
                        options,
                    }
                }
                Some(adapter) => {
                    return Err(mlua::Error::external(format!(
                        "unknown model adapter: {adapter}"
                    )));
                }
                None => ModelKind::Lua {
                    request: table.get("request")?,
                    response: table.get("response")?,
                },
            };
            if discover.is_some() && !matches!(kind, ModelKind::ChatCompletions { .. }) {
                return Err(mlua::Error::external(
                    "discovery currently requires chat_completions",
                ));
            }
            let mut registry = providers.borrow_mut();
            if registry.provider.is_some() {
                return Err(mlua::Error::external("only one provider can be configured"));
            }
            registry.provider = Some(RegisteredProvider {
                name,
                models,
                default_model,
                discover,
                kind,
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
