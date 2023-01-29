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
    #[arg(default_value = "ewebuild")]
    path: PathBuf,
  },
  #[command(name = "__internal_package_inside_fakeroot", hide = true)]
  InternalPackage {
    path: PathBuf,
    source_dir: PathBuf,
    arch: String,
  },
}

fn run() -> anyhow::Result<()> {
  let args = Args::parse();
  match args.cmd {
    Command::Build { path } => build::run(path)?,
    Command::InternalPackage {
      path,
      source_dir,
      arch,
    } => build::run_package(path, source_dir, arch)?,
  }
  Ok(())
}

fn main() {
  if let Err(error) = run() {
    eprint!("{} {error}", style("error:").red().bold());
    if let Some(x) = error.chain().nth(1) {
      eprintln!(" ({x})");
    } else {
      eprintln!();
    }
    exit(1);
  }
}
