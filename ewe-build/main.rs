use build_script::BuildScript;
use clap::Parser;
use std::path::PathBuf;

mod build_script;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
  path: PathBuf,
}

fn run() -> anyhow::Result<()> {
  let args = Args::parse();
  BuildScript::new(&args.path)?;
  Ok(())
}

fn main() {
  if let Err(error) = run() {
    println!("error: {error:?}")
  }
}
