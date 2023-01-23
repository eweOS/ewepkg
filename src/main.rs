mod source;
mod version;

use clap::{Parser, Subcommand};
use rhai::{Engine, Scope};
use source::Source;
use std::fs::read_to_string;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
  #[command(subcommand)]
  cmd: Command,
}

#[derive(Subcommand)]
enum Command {
  Build { path: PathBuf },
}

fn main() -> anyhow::Result<()> {
  let args = Args::parse();
  match args.cmd {
    Command::Build { path } => {
      let engine = Engine::new();
      let mut scope = Scope::new();
      scope.push("source_dir", ".").push("package_dir", ".");
      let mut value = engine.eval_with_scope(&mut scope, &read_to_string(path)?)?;
      let source = Source::from_dynamic(&mut value)?;
      println!("{source:#?}");
    }
  }
  Ok(())
}
