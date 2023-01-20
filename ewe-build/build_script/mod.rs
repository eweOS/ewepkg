mod structs;

use ewe_commons::lua_helpers::LuaTableExt;
use ewe_commons::types::{Package, Source};
use mlua::Error::{CallbackError, MemoryError, RuntimeError, SyntaxError};
use mlua::{ExternalError, Function, Lua, Scope, Table};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use structs::{PackageDelta, PackageItem, SourceItem};
use tempfile::{tempdir, TempDir};

#[derive(Debug)]
pub struct BuildScript {
  lua: Lua,
  path: Box<Path>,
  source: SourceItem,
  packages: BTreeSet<PackageItem>,
  source_dir: TempDir,
}

impl BuildScript {
  pub fn new(path: impl Into<PathBuf>) -> mlua::Result<Self> {
    let lua = Lua::new();
    lua.set_app_data(BTreeSet::<PackageItem>::new());
    let path = path.into().into_boxed_path();

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
      lua.create_function(|lua, delta: PackageDelta| {
        let source = lua
          .app_data_ref::<SourceItem>()
          .ok_or_else(|| "source not defined; define source before packages".to_lua_err())?;
        let package = delta.into_package_item(&source.info);
        drop(source);
        let mut packages = lua.app_data_mut::<BTreeSet<PackageItem>>().unwrap();
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
      .load(&*path)
      .exec()
      .map_err(|error| prettify_lua_error(&error).to_lua_err())?;

    let source = lua
      .remove_app_data()
      .ok_or_else(|| "no source specified".to_lua_err())?;
    let packages: BTreeSet<_> = lua.remove_app_data().unwrap();
    if packages.is_empty() {
      return Err("no package specified".to_lua_err());
    }

    let source_dir = tempdir()?;
    lua
      .globals()
      .raw_set("source_dir", source_dir.path().display().to_string())?;

    Ok(Self {
      lua,
      path,
      source,
      packages,
      source_dir,
    })
  }

  pub fn source(&self) -> &Source {
    &self.source.info
  }

  pub fn packages(&self) -> impl Iterator<Item = &Package> {
    self.packages.iter().map(|x| &x.info)
  }

  // fn fetch_source(&self) -> anyhow::Result<()> {
  //   for file in self.source.info.source.iter() {
  //     match &file.location {
  //       SourceLocation::Http(_url) => continue,
  //       SourceLocation::Local(path) => ,
  //     }
  //   }
  //   Ok(())
  // }

  fn scope<F, R>(&self, current_dir: impl Into<PathBuf>, f: F) -> mlua::Result<R>
  where
    F: FnOnce(&Scope) -> mlua::Result<R>,
    R: 'static,
  {
    let current_dir = RefCell::new(current_dir.into());
    self.lua.scope(|scope| {
      let run = scope.create_function(|_lua, cmd: String| {
        let status = Command::new("bash")
          .args(["-c", &cmd])
          .current_dir(&*current_dir.borrow())
          .stdin(Stdio::null())
          .stdout(Stdio::inherit())
          .status()?;
        if status.success() {
          Ok(())
        } else if let Some(code) = status.code() {
          Err(format!("command `{cmd}` exited with status {code}").to_lua_err())
        } else {
          Err(format!("command `{cmd}` terminated by signal").to_lua_err())
        }
      })?;
      let cd = scope.create_function_mut(|_lua, path: mlua::String| {
        let path = std::str::from_utf8(path.as_bytes())?;
        let path = Path::new(path);
        current_dir.borrow_mut().push(path);
        Ok(())
      })?;
      self.lua.globals().raw_set("run", run)?;
      self.lua.globals().raw_set("cd", cd)?;
      f(scope)
    })
    // TODO: maybe cleanup?
  }

  pub fn build(&self) -> mlua::Result<()> {
    self.scope(self.source_dir.path(), |_scope| {
      let source_table: Table = self.lua.registry_value(&self.source.table_key)?;
      if let Some(build_fn) = source_table.get_better_error::<Option<Function>>("build")? {
        build_fn.call(())?;
        println!("done");
      }
      Ok(())
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
