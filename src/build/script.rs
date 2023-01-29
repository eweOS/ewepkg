use crate::build::fetch::fetch_source;
use crate::segment_info;
use crate::source::{Execution, Package, Source};
use crate::util::PB_STYLE;
use anyhow::bail;
use indicatif::{ProgressBar, ProgressStyle};
use rhai::{Dynamic, Engine, FnPtr, FuncArgs, Scope, AST};
use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{tempdir, TempDir};
use xz2::write::XzEncoder;

#[derive(Debug)]
pub struct BuildScript {
  engine: Engine,
  ast: AST,
  path: Box<Path>,
  source: Source,
  source_dir: TempDir,
}

impl BuildScript {
  pub fn new(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
    let path = path.into();
    let engine = Engine::new();
    let mut scope = Scope::new();
    let source_dir = tempdir()?;
    let source_dir_path = source_dir
      .path()
      .to_str()
      .expect("tempdir path is not UTF-8")
      .to_string();
    scope.push("source_dir", source_dir_path);

    let ast = engine.compile_file_with_scope(&scope, path.clone())?;
    let mut value = engine.eval_ast_with_scope(&mut scope, &ast)?;
    let source = Source::from_dynamic(&mut value)?;
    Ok(Self {
      engine,
      ast,
      path: path.into(),
      source,
      source_dir,
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
    fetch_source(source_dir, &self.source.meta.source)?;

    segment_info!("Preparing source...");
    if let Some(prepare) = &self.source.prepare {
      self.exec(source_dir, prepare, ())?;
    }
    Ok(())
  }

  pub fn build(&self) -> anyhow::Result<()> {
    segment_info!("Building package...");
    if let Some(build) = &self.source.build {
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
}

impl PackScript {
  pub fn new(path: impl Into<PathBuf>, source_dir: impl Into<PathBuf>) -> anyhow::Result<Self> {
    let engine = Engine::new();
    let mut scope = Scope::new();
    let source_dir = source_dir.into();
    let source_dir_str = source_dir
      .to_str()
      .expect("tempdir path is not UTF-8")
      .to_string();
    scope.push("source_dir", source_dir_str);

    let ast = engine.compile_file_with_scope(&scope, path.into())?;
    let mut value = engine.eval_ast_with_scope(&mut scope, &ast)?;
    let source = Source::from_dynamic(&mut value)?;
    Ok(Self {
      engine,
      ast,
      packages: source.packages,
      source_dir: source_dir.into(),
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
        package.meta.name,
        package.meta.version
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
      let archive_name = format!("{}_{}.tar.xz", package.meta.name, package.meta.version);
      let mut archive = tar::Builder::new(XzEncoder::new(File::create(&archive_name)?, 1));
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
      pb.set_prefix(archive_name);
      let style = ProgressStyle::with_template(PB_STYLE)
        .unwrap()
        .progress_chars("=> ");
      pb.set_style(style);

      for path in paths {
        archive.append_path_with_name(&path, path.strip_prefix(base)?)?;
        pb.inc(1);
      }

      archive.finish()?;
      pb.finish_with_message("done");
    }
    Ok(())
  }
}
