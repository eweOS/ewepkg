mod version;

pub use version::*;

use serde::{de, Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Debug, Display, Formatter};
use std::ops::Deref;
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;
use url::Url;
use version::PkgVersion;

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

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct PkgName(Box<str>);

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

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("package name contains invalid character `{0}`")]
pub struct ParseNameError(char);

// TODO: architecture, license
#[derive(Debug, Serialize, Deserialize)]
pub struct Source {
  pub name: PkgName,
  pub description: Box<str>,
  pub version: PkgVersion,

  #[serde(default)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub homepage: Option<Url>,

  // TODO: add version requirement
  #[serde(default)]
  #[serde(skip_serializing_if = "BTreeSet::is_empty")]
  pub build_depends: BTreeSet<PkgName>,

  #[serde(default)]
  #[serde(skip_serializing_if = "BTreeSet::is_empty")]
  pub depends: BTreeSet<PkgName>,

  #[serde(default)]
  #[serde(skip_serializing_if = "BTreeSet::is_empty")]
  pub optional_depends: BTreeSet<OptionalDepends>,

  #[serde(default)]
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub source: Vec<SourceFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionalDepends {
  pub name: PkgName,

  #[serde(default)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub description: Option<Box<str>>,
}

impl PartialEq for OptionalDepends {
  fn eq(&self, other: &Self) -> bool {
    self.name == other.name
  }
}

impl Eq for OptionalDepends {}

impl PartialOrd for OptionalDepends {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for OptionalDepends {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    self.name.cmp(&other.name)
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
  #[serde(flatten)]
  pub location: SourceLocation,
  #[serde(flatten)]
  pub checksums: BTreeMap<ChecksumKind, Box<str>>,
  #[serde(default)]
  #[serde(skip_serializing_if = "std::ops::Not::not")]
  pub skip_checksum: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceLocation {
  #[serde(rename = "url")]
  Http(Url),

  #[serde(rename = "path")]
  Local(Box<Path>),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ChecksumKind {
  #[serde(rename = "sha256sum")]
  Sha256,

  #[serde(rename = "blake2sum")]
  Blake2,
}

// TODO: architecture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
  pub name: PkgName,
  pub description: Box<str>,
  pub version: PkgVersion,

  #[serde(default)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub homepage: Option<Url>,

  #[serde(default)]
  #[serde(skip_serializing_if = "BTreeSet::is_empty")]
  pub depends: BTreeSet<PkgName>,

  #[serde(default)]
  #[serde(skip_serializing_if = "BTreeSet::is_empty")]
  pub optional_depends: BTreeSet<OptionalDepends>,
}
