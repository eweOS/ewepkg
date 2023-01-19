use ewe_commons::{Package, Source};
use mlua::{FromLua, Lua, RegistryKey, Table};
use std::hash::{Hash, Hasher};

#[derive(Debug)]
pub struct SourceItem {
  pub info: Source,
  pub table_key: RegistryKey,
}

impl<'lua> FromLua<'lua> for SourceItem {
  fn from_lua(value: mlua::Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
    let table: Table = lua.unpack(value)?;
    let info = Source::from_table(&table)?;
    let table_key = lua.create_registry_value(table)?;
    Ok(Self { info, table_key })
  }
}

#[derive(Debug)]
pub struct PackageItem {
  pub info: Package,
  pub table_key: RegistryKey,
}

impl<'lua> FromLua<'lua> for PackageItem {
  fn from_lua(value: mlua::Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
    let table: Table = lua.unpack(value)?;
    let info = Package::from_table(&table)?;
    let table_key = lua.create_registry_value(table)?;
    Ok(Self { info, table_key })
  }
}

impl Hash for PackageItem {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.info.name.hash(state);
  }
}

impl PartialEq for PackageItem {
  fn eq(&self, other: &Self) -> bool {
    self.info.name == other.info.name
  }
}

impl Eq for PackageItem {}
