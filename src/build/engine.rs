use rhai::{Array, Engine, Map, Scope};
use std::path::Path;

macro_rules! gen_conditional {
  ($type:ident) => {
    paste::paste! {
      |cond: bool, value: $type| -> $type {
        if cond {
          value
        } else {
          Default::default()
        }
      }
    }
  };
}

pub fn create_engine(source_dir: &Path, arch: String) -> (Engine, Scope<'static>) {
  let mut engine = Engine::new();
  engine
    .register_fn("conditional", gen_conditional!(Array))
    .register_fn("conditional", gen_conditional!(Map));

  let source_dir_path = source_dir
    .to_str()
    .expect("tempdir path is not UTF-8")
    .to_string();

  let mut scope = Scope::new();
  scope.push("source_dir", source_dir_path);
  scope.push("arch", arch);

  (engine, scope)
}
