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
  Build {
    path: PathBuf,
  },
  #[command(name = "__internal_package_inside_fakeroot", hide = true)]
  Package {
    path: PathBuf,
    source_dir: PathBuf,
  },
}

fn run() -> anyhow::Result<()> {
  let args = Args::parse();
  match args.cmd {
    Command::Build { path } => build::run(path)?,
    Command::Package { path, source_dir } => build::run_package(path, source_dir)?,
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
