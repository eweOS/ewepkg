mod fetch;
mod script;

pub use script::BuildScript;

use console::style;
use std::path::PathBuf;

pub fn run(path: PathBuf) -> anyhow::Result<()> {
  let script = BuildScript::new(path)?;
  let source = &script.source().meta;
  println!(
    "{} {} {} {}",
    style("::").green().bold(),
    style("Starting building:").bold(),
    source.name,
    source.version,
  );
  script.prepare()?;
  // script.build()?;
  script.pack()?;
  Ok(())
}
