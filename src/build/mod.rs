mod fetch;
mod script;

use crate::segment_info;
use anyhow::bail;
use script::{BuildScript, PackScript};
use std::path::PathBuf;
use std::process::Command;
use std::str::from_utf8;

pub fn run(path: PathBuf) -> anyhow::Result<()> {
  let script = BuildScript::new(path)?;
  let source = &script.source().meta;
  let arch = Command::new("uname").arg("-m").output()?.stdout;
  let arch = from_utf8(&arch)?.trim();
  if !source.architecture.contains(arch) {
    bail!("source architecture does not contain `{arch}`")
  }
  segment_info!("Starting building:", "{} {}", source.name, source.version);
  script.prepare()?;
  script.build()?;
  script.pack()?;
  Ok(())
}

pub fn run_package(path: PathBuf, source_dir: PathBuf) -> anyhow::Result<()> {
  // SAFETY: only gets current user's UID
  if unsafe { libc::getuid() } != 0 {
    bail!("not running in fakeroot/root environment");
  }
  let script = PackScript::new(path, source_dir)?;
  script.pack()?;
  Ok(())
}
