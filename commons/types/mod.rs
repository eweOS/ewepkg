mod version;

pub use version::*;

use serde::{de, Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fmt::{self, Debug, Display, Formatter};
use std::ops::Deref;
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;
use url::Url;
use version::PkgVersion;

#[cfg(feature = "mlua")]
use crate::LuaTableExt;
#[cfg(feature = "mlua")]
use mlua::{ExternalError, ExternalResult, FromLua, Lua, LuaSerdeExt, Table};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct PkgName(Box<str>);

// TODO: more strict
fn assure_pkg_name<S: AsRef<str>>(s: S) -> Result<S, ParseNameError> {
  match s
    .as_ref()
    .chars()
    .find(|c| !c.is_alphanumeric() && *c != '-')
  {
    None => Ok(s),
    Some(c) => Err(ParseNameError(c)),
  }
}

impl FromStr for PkgName {
  type Err = ParseNameError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    assure_pkg_name(s)?;
    Ok(Self(s.into()))
  }
}

impl Deref for PkgName {
  type Target = str;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl Debug for PkgName {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    <str as Debug>::fmt(self, f)
  }
}

impl Display for PkgName {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    f.write_str(self)
  }
}

impl<'de> Deserialize<'de> for PkgName {
  fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
    assure_pkg_name(String::deserialize(de)?)
      .map(|x| Self(x.into()))
      .map_err(de::Error::custom)
  }
}

#[cfg(feature = "mlua")]
impl<'lua> FromLua<'lua> for PkgName {
  fn from_lua(lua_value: mlua::Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
    Ok(Self(assure_pkg_name(lua.unpack(lua_value)?).to_lua_err()?))
  }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("package name contains invalid character `{0}`")]
pub struct ParseNameError(char);

#[derive(Debug, Serialize, Deserialize)]
pub struct Source {
  pub name: PkgName,
  pub description: Box<str>,
  pub version: PkgVersion,
  // TODO: add version requirement
  #[serde(default)]
  pub build_depends: Vec<PkgName>,
  pub depends: Vec<PkgName>,
  #[serde(default)]
  pub optional_depends: Vec<OptionalDepends>,
  pub source: Vec<SourceFile>,
}

#[cfg(feature = "mlua")]
impl Source {
  pub fn from_table(table: &Table) -> mlua::Result<Self> {
    Ok(Self {
      name: table.get_better_error("name")?,
      description: table.get_better_error("description")?,
      version: table.get_better_error("version")?,
      build_depends: table
        .get_better_error::<Option<_>>("build_depends")?
        .unwrap_or_default(),
      depends: table.get_better_error("depends")?,
      optional_depends: table
        .get_better_error::<Option<_>>("optional_depends")?
        .unwrap_or_default(),
      source: table.get_better_error("source")?,
    })
  }
}

#[cfg(feature = "mlua")]
impl<'lua> FromLua<'lua> for Source {
  fn from_lua(lua_value: mlua::Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
    Self::from_table(&lua.unpack(lua_value)?)
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionalDepends {
  pub name: PkgName,
  #[serde(default)]
  pub description: Option<Box<str>>,
}

#[cfg(feature = "mlua")]
impl<'lua> FromLua<'lua> for OptionalDepends {
  fn from_lua(lua_value: mlua::Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
    lua.from_value(lua_value)
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
  #[serde(flatten)]
  pub location: SourceLocation,
  #[serde(flatten)]
  pub checksums: HashMap<ChecksumKind, Box<str>>,
  pub skip_checksum: bool,
}

#[cfg(feature = "mlua")]
struct LuaUrl(Url);

#[cfg(feature = "mlua")]
impl<'lua> FromLua<'lua> for LuaUrl {
  fn from_lua(lua_value: mlua::Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
    let s: mlua::String = lua.unpack(lua_value)?;
    let s = std::str::from_utf8(s.as_bytes())?;
    let url = Url::parse(s).to_lua_err()?;
    Ok(Self(url))
  }
}

#[cfg(feature = "mlua")]
struct LuaPath(Box<Path>);

#[cfg(feature = "mlua")]
impl<'lua> FromLua<'lua> for LuaPath {
  fn from_lua(lua_value: mlua::Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
    let s: mlua::String = lua.unpack(lua_value)?;
    let s = std::str::from_utf8(s.as_bytes())?;
    let path = Path::new(s);
    Ok(Self(path.into()))
  }
}

#[cfg(feature = "mlua")]
impl SourceFile {
  pub fn from_table(table: &Table) -> mlua::Result<Self> {
    let http_src: Option<LuaUrl> = table.get_better_error("url")?;
    let local_src: Option<LuaPath> = table.get_better_error("path")?;
    let location = match (http_src, local_src) {
      (Some(LuaUrl(url)), None) => SourceLocation::Http(url),
      (None, Some(LuaPath(path))) => SourceLocation::Local(path),
      (Some(_), Some(_)) => return Err("can't decide whether to use URL or path".to_lua_err()),
      (None, None) => return Err("no source location defined".to_lua_err()),
    };
    let mut checksums = HashMap::new();
    for (kind, key) in [
      (ChecksumKind::Sha256, "sha256sum"),
      (ChecksumKind::Blake2, "blake2sum"),
    ] {
      if let Some(s) = table.get_better_error(key)? {
        checksums.insert(kind, s);
      }
    }
    let skip_checksum = table.get_better_error("skip_checksum")?;
    Ok(Self {
      location,
      checksums,
      skip_checksum,
    })
  }
}

#[cfg(feature = "mlua")]
impl<'lua> FromLua<'lua> for SourceFile {
  fn from_lua(lua_value: mlua::Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
    Self::from_table(&lua.unpack(lua_value)?)
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceLocation {
  #[serde(rename = "url")]
  Http(Url),
  #[serde(rename = "path")]
  Local(Box<Path>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChecksumKind {
  #[serde(rename = "sha256sum")]
  Sha256,
  #[serde(rename = "blake2sum")]
  Blake2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
  pub name: PkgName,
  pub description: Box<str>,
}

#[cfg(feature = "mlua")]
impl Package {
  pub fn from_table(table: &Table) -> mlua::Result<Self> {
    Ok(Self {
      name: table.get_better_error("name")?,
      description: table.get_better_error("description")?,
    })
  }
}

#[cfg(feature = "mlua")]
impl<'lua> FromLua<'lua> for Package {
  fn from_lua(lua_value: mlua::Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
    let table: Table = lua.unpack(lua_value)?;
    Self::from_table(&table)
  }
}
