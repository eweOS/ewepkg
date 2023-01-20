use crate::color::green_bold;
use build_script::BuildScript;
use clap::Parser;
use color::red_bold;
use std::path::PathBuf;

mod build_script;
mod color;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
  path: PathBuf,
}

fn run() -> anyhow::Result<()> {
  let args = Args::parse();
  let script = BuildScript::new(&args.path)?;
  println!(
    "{} Building source {} version {}",
    green_bold("::"),
    script.source().name,
    script.source().version
  );
  script.build()?;
  Ok(())
}

fn main() {
  if let Err(error) = run() {
    println!("{} {error:?}", red_bold("error:"))
  }
}
