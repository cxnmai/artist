//! Lua registration for compaction policy. Summarization is an async Rust builtin.

use crate::registry::Registry;
use artist_core::compaction::CompactionSettings;
use mlua::{Function, Lua, LuaSerdeExt, Table};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct RegisteredCompaction {
    pub settings: CompactionSettings,
    pub compact: Function,
}

pub fn install(lua: &Lua, registry: Rc<RefCell<Registry>>) -> mlua::Result<()> {
    let artist: Table = lua.globals().get("artist")?;
    artist.set(
        "set_compaction",
        lua.create_function(move |lua, table: Table| {
            let settings_table = lua.create_table()?;
            for key in [
                "enabled",
                "reserve_tokens",
                "keep_recent_tokens",
                "max_summary_tokens",
                "instructions",
            ] {
                settings_table.set(key, table.get::<mlua::Value>(key)?)?;
            }
            let settings: CompactionSettings =
                lua.from_value(mlua::Value::Table(settings_table))?;
            settings.validate().map_err(mlua::Error::external)?;
            let compact = match table.get::<Option<Function>>("compact")? {
                Some(callback) => callback,
                None => lua
                    .globals()
                    .get::<Table>("artist")?
                    .get::<Table>("compaction")?
                    .get("summarize")?,
            };
            registry.borrow_mut().compaction = Some(RegisteredCompaction { settings, compact });
            Ok(())
        })?,
    )?;
    Ok(())
}

pub async fn maybe_compact(
    model: &crate::model::LuaModel<'_>,
    context: &artist_core::context::Conversation,
    window: Option<u64>,
) -> Result<Option<artist_core::compaction::CompactionResult>, String> {
    let Some(policy) = &model.compaction else {
        return Ok(None);
    };
    if !policy.settings.enabled {
        return Ok(None);
    }
    let Some(window) = window.filter(|window| *window > 0) else {
        return Ok(None);
    };
    policy.settings.validate_window(window)?;
    let usage = context
        .context_usage(window)
        .expect("nonzero context window");
    if usage.tokens <= window - policy.settings.reserve_tokens {
        return Ok(None);
    }
    let options =
        crate::compaction_options::SummaryOptions::for_model(model, policy.settings.clone());
    let context = model.lua.to_value(context).map_err(|e| e.to_string())?;
    let options = model.lua.to_value(&options).map_err(|e| e.to_string())?;
    let result: mlua::Value = policy
        .compact
        .call_async((context, options))
        .await
        .map_err(|e| format!("compaction: {e}"))?;
    if matches!(result, mlua::Value::Nil) {
        return Ok(None);
    }
    model
        .lua
        .from_value(result)
        .map(Some)
        .map_err(|e| format!("compaction result: {e}"))
}
