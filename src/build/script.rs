use super::engine::create_engine;
use super::types::{Execution, Package, Source};
use crate::build::fetch::fetch_source;
use crate::build::PackageMeta;
use crate::segment_info;
use crate::util::PB_STYLE;
use anyhow::bail;
use indicatif::{ProgressBar, ProgressStyle};
use rhai::{Dynamic, Engine, FnPtr, FuncArgs, AST};
use smartstring::{LazyCompact, SmartString};
use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::from_utf8;
use tempfile::{tempdir, TempDir};
use zstd::stream::Encoder as ZstEncoder;

#[derive(Debug)]
pub struct BuildScript {
  engine: Engine,
  ast: AST,
  path: Box<Path>,
  source: Source,
  source_dir: TempDir,
  arch: Box<str>,
}

impl BuildScript {
  pub fn new(path: PathBuf) -> anyhow::Result<Self> {
    let source_dir = tempdir()?;
    let arch = Command::new("uname").arg("-m").output()?.stdout;
    let arch = from_utf8(&arch)?.trim();
    let (engine, mut scope) = create_engine(source_dir.path(), arch.to_string());

    let ast = engine.compile_file_with_scope(&scope, path.clone())?;
    let mut value = engine.eval_ast_with_scope(&mut scope, &ast)?;
    let source = Source::from_dynamic(&mut value)?;
    if !source.info.architecture.contains(arch) {
      bail!("source architecture does not contain `{arch}`")
    }

    Ok(Self {
      engine,
      ast,
      path: path.into(),
      source,
      source_dir,
      arch: arch.into(),
    })
  }

  pub fn source(&self) -> &Source {
    &self.source
  }

  fn exec_shell(&self, dir: impl AsRef<Path>, x: &str) -> anyhow::Result<()> {
    let status = Command::new("sh")
      .args(["-c", &format!("set -e\n{x}")])
      .current_dir(dir)
      .status()?;
    if !status.success() {
      bail!("shell exited with {status}");
    }
    Ok(())
  }

  fn exec_fn(&self, dir: impl AsRef<Path>, f: &FnPtr, args: impl FuncArgs) -> anyhow::Result<()> {
    let result: Dynamic = f.call(&self.engine, &self.ast, args)?;
    if let Ok(x) = result.into_string() {
      self.exec_shell(dir, &x)?;
    }
    Ok(())
  }

  fn exec(&self, dir: impl AsRef<Path>, x: &Execution, args: impl FuncArgs) -> anyhow::Result<()> {
    match x {
      Execution::Shell(x) => self.exec_shell(dir, x),
      Execution::Fn(f) => self.exec_fn(dir, f, args),
    }
  }

  pub fn prepare(&self) -> anyhow::Result<()> {
    let source_dir = self.source_dir.path();

    // TODO: dependency check
    segment_info!("Checking dependencies...");
    println!("Not implemented, skipping");

    segment_info!("Fetching source...");
    fetch_source(source_dir, &self.source.info.source)?;

    if let Some(prepare) = &self.source.prepare {
      segment_info!("Preparing source...");
      self.exec(source_dir, prepare, ())?;
    }
    Ok(())
  }

  pub fn build(&self) -> anyhow::Result<()> {
    if let Some(build) = &self.source.build {
      segment_info!("Building package...");
      self.exec(self.source_dir.path(), build, ())?;
    }
    Ok(())
  }

  pub fn pack(&self) -> anyhow::Result<()> {
    segment_info!("Entering fakeroot...");
    let exe = std::env::current_exe()?;
    let status = Command::new("fakeroot")
      .args([
        &*exe,
        Path::new("__internal_package_inside_fakeroot"),
        &self.path,
        self.source_dir.path(),
        Path::new(&*self.arch),
      ])
      .status()?;
    if !status.success() {
      bail!("fakeroot exited with {status}");
    }
    segment_info!("Exiting fakeroot...");
    Ok(())
  }
}

#[derive(Debug)]
pub struct PackScript {
  engine: Engine,
  ast: AST,
  packages: BTreeSet<Package>,
  source_dir: Box<Path>,
  arch: SmartString<LazyCompact>,
}

impl PackScript {
  pub fn new(path: PathBuf, source_dir: &Path, arch: String) -> anyhow::Result<Self> {
    let (engine, mut scope) = create_engine(source_dir, arch.clone());
    let ast = engine.compile_file_with_scope(&scope, path)?;
    let mut value = engine.eval_ast_with_scope(&mut scope, &ast)?;
    let source = Source::from_dynamic(&mut value)?;
    Ok(Self {
      engine,
      ast,
      packages: source.packages,
      source_dir: source_dir.into(),
      arch: arch.into(),
    })
  }

  fn exec_shell(&self, dir: impl AsRef<Path>, x: &str) -> anyhow::Result<()> {
    let status = Command::new("sh")
      .args(["-c", &format!("set -e\n{x}")])
      .current_dir(dir)
      .status()?;
    if !status.success() {
      bail!("Shell exited with {status}");
    }
    Ok(())
  }

  fn exec_fn(&self, dir: impl AsRef<Path>, f: &FnPtr, args: impl FuncArgs) -> anyhow::Result<()> {
    let result: Dynamic = f.call(&self.engine, &self.ast, args)?;
    if let Ok(x) = result.into_string() {
      self.exec_shell(dir, &x)?;
    }
    Ok(())
  }

  pub fn pack(&self) -> anyhow::Result<()> {
    for package in &self.packages {
      segment_info!(
        "Starting packing:",
        "{} {}",
        package.info.name,
        package.info.version
      );
      let package_dir = tempdir()?;
      let path = package_dir
        .path()
        .to_str()
        .expect("tempdir path should be UTF-8")
        .to_string();
      if let Some(f) = &package.pack {
        self.exec_fn(&self.source_dir, f, [path])?;
      }

      segment_info!("Creating tarball...");
      let archive_name = format!(
        "{}_{}_{}.tar.zst",
        package.info.name, package.info.version, self.arch,
      );
      let mut archive = tar::Builder::new(ZstEncoder::new(File::create(&archive_name)?, 3)?);
      archive.follow_symlinks(false);

      let base = package_dir.path();
      let mut paths = vec![];
      let mut stack = vec![(base.to_path_buf(), true)];
      while let Some((path, is_dir)) = stack.pop() {
        if is_dir {
          for entry in path.read_dir()? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            stack.push((entry.path(), file_type.is_dir()))
          }
        }
        if path != base {
          paths.push(path);
        }
      }

      let pb = ProgressBar::new(paths.len() as _);
      pb.set_message(archive_name);
      pb.set_prefix("packing");
      let style = ProgressStyle::with_template(PB_STYLE)
        .unwrap()
        .progress_chars("=> ");
      pb.set_style(style);

      for path in paths {
        let name = path.strip_prefix(base)?;
        archive.append_path_with_name(&path, name)?;
        pb.inc(1);
      }

      let metadata = PackageMeta {
        architecture: self.arch.clone(),
        info: package.info.clone(),
      };
      let metadata = serde_json::to_vec_pretty(&metadata)?;
      let mut header = tar::Header::new_old();
      header.set_size(metadata.len() as _);
      header.set_path("metadata.json")?;
      header.set_mode(0o644);
      header.set_cksum();
      archive.append(&header, &*metadata)?;

      archive.into_inner()?.finish()?;
      pb.set_prefix("done");
      pb.finish();
    }
    Ok(())
  }
}
