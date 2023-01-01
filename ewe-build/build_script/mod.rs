mod structs;

use self::structs::Package;
use mlua::Error::{CallbackError, MemoryError, RuntimeError, SyntaxError};
use mlua::{ExternalError, Lua, Table};
use std::path::Path;
use structs::Source;

pub struct BuildScript {
  lua: Lua,
  source: Source,
  packages: Vec<Package>,
}

impl BuildScript {
  pub fn new(path: &Path) -> mlua::Result<Self> {
    let lua = Lua::new();
    lua.set_app_data(Vec::<Package>::new());
    // TODO: add many things
    let globals = lua.globals();
    globals.raw_set(
      "define_source",
      lua.create_function(|lua, source: Source| {
        if lua.app_data_ref::<Source>().is_none() {
          lua.set_app_data(source);
          Ok(())
        } else {
          Err("can only define source once".to_lua_err())
        }
      })?,
    )?;
    globals.raw_set(
      "define_package",
      lua.create_function(|_lua, table: Table| {
        let name = table.get::<_, String>("name")?;
        dbg!(name);
        Ok(())
      })?,
    )?;
    drop(globals);
    lua
      .load(path)
      .exec()
      .map_err(|error| prettify_lua_error(&error).to_lua_err())?;

    let source = lua
      .remove_app_data()
      .ok_or_else(|| "no source specified".to_lua_err())?;
    let packages: Vec<_> = lua.remove_app_data().unwrap();
    if packages.is_empty() {
      return Err("no package specified".to_lua_err());
    }

    Ok(Self {
      lua,
      source,
      packages,
    })
  }
}

fn prettify_lua_error(error: &mlua::Error) -> String {
  match error {
    // Remove prefix
    RuntimeError(s) | SyntaxError { message: s, .. } | MemoryError(s) => s.clone(),
    CallbackError { traceback, cause } => {
      let buf = prettify_lua_error(cause);
      if buf.contains("stack traceback:") {
        buf
      } else {
        buf + "\n" + traceback
      }
    }
    _ => error.to_string(),
  }
}
