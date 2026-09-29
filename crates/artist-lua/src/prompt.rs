//! Lua-owned system prompt policy, evaluated once per conversation.

use crate::registry::Registry;
use mlua::{Function, Lua, Result, Table, Value};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

#[derive(Clone)]
pub enum PromptPart {
    Text(String),
    Function(Function),
}

impl PromptPart {
    fn from_value(value: Value) -> Result<Self> {
        match value {
            Value::String(text) => Ok(Self::Text(text.to_str()?.to_owned())),
            Value::Function(function) => Ok(Self::Function(function)),
            _ => Err(mlua::Error::external(
                "system prompt must be a string or function",
            )),
        }
    }

    fn evaluate(&self, context: &Table) -> Result<String> {
        match self {
            Self::Text(text) => Ok(text.clone()),
            Self::Function(function) => function.call(context.clone()),
        }
    }
}

pub fn install(lua: &Lua, registry: Rc<RefCell<Registry>>) -> Result<()> {
    let artist: Table = lua.globals().get("artist")?;
    let base = Rc::clone(&registry);
    artist.set(
        "set_system_prompt",
        lua.create_function(move |_, value: Value| {
            base.borrow_mut().system_prompt = Some(PromptPart::from_value(value)?);
            Ok(())
        })?,
    )?;
    artist.set(
        "append_system_prompt",
        lua.create_function(move |_, value: Value| {
            registry
                .borrow_mut()
                .prompt_appends
                .push(PromptPart::from_value(value)?);
            Ok(())
        })?,
    )?;
    Ok(())
}

pub fn build(
    lua: &Lua,
    registry: &Rc<RefCell<Registry>>,
    cwd: &Path,
    model_name: &str,
) -> Result<String> {
    let registry = registry.borrow();
    let base = registry.system_prompt.clone();
    let appends = registry.prompt_appends.clone();
    drop(registry);
    let context = lua.create_table()?;
    context.set("model_name", model_name)?;
    context.set("cwd", cwd.to_string_lossy().as_ref())?;
    let mut prompt = match base {
        Some(part) => part.evaluate(&context)?,
        None => String::new(),
    };
    for part in appends {
        let text = part.evaluate(&context)?;
        if !text.is_empty() {
            if !prompt.is_empty() {
                prompt.push_str("\n\n");
            }
            prompt.push_str(&text);
        }
    }
    Ok(prompt)
}
