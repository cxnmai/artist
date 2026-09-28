use artist_core::tools::ToolDefinition;
use mlua::{Function, Lua, LuaSerdeExt, Result, Table};
use std::cell::RefCell;
use std::rc::Rc;

pub struct RegisteredTool {
    pub definition: ToolDefinition,
    pub execute: Function,
}

#[derive(Clone)]
pub struct ModelCallbacks {
    pub request: Function,
    pub response: Function,
}

#[derive(Default)]
pub struct Registry {
    pub tools: Vec<RegisteredTool>,
    pub model: Option<ModelCallbacks>,
    pub selector: Option<Function>,
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
    let models = Rc::clone(&registry);
    artist.set(
        "model",
        lua.create_function(move |_, table: Table| {
            models.borrow_mut().model = Some(ModelCallbacks {
                request: table.get("request")?,
                response: table.get("response")?,
            });
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
