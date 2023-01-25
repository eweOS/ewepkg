mod build;
mod source;
mod util;
mod version;

use clap::{Parser, Subcommand};
use console::style;
use std::path::PathBuf;
use std::process::exit;

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

fn run() -> anyhow::Result<()> {
  let args = Args::parse();
  match args.cmd {
    Command::Build { path } => build::run(path)?,
  }
  Ok(())
}

fn main() {
  if let Err(error) = run() {
    eprint!("{} error: {error}", style("!!").red().bold());
    if let Some(x) = error.chain().nth(1) {
      eprintln!(" ({x})");
    } else {
      eprintln!();
    }
    exit(1);
  }
}
