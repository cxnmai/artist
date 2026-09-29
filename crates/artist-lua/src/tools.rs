use crate::registry::Registry;
use artist_core::tools::{ToolExecutor, ToolOutput};
use artist_tools::executor::BuiltinTools;
use mlua::{Lua, LuaSerdeExt, Value as LuaValue};
use serde::Deserialize;
use serde_json::Value;
use std::cell::RefCell;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::rc::Rc;

#[derive(Deserialize)]
struct LuaOutput {
    text: String,
    #[serde(default)]
    is_error: bool,
}

pub struct LuaTools<'a> {
    pub lua: &'a Lua,
    pub registry: Rc<RefCell<Registry>>,
}

impl ToolExecutor for LuaTools<'_> {
    fn execute<'a>(
        &'a self,
        name: &'a str,
        arguments: &'a Value,
        cwd: &'a Path,
    ) -> Pin<Box<dyn Future<Output = ToolOutput> + 'a>> {
        Box::pin(async move {
            let callback = self
                .registry
                .borrow()
                .tools
                .iter()
                .find(|tool| tool.definition.name == name)
                .map(|tool| tool.execute.clone());
            let Some(callback) = callback else {
                return ToolOutput::error(format!("unknown tool: {name}"));
            };
            let result = async {
                let args = self.lua.to_value(arguments)?;
                let ctx = self.lua.create_table()?;
                ctx.set("cwd", cwd.to_string_lossy().as_ref())?;
                let value: LuaValue = callback.call_async((args, ctx)).await?;
                if let mlua::Value::String(text) = value {
                    return Ok(ToolOutput::success(text.to_str()?.to_owned()));
                }
                let output: LuaOutput = self.lua.from_value(value)?;
                Ok(ToolOutput {
                    text: output.text,
                    is_error: output.is_error,
                })
            }
            .await;
            result.unwrap_or_else(|error: mlua::Error| ToolOutput::error(error.to_string()))
        })
    }
}

pub fn install_native(lua: &Lua) -> mlua::Result<()> {
    let artist: mlua::Table = lua.globals().get("artist")?;
    let native = lua.create_table()?;
    native.set(
        "execute",
        lua.create_async_function(
            |lua, (name, args, cwd): (String, LuaValue, String)| async move {
                let args: Value = lua.from_value(args)?;
                let output = BuiltinTools.execute(&name, &args, Path::new(&cwd)).await;
                lua.to_value(&serde_json::json!({"text": output.text, "is_error": output.is_error}))
            },
        )?,
    )?;
    artist.set("native", native)?;
    let json = lua.create_table()?;
    json.set("null", LuaValue::NULL)?;
    json.set(
        "decode",
        lua.create_function(|lua, text: String| {
            let value: Value = serde_json::from_str(&text).map_err(mlua::Error::external)?;
            lua.to_value(&value)
        })?,
    )?;
    json.set(
        "encode",
        lua.create_function(|lua, value: LuaValue| {
            let value: Value = lua.from_value(value)?;
            serde_json::to_string(&value).map_err(mlua::Error::external)
        })?,
    )?;
    artist.set("json", json)?;
    Ok(())
}
