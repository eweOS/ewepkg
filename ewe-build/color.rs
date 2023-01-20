use once_cell::sync::Lazy;
use std::fmt::{Display, Write};

static NO_COLOR: Lazy<bool> = Lazy::new(|| std::env::var_os("NO_COLOR").is_some());

pub fn style<'a>(s: impl Display, codes: impl IntoIterator<Item = &'a str>) -> String {
  if *NO_COLOR {
    s.to_string()
  } else {
    let mut colored = String::new();
    for code in codes {
      colored += "\x1b[";
      colored += code;
      colored += "m";
    }
    write!(&mut colored, "{s}").unwrap();
    colored += "\x1b[0m";
    colored
  }
}

pub fn red_bold(s: impl Display) -> String {
  style(s, ["31", "1"])
}

pub fn green_bold(s: impl Display) -> String {
  style(s, ["32", "1"])
}
