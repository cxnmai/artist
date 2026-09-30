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
            let settings: CompactionSettings = lua.from_value(mlua::Value::Table(table.clone()))?;
            settings.validate().map_err(mlua::Error::external)?;
            registry.borrow_mut().compaction = Some(RegisteredCompaction {
                settings,
                compact: table.get("compact")?,
            });
            Ok(())
        })?,
    )?;
    Ok(())
}
