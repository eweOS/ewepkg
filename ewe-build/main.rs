mod color;

use clap::Parser;
use color::red_bold;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
  path: PathBuf,
}

fn run() -> anyhow::Result<()> {
  let args = Args::parse();
  // TODO: main
  Ok(())
}

fn main() {
  if let Err(error) = run() {
    println!("{} {error:?}", red_bold("error:"))
  }
}
