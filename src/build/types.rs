use crate::types::{ArchList, OptionalDepends, PackageInfo, PackageName, SourceInfo};
use crate::version::PackageVersion;
use anyhow::bail;
use reqwest::Url;
use rhai::serde::from_dynamic;
use rhai::EvalAltResult::ErrorMismatchDataType;
use rhai::{Dynamic, EvalAltResult, FnPtr, Map, Position};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::ops::Deref;

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

#[derive(Debug, Deserialize)]
struct PackageInfoDelta {
  name: Option<PackageName>,
  description: Option<Box<str>>,
  version: Option<PackageVersion>,
  architecture: Option<ArchList>,
  homepage: Option<Url>,

  #[serde(default)]
  provides: Option<BTreeSet<PackageName>>,

  #[serde(default)]
  conflicts: Option<BTreeSet<PackageName>>,

  #[serde(default)]
  depends: Option<BTreeSet<PackageName>>,

  #[serde(default)]
  optional_depends: Option<BTreeSet<OptionalDepends>>,
}

impl PackageInfoDelta {
  fn merge_into(self, info: &PackageInfo) -> PackageInfo {
    PackageInfo {
      name: self.name.unwrap_or_else(|| info.name.clone()),
      description: self.description.unwrap_or_else(|| info.description.clone()),
      version: self.version.unwrap_or_else(|| info.version.clone()),
      architecture: self
        .architecture
        .unwrap_or_else(|| info.architecture.clone()),
      homepage: self.homepage.or_else(|| info.homepage.clone()),
      provides: self.provides.unwrap_or_else(|| info.provides.clone()),
      conflicts: self.conflicts.unwrap_or_else(|| info.conflicts.clone()),
      depends: self.depends.unwrap_or_else(|| info.depends.clone()),
      optional_depends: self
        .optional_depends
        .unwrap_or_else(|| info.optional_depends.clone()),
    }
  }
}

#[derive(Debug, Clone)]
pub struct Package {
  pub info: PackageInfo,
  pub pack: Option<FnPtr>,
}

impl Package {
  pub fn from_dynamic_delta(
    value: &mut Dynamic,
    fallback: &PackageInfo,
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
    let delta: PackageInfoDelta = from_dynamic(value)?;
    let info = delta.merge_into(fallback);
    Ok(Self { info, pack })
  }
}

impl Deref for Package {
  type Target = PackageInfo;

  fn deref(&self) -> &Self::Target {
    &self.info
  }
}

impl PartialEq for Package {
  fn eq(&self, other: &Self) -> bool {
    self.info.name == other.info.name
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
    self.info.name.cmp(&other.info.name)
  }
}

#[derive(Debug, Clone)]
pub struct Source {
  pub info: SourceInfo,
  pub prepare: Option<Execution>,
  pub build: Option<Execution>,
  pub check: Option<Execution>,
  pub packages: BTreeSet<Package>,
}

impl Source {
  pub fn from_dynamic(value: &mut Dynamic) -> anyhow::Result<Self> {
    let type_name = value.type_name();
    let mut map = value.write_lock::<Map>().ok_or_else(|| {
      Box::new(ErrorMismatchDataType(
        "Map".into(),
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
      bail!("field `pack` and `packages` conflicts");
    }

    drop(map);
    let info: SourceInfo = from_dynamic(value)?;
    let mut packages = BTreeSet::new();
    if let Some(packages_repr) = packages_repr {
      for mut package in packages_repr {
        packages.insert(Package::from_dynamic_delta(&mut package, &info)?);
      }
    } else {
      if !info.architecture.is_valid_for_package() {
        bail!("architecture for package conflicts between `all` and other platforms");
      }
      packages.insert(Package {
        info: info.inner.clone(),
        pack,
      });
    }

    Ok(Self {
      info,
      prepare,
      build,
      check,
      packages,
    })
  }
}

impl Deref for Source {
  type Target = SourceInfo;

  fn deref(&self) -> &Self::Target {
    &self.info
  }
}
