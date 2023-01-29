use crate::version::PkgVersion;
use rhai::serde::from_dynamic;
use rhai::EvalAltResult::{self, ErrorMismatchDataType, ErrorRuntime};
use rhai::{Dynamic, FnPtr, Map, Position};
use serde::de::Error;
use serde::{de, Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Debug, Display, Formatter};
use std::ops::Deref;
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;
use url::Url;

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

  #[serde(rename = "blake2sum")]
  Blake2,
}

#[derive(Debug, Clone, Deserialize)]
struct SourceFileHelper {
  #[serde(flatten)]
  pub location: SourceLocation,

  #[serde(default)]
  pub rename: Option<Box<str>>,

  #[serde(flatten)]
  pub checksums: BTreeMap<ChecksumKind, Box<str>>,

  #[serde(default)]
  pub skip_checksum: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceFile {
  #[serde(flatten)]
  pub location: SourceLocation,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub rename: Option<Box<str>>,

  #[serde(flatten)]
  pub checksums: BTreeMap<ChecksumKind, Box<str>>,

  #[serde(skip_serializing_if = "std::ops::Not::not")]
  pub skip_checksum: bool,
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
      skip_checksum,
    } = SourceFileHelper::deserialize(de)?;
    if rename.is_none() && location.file_name().is_none() {
      return Err(D::Error::custom("no file name given"));
    }
    if !skip_checksum && checksums.is_empty() {
      return Err(D::Error::custom("no checksum given or `skip_checksum`"));
    }
    Ok(Self {
      location,
      rename,
      checksums,
      skip_checksum,
    })
  }
}

#[derive(Clone)]
pub enum Execution {
  Shell(Box<str>),
  Fn(FnPtr),
}

impl Debug for Execution {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    match self {
      Self::Shell(_) => f.debug_tuple("Shell").field(&"...").finish(),
      Self::Fn(arg0) => f.debug_tuple("Fn").field(arg0).finish(),
    }
  }
}

impl Execution {
  pub fn from_dynamic(value: Dynamic) -> Result<Self, Box<EvalAltResult>> {
    if value.is_string() {
      Ok(Self::Shell(value.into_string().unwrap().into()))
    } else if value.is::<FnPtr>() {
      Ok(Self::Fn(value.cast()))
    } else {
      Err(Box::new(ErrorMismatchDataType(
        "String or Fn".into(),
        value.type_name().into(),
        Position::NONE,
      )))
    }
  }
}

fn fnptr_from_dynamic(x: Dynamic) -> Result<FnPtr, Box<EvalAltResult>> {
  let type_name = x.type_name();
  x.try_cast().ok_or_else(|| {
    Box::new(ErrorMismatchDataType(
      "Fn".into(),
      type_name.into(),
      Position::NONE,
    ))
  })
}

// TODO: architecture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMeta {
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

#[derive(Debug, Deserialize)]
struct PackageDelta {
  name: Option<PkgName>,
  description: Option<Box<str>>,
  version: Option<PkgVersion>,
  homepage: Option<Url>,

  #[serde(default)]
  depends: BTreeSet<PkgName>,

  #[serde(default)]
  optional_depends: BTreeSet<OptionalDepends>,
}

#[derive(Debug, Clone)]
pub struct Package {
  pub meta: PackageMeta,
  pub pack: Option<FnPtr>,
}

impl Package {
  pub fn from_dynamic_and_source_meta(
    value: &mut Dynamic,
    source_meta: &SourceMeta,
  ) -> Result<Self, Box<EvalAltResult>> {
    let type_name = value.type_name();
    let mut map = value.write_lock::<Map>().ok_or_else(|| {
      Box::new(ErrorMismatchDataType(
        "Map".into(),
        type_name.into(),
        Position::NONE,
      ))
    })?;
    let pack = map.remove("pack").map(fnptr_from_dynamic).transpose()?;
    drop(map);
    let mut delta: PackageDelta = from_dynamic(value)?;
    let meta = PackageMeta {
      name: delta.name.unwrap_or_else(|| source_meta.name.clone()),
      description: delta
        .description
        .unwrap_or_else(|| source_meta.description.clone()),
      version: delta.version.unwrap_or_else(|| source_meta.version.clone()),
      homepage: delta.homepage.or_else(|| source_meta.homepage.clone()),
      depends: {
        delta.depends.extend(source_meta.depends.iter().cloned());
        delta.depends
      },
      optional_depends: {
        delta
          .optional_depends
          .extend(source_meta.optional_depends.iter().cloned());
        delta.optional_depends
      },
    };
    Ok(Self { meta, pack })
  }
}

impl PartialEq for Package {
  fn eq(&self, other: &Self) -> bool {
    self.meta.name == other.meta.name
  }
}

impl Eq for Package {}

impl PartialOrd for Package {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for Package {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    self.meta.name.cmp(&other.meta.name)
  }
}
// TODO: architecture, license
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMeta {
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

  // TODO: use set
  #[serde(default)]
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub source: Vec<SourceFile>,
}

#[derive(Debug, Clone)]
pub struct Source {
  pub meta: SourceMeta,
  pub prepare: Option<Execution>,
  pub build: Option<Execution>,
  pub check: Option<Execution>,
  pub packages: BTreeSet<Package>,
}

impl Source {
  pub fn from_dynamic(value: &mut Dynamic) -> Result<Self, Box<EvalAltResult>> {
    let type_name = value.type_name();
    let mut map = value.write_lock::<Map>().ok_or_else(|| {
      Box::new(ErrorMismatchDataType(
        "map".into(),
        type_name.into(),
        Position::NONE,
      ))
    })?;
    let mut execs = [None, None, None];
    for (i, name) in ["prepare", "build", "check"].iter().enumerate() {
      execs[i] = map.remove(*name).map(Execution::from_dynamic).transpose()?;
    }
    let [prepare, build, check] = execs;

    let pack = map.remove("pack").map(fnptr_from_dynamic).transpose()?;
    let packages_repr = map
      .remove("packages")
      .map(|x| {
        x.into_array().map_err(|t| {
          Box::new(ErrorMismatchDataType(
            "Array".into(),
            t.into(),
            Position::NONE,
          ))
        })
      })
      .transpose()?;
    if pack.is_some() && packages_repr.is_some() {
      return Err(Box::new(ErrorRuntime(
        Dynamic::from("field `pack` and `packages` conflicts"),
        Position::NONE,
      )));
    }

    drop(map);
    let meta: SourceMeta = from_dynamic(value)?;
    let mut packages = BTreeSet::new();
    if let Some(packages_repr) = packages_repr {
      for mut package in packages_repr {
        packages.insert(Package::from_dynamic_and_source_meta(&mut package, &meta)?);
      }
    } else {
      packages.insert(Package {
        meta: PackageMeta {
          name: meta.name.clone(),
          description: meta.description.clone(),
          version: meta.version.clone(),
          homepage: meta.homepage.clone(),
          depends: meta.depends.clone(),
          optional_depends: meta.optional_depends.clone(),
        },
        pack,
      });
    }

    Ok(Self {
      meta,
      prepare,
      build,
      check,
      packages,
    })
  }
}
