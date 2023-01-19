mod types;
pub use types::*;

#[cfg(feature = "mlua")]
use mlua::{ExternalError, FromLua, Table};

#[cfg(feature = "mlua")]
pub trait LuaTableExt<'lua> {
  fn get_better_error<V: FromLua<'lua>>(&'lua self, k: &str) -> mlua::Result<V>;
}

#[cfg(feature = "mlua")]
impl<'lua> LuaTableExt<'lua> for Table<'lua> {
  fn get_better_error<V: FromLua<'lua>>(&'lua self, k: &str) -> mlua::Result<V> {
    self
      .get(k)
      .map_err(|e| format!("failed to get field {k}: {e}").to_lua_err())
  }
}
