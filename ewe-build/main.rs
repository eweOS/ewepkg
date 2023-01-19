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
  let script = BuildScript::new(&args.path)?;
  println!("{}", serde_json::to_string_pretty(script.source())?);
  Ok(())
}

fn main() {
  if let Err(error) = run() {
    println!("error: {error:?}")
  }
}
