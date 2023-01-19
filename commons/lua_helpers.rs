use mlua::{ExternalError, ExternalResult, FromLua, Lua, Table};
use std::path::Path;
use url::Url;

pub struct LuaUrl(pub Url);

impl<'lua> FromLua<'lua> for LuaUrl {
  fn from_lua(lua_value: mlua::Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
    let s: mlua::String = lua.unpack(lua_value)?;
    let s = std::str::from_utf8(s.as_bytes())?;
    let url = Url::parse(s).to_lua_err()?;
    Ok(Self(url))
  }
}

pub struct LuaPath(pub Box<Path>);

impl<'lua> FromLua<'lua> for LuaPath {
  fn from_lua(lua_value: mlua::Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
    let s: mlua::String = lua.unpack(lua_value)?;
    let s = std::str::from_utf8(s.as_bytes())?;
    let path = Path::new(s);
    Ok(Self(path.into()))
  }
}

pub trait LuaTableExt<'lua> {
  fn get_better_error<V: FromLua<'lua>>(&'lua self, k: &str) -> mlua::Result<V>;
}

impl<'lua> LuaTableExt<'lua> for Table<'lua> {
  fn get_better_error<V: FromLua<'lua>>(&'lua self, k: &str) -> mlua::Result<V> {
    self
      .get(k)
      .map_err(|e| format!("failed to get field {k}: {e}").to_lua_err())
  }
}
