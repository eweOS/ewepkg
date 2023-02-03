use crate::version::PackageVersion;
use openssl::error::ErrorStack;
use openssl::hash::{Hasher, MessageDigest};
use serde::de::Error;
use serde::{de, Deserialize, Deserializer, Serialize};
use smartstring::{LazyCompact, SmartString};
use std::borrow::Borrow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Debug, Display, Formatter};
use std::ops::Deref;
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;
use url::Url;

// TODO: more strict
pub fn assure_pkg_name<S: AsRef<str>>(s: S) -> Result<S, ParseNameError> {
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
pub struct PackageName(SmartString<LazyCompact>);

impl FromStr for PackageName {
  type Err = ParseNameError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    assure_pkg_name(s)?;
    Ok(Self(s.into()))
  }
}

impl Deref for PackageName {
  type Target = str;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl AsRef<str> for PackageName {
  fn as_ref(&self) -> &str {
    self
  }
}

impl Borrow<str> for PackageName {
  fn borrow(&self) -> &str {
    self
  }
}

impl Debug for PackageName {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    <str as Debug>::fmt(self, f)
  }
}

impl Display for PackageName {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    f.write_str(self)
  }
}

impl<'de> Deserialize<'de> for PackageName {
  fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
    assure_pkg_name(String::deserialize(de)?)
      .map(|x| Self(x.into()))
      .map_err(de::Error::custom)
  }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("package name contains invalid character `{0}`")]
pub struct ParseNameError(char);

#[derive(Debug, Clone, Serialize)]
pub struct ArchList(BTreeSet<SmartString<LazyCompact>>);

impl ArchList {
  pub fn contains(&self, arch: &str) -> bool {
    (self.0)
      .iter()
      .any(|x| &**x == "any" || &**x == "all" || &**x == arch)
  }

  pub fn contains_all(&self) -> bool {
    self.0.contains("all")
  }

  pub fn is_valid_for_package(&self) -> bool {
    if self.contains_all() {
      self.0.len() == 1
    } else {
      true
    }
  }
}

impl Deref for ArchList {
  type Target = BTreeSet<SmartString<LazyCompact>>;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl<'de> Deserialize<'de> for ArchList {
  fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
    let mut set = BTreeSet::<SmartString<_>>::deserialize(de)?;
    if set.is_empty() {
      return Err(serde::de::Error::invalid_length(
        0,
        &"one or more architecture",
      ));
    }
    if set.contains("any") {
      set.retain(|x| &**x == "any" || &**x == "all");
    }
    Ok(Self(set))
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionalDepends {
  pub name: PackageName,

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
pub enum SourceLocation {
  #[serde(rename = "url")]
  Http(Url),

  #[serde(rename = "path")]
  Local(Box<Path>),
}

impl SourceLocation {
  pub fn file_name(&self) -> Option<&str> {
    match self {
      Self::Http(url) => url.path_segments()?.last(),
      Self::Local(path) => path.file_name()?.to_str(),
    }
  }
}

impl Display for SourceLocation {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    match self {
      SourceLocation::Http(url) => write!(f, "{url}"),
      SourceLocation::Local(path) => write!(f, "{}", path.display()),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ChecksumKind {
  #[serde(rename = "sha256sum")]
  Sha256,
  #[serde(rename = "sha512sum")]
  Sha512,
}

impl ChecksumKind {
  pub fn new_hasher(&self) -> Result<Hasher, ErrorStack> {
    match self {
      Self::Sha256 => Hasher::new(MessageDigest::sha256()),
      Self::Sha512 => Hasher::new(MessageDigest::sha512()),
    }
  }

  pub fn name(&self) -> &'static str {
    match self {
      Self::Sha256 => "SHA-256",
      Self::Sha512 => "SHA-512",
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hash(#[serde(with = "hex::serde")] Vec<u8>);

impl AsRef<[u8]> for Hash {
  fn as_ref(&self) -> &[u8] {
    self
  }
}

impl Deref for Hash {
  type Target = [u8];

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

fn get_true() -> bool {
  true
}

#[derive(Debug, Clone, Deserialize)]
struct SourceFileHelper {
  #[serde(flatten)]
  pub location: SourceLocation,

  #[serde(default)]
  pub rename: Option<Box<str>>,

  #[serde(flatten)]
  pub checksums: BTreeMap<ChecksumKind, Hash>,

  #[serde(default = "get_true")]
  pub extract: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceFile {
  #[serde(flatten)]
  pub location: SourceLocation,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub rename: Option<Box<str>>,

  #[serde(flatten)]
  pub checksums: BTreeMap<ChecksumKind, Hash>,

  #[serde(skip_serializing_if = "bool::clone")]
  pub extract: bool,
}

impl SourceFile {
  pub fn file_name(&self) -> &str {
    self.rename.as_deref().unwrap_or_else(|| {
      self
        .location
        .file_name()
        .expect("location should include a file name")
    })
  }
}

impl<'de> Deserialize<'de> for SourceFile {
  fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
    let SourceFileHelper {
      location,
      rename,
      checksums,
      extract,
    } = SourceFileHelper::deserialize(de)?;
    if rename.is_none() && location.file_name().is_none() {
      return Err(D::Error::custom("no file name given"));
    }
    Ok(Self {
      location,
      rename,
      checksums,
      extract,
    })
  }
}

// TODO: license, backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
  pub name: PackageName,
  pub description: Box<str>,
  pub version: PackageVersion,
  pub architecture: ArchList,

  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub homepage: Option<Url>,

  #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
  pub provides: BTreeSet<PackageName>,

  #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
  pub conflicts: BTreeSet<PackageName>,

  #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
  pub depends: BTreeSet<PackageName>,

  #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
  pub optional_depends: BTreeSet<OptionalDepends>,
}

impl PartialEq for PackageInfo {
  fn eq(&self, other: &Self) -> bool {
    self.name == other.name
  }
}

impl Eq for PackageInfo {}

impl PartialOrd for PackageInfo {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for PackageInfo {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    self.name.cmp(&other.name)
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
  #[serde(flatten)]
  pub inner: PackageInfo,

  #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
  pub build_depends: BTreeSet<PackageName>,

  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub source: Vec<SourceFile>,
}

impl Deref for SourceInfo {
  type Target = PackageInfo;

  fn deref(&self) -> &Self::Target {
    &self.inner
  }
}
