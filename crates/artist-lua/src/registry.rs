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
pub struct RegisteredModel {
    pub name: String,
    pub kind: ModelKind,
}

#[derive(Default)]
pub struct Registry {
    pub tools: Vec<RegisteredTool>,
    pub model: Option<RegisteredModel>,
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
    let models = Rc::clone(&registry);
    artist.set(
        "model",
        lua.create_function(move |lua, table: Table| {
            let name: String = table.get("name")?;
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
            models.borrow_mut().model = Some(RegisteredModel { name, kind });
            Ok(())
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
