use mlua::RegistryKey;
use mlua::{ExternalError, FromLua, Lua, Table};
use reqwest::Url;
use std::collections::HashMap;
use std::path::PathBuf;
use ChecksumKind::*;

#[derive(Debug)]
pub struct Source {
  pub meta: SourceMeta,
  pub table_key: RegistryKey,
}

impl<'lua> FromLua<'lua> for Source {
  fn from_lua(value: mlua::Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
    let table = lua.unpack(value)?;
    let meta = SourceMeta::from_table(&table)?;
    let table_key = lua.create_registry_value(table)?;
    Ok(Self { meta, table_key })
  }
}

#[derive(Debug)]
pub struct SourceMeta {
  pub name: String,
  pub description: String,
  pub version: String,
  pub build_depends: Vec<String>,
  pub source: Vec<SourceFile>,
}

impl SourceMeta {
  fn from_table(table: &Table) -> mlua::Result<Self> {
    Ok(Self {
      name: table.get("name")?,
      description: table.get("description")?,
      // TODO: version check
      version: table.get("version")?,
      build_depends: table
        .get::<_, Table>("build_depends")?
        .sequence_values::<String>()
        .collect::<Result<_, _>>()?,
      source: table
        .get::<_, Table>("source")?
        .sequence_values::<SourceFile>()
        .collect::<Result<_, _>>()?,
    })
  }
}

#[derive(Debug)]
pub struct SourceFile {
  pub location: SourceLocation,
  pub checksums: HashMap<ChecksumKind, String>,
}

impl<'lua> FromLua<'lua> for SourceFile {
  fn from_lua(value: mlua::Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
    let table: Table = lua.unpack(value)?;

    let location_str: String = table.get(1)?;
    let location = if let Ok(url) = Url::parse(&location_str) {
      let scheme = url.scheme();
      if ["http", "https"].contains(&scheme) {
        SourceLocation::Http(url)
      } else {
        return Err(format!("unknown scheme '{scheme}'").to_lua_err());
      }
    } else if let Ok(path) = location_str.parse::<PathBuf>() {
      if path.is_relative() {
        SourceLocation::Local(path)
      } else {
        return Err(format!("absolute path ('{location_str}') is not allowed").to_lua_err());
      }
    } else {
      return Err(
        format!("cannot parse source '{location_str}' as either URL or path").to_lua_err(),
      );
    };

    let mut checksums = HashMap::new();

    for sum_kind in [Sha256, Blake2] {
      if let Some(sum) = table.get::<_, Option<String>>(sum_kind.field_name())? {
        checksums.insert(sum_kind, sum);
      }
    }

    Ok(Self {
      location,
      checksums,
    })
  }
}

#[derive(Debug)]
pub enum SourceLocation {
  Http(Url),
  Local(PathBuf),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ChecksumKind {
  Sha256,
  Blake2,
}

impl ChecksumKind {
  fn field_name(&self) -> &'static str {
    match self {
      Self::Sha256 => "sha256sum",
      Self::Blake2 => "blake2sum",
    }
  }
}

#[derive(Debug)]
pub struct Package {
  pub meta: PackageMeta,
  pub table_key: RegistryKey,
}

#[derive(Debug)]
pub struct PackageMeta {
  pub name: String,
  pub description: String,
}
