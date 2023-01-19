use ewe_commons::lua_helpers::{LuaTableExt, LuaUrl};
use ewe_commons::types::{OptionalDepends, Package, PkgName, Source};
use mlua::{FromLua, Lua, RegistryKey, Table};
use reqwest::Url;
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

pub struct PackageDelta {
  pub name: Option<PkgName>,
  pub description: Option<Box<str>>,
  pub homepage: Option<Url>,
  pub depends: Option<Vec<PkgName>>,
  pub optional_depends: Option<Vec<OptionalDepends>>,
  pub table_key: RegistryKey,
}

impl PackageDelta {
  pub fn into_package_item(self, source: &Source) -> PackageItem {
    let info = Package {
      name: self.name.unwrap_or_else(|| source.name.clone()),
      description: self
        .description
        .unwrap_or_else(|| source.description.clone()),
      version: source.version.clone(),
      homepage: self.homepage.or_else(|| source.homepage.clone()),
      depends: self
        .depends
        .map(|mut depends| {
          depends.extend(source.depends.iter().cloned());
          depends
        })
        .unwrap_or_else(|| source.depends.clone()),
      optional_depends: self
        .optional_depends
        .map(|mut depends| {
          depends.extend(source.optional_depends.iter().cloned());
          depends
        })
        .unwrap_or_else(|| source.optional_depends.clone()),
    };
    PackageItem {
      info,
      table_key: self.table_key,
    }
  }
}

impl<'lua> FromLua<'lua> for PackageDelta {
  fn from_lua(lua_value: mlua::Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
    let table: Table = lua.unpack(lua_value)?;
    Ok(Self {
      name: table.get_better_error("name")?,
      description: table.get_better_error("description")?,
      homepage: table
        .get_better_error::<Option<LuaUrl>>("homepage")?
        .map(|x| x.0),
      depends: table.get_better_error("depends")?,
      optional_depends: table.get_better_error("optional_depends")?,
      table_key: lua.create_registry_value(table)?,
    })
  }
}
