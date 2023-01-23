mod script;
mod source;
mod version;

use clap::{Parser, Subcommand};
use script::BuildScript;
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
      let script = BuildScript::new(path)?;
      println!("{:#?}", script.source());
      script.prepare()?;
    }
  }
  Ok(())
}
