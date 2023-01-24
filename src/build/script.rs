use crate::source::{Execution, Source, SourceFile, SourceLocation};
use crate::util::mv_async;
use futures::stream::FuturesUnordered;
use futures::TryStreamExt;
use reqwest::Client;
use rhai::{Dynamic, Engine, FnPtr, Scope, AST};
use std::fs::read_to_string;
use std::path::Path;
use tempfile::{tempdir, TempDir};
use tokio::fs::File;
use tokio::io;
use tokio::runtime::Runtime;
use tokio_util::io::StreamReader;

async fn fetch_single_source(
  source_dir: &Path,
  file: &SourceFile,
  client: Client,
) -> anyhow::Result<()> {
  println!("{} pushed", file.location);
  let dst = source_dir.with_file_name(file.file_name());
  match &file.location {
    SourceLocation::Http(url) => {
      let resp = client.get(url.clone()).send().await?.error_for_status()?;
      println!("{url} content length: {:?}", resp.content_length());
      let stream = resp
        .bytes_stream()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e));
      let mut reader = StreamReader::new(stream);
      let mut file = File::create(dst).await?;
      io::copy(&mut reader, &mut file).await?;
      println!("download {url} complete");
    }
    SourceLocation::Local(path) => mv_async(path, dst).await?,
  }
  println!("{} complete", file.location);
  Ok(())
}

async fn fetch_source(source_dir: &Path, files: &[SourceFile]) -> anyhow::Result<()> {
  const PARALLEL: usize = 5;
  let mut iter = files.iter();
  let mut pool = FuturesUnordered::new();
  let client = Client::new();

  for file in iter.by_ref().take(PARALLEL) {
    pool.push(fetch_single_source(source_dir, file, client.clone()));
  }
  while let Some(()) = pool.try_next().await? {
    if let Some(file) = iter.next() {
      pool.push(fetch_single_source(source_dir, file, client.clone()));
    }
  }
  Ok(())
}

#[derive(Debug)]
pub struct BuildScript {
  engine: Engine,
  ast: AST,
  source: Source,
  source_dir: TempDir,
}

impl BuildScript {
  pub fn new(path: impl AsRef<Path>) -> anyhow::Result<Self> {
    let engine = Engine::new();
    let mut scope = Scope::new();
    let source_dir = tempdir()?;
    let source_dir_path = source_dir
      .path()
      .to_str()
      .expect("tempdir path is not UTF-8")
      .to_string();

    scope.push("source_dir", source_dir_path);

    let ast = engine.compile_with_scope(&mut scope, &read_to_string(path)?)?;
    let mut value = engine.eval_ast_with_scope(&mut scope, &ast)?;
    let source = Source::from_dynamic(&mut value)?;
    Ok(Self {
      engine,
      ast,
      source,
      source_dir,
    })
  }

  pub fn source(&self) -> &Source {
    &self.source
  }

  fn exec_shell(&self, _dir: impl AsRef<Path>, x: &str) -> anyhow::Result<()> {
    todo!()
  }

  fn exec_fn(&self, dir: impl AsRef<Path>, f: &FnPtr) -> anyhow::Result<()> {
    let result: Dynamic = f.call(&self.engine, &self.ast, ())?;
    if let Ok(x) = result.into_string() {
      self.exec_shell(dir, &x)?;
    }
    Ok(())
  }

  fn exec(&self, dir: impl AsRef<Path>, x: &Execution) -> anyhow::Result<()> {
    match x {
      Execution::Shell(x) => self.exec_shell(dir, x),
      Execution::Fn(f) => self.exec_fn(dir, f),
    }
  }

  pub fn prepare(&self) -> anyhow::Result<()> {
    let rt = Runtime::new()?;
    rt.block_on(fetch_source(
      self.source_dir.path(),
      &self.source.meta.source,
    ))?;
    // TODO: dependency check
    if let Some(prepare) = &self.source.prepare {
      self.exec(self.source_dir.path(), prepare)?;
    }
    Ok(())
  }

  pub fn build(&self) -> anyhow::Result<()> {
    if let Some(build) = &self.source.build {
      self.exec(self.source_dir.path(), build)?;
    }
    Ok(())
  }

  pub fn pack(&self) -> anyhow::Result<()> {
    todo!()
  }
}
