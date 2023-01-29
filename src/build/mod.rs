mod engine;
mod fetch;
mod script;

use crate::segment_info;
use anyhow::bail;
use script::{BuildScript, PackScript};
use std::path::PathBuf;

pub fn run(path: PathBuf) -> anyhow::Result<()> {
  let script = BuildScript::new(path)?;
  let source = &script.source().meta;
  segment_info!("Starting building:", "{} {}", source.name, source.version);
  script.prepare()?;
  script.build()?;
  script.pack()?;
  Ok(())
}

pub fn run_package(path: PathBuf, source_dir: PathBuf, arch: String) -> anyhow::Result<()> {
  // SAFETY: only gets current user's UID
  if unsafe { libc::getuid() } != 0 {
    bail!("not running in fakeroot/root environment");
  }
  let script = PackScript::new(path, &source_dir, arch)?;
  script.pack()?;
  Ok(())
}
