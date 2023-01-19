mod structs;

use self::structs::PackageItem;
use ewe_commons::{Package, Source};
use mlua::Error::{CallbackError, MemoryError, RuntimeError, SyntaxError};
use mlua::{ExternalError, Lua};
use std::collections::HashSet;
use std::path::Path;
use structs::SourceItem;

#[derive(Debug)]
pub struct BuildScript {
  lua: Lua,
  source: SourceItem,
  packages: HashSet<PackageItem>,
}

impl BuildScript {
  pub fn new(path: &Path) -> mlua::Result<Self> {
    let lua = Lua::new();
    lua.set_app_data(HashSet::<PackageItem>::new());
    // TODO: add many things
    let globals = lua.globals();
    globals.raw_set(
      "define_source",
      lua.create_function(|lua, source: SourceItem| {
        if lua.app_data_ref::<SourceItem>().is_none() {
          lua.set_app_data(source);
          Ok(())
        } else {
          Err("can only define source once".to_lua_err())
        }
      })?,
    )?;
    globals.raw_set(
      "define_package",
      lua.create_function(|lua, package: PackageItem| {
        let mut packages = lua.app_data_mut::<HashSet<PackageItem>>().unwrap();
        if packages.contains(&package) {
          Err(format!("duplicate package '{}'", &package.info.name).to_lua_err())
        } else {
          packages.insert(package);
          Ok(())
        }
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
    let packages: HashSet<_> = lua.remove_app_data().unwrap();
    if packages.is_empty() {
      return Err("no package specified".to_lua_err());
    }

    Ok(Self {
      lua,
      source,
      packages,
    })
  }

  pub fn source(&self) -> &Source {
    &self.source.info
  }

  pub fn packages(&self) -> impl Iterator<Item = &Package> {
    self.packages.iter().map(|x| &x.info)
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
