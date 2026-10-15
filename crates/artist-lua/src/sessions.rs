//! Session storage policy; storage itself lives outside the Lua runtime.
use crate::registry::Registry;
use mlua::{Lua, Table};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

pub fn install(lua: &Lua, registry: Rc<RefCell<Registry>>) -> mlua::Result<()> {
    let artist: Table = lua.globals().get("artist")?;
    artist.set(
        "sessions",
        lua.create_function(move |_, options: Table| {
            let directory: String = options.get("directory")?;
            let directory = PathBuf::from(directory);
            if !directory.is_absolute() {
                return Err(mlua::Error::external("session directory must be absolute"));
            }
            registry.borrow_mut().sessions_directory = Some(directory);
            Ok(())
        })?,
    )
}
