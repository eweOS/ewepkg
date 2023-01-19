use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::Ordering::{self, *};
use std::num::ParseIntError;
use std::str::FromStr;
use thiserror::Error;

#[cfg(feature = "mlua")]
use mlua::{ExternalResult, FromLua};

fn is_allowed_in_version(c: char) -> bool {
  c.is_ascii_alphanumeric() || ".+~".contains(c)
}

fn cmp_lexical(a: &str, b: &str) -> Ordering {
  let is_invalid = |c: char| !c.is_ascii_alphabetic() && !".+~".contains(c);
  assert!(!a.contains(is_invalid));
  assert!(!b.contains(is_invalid));

  let (mut ai, mut bi) = (a.bytes().peekable(), b.bytes().peekable());
  while let (Some(&ac), Some(&bc)) = (ai.peek(), bi.peek()) {
    let _ = (ai.next(), bi.next());
    if ac == bc {
      continue;
    }
    match (ac, bc) {
      (b'~', _) => return Less,
      (_, b'~') => return Greater,
      _ => {}
    }
    if ac.is_ascii_alphabetic() && !bc.is_ascii_alphabetic() {
      return Less;
    }
    if !ac.is_ascii_alphabetic() && bc.is_ascii_alphabetic() {
      return Greater;
    }
    return ac.cmp(&bc);
  }

  match (ai.next(), bi.next()) {
    (Some(b'~'), None) => Less,
    (None, Some(b'~')) | (Some(_), None) => Greater,
    (None, Some(_)) => Less,
    _ => Equal,
  }
}

fn cmp_numerical(a: &str, b: &str) -> Ordering {
  let is_not_numeric = |c: char| !c.is_numeric();
  assert!(!a.contains(is_not_numeric));
  assert!(!b.contains(is_not_numeric));

  let ai = a.trim_start_matches('0');
  let bi = b.trim_start_matches('0');

  match ai.len().cmp(&bi.len()) {
    Equal => ai.cmp(bi),
    ord => ord,
  }
}

pub fn cmp_version(mut a: &str, mut b: &str) -> Ordering {
  assert!(!a.contains(|c: char| !is_allowed_in_version(c)));
  assert!(!b.contains(|c: char| !is_allowed_in_version(c)));

  while !a.is_empty() || !b.is_empty() {
    let (asub1, a1) = a.split_at(a.find(char::is_numeric).unwrap_or(a.len()));
    let (bsub1, b1) = b.split_at(b.find(char::is_numeric).unwrap_or(b.len()));
    dbg!(asub1, bsub1);
    match cmp_lexical(asub1, bsub1) {
      Equal => {}
      ord => return dbg!(ord),
    }
    let is_not_numeric = |c: char| !c.is_numeric();
    let (asub2, a2) = a1.split_at(a1.find(is_not_numeric).unwrap_or(a1.len()));
    let (bsub2, b2) = b1.split_at(b1.find(is_not_numeric).unwrap_or(b1.len()));
    dbg!(asub2, bsub2);
    match cmp_numerical(asub2, bsub2) {
      Equal => (a, b) = (a2, b2),
      ord => return dbg!(ord),
    }
  }
  Equal
}

#[derive(Debug, Clone)]
pub struct PkgVersion {
  epoch: u32,
  upstream: Box<str>,
  revision: Option<Box<str>>,
}

impl PkgVersion {
  pub fn epoch(&self) -> u32 {
    self.epoch
  }

  pub fn upstream(&self) -> &str {
    &self.upstream
  }

  pub fn revision(&self) -> Option<&str> {
    self.revision.as_deref()
  }
}

impl FromStr for PkgVersion {
  type Err = ParseVersionError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let (epoch, s) = s
      .split_once(':')
      .map(|(e, s)| (Some(e), s))
      .unwrap_or((None, s));
    let epoch = epoch
      .map(|e| u32::from_str_radix(e, 10))
      .transpose()?
      .unwrap_or(0);
    let (upstream, revision) = s
      .rsplit_once('-')
      .map(|(u, r)| (u, Some(r)))
      .unwrap_or_else(|| (s, None));

    if let Some(c) = upstream.chars().find(|c| !is_allowed_in_version(*c)) {
      return Err(ParseVersionError::Upstream(c));
    }
    if let Some(r) = revision {
      if let Some(c) = r.chars().find(|c| !is_allowed_in_version(*c)) {
        return Err(ParseVersionError::Revision(c));
      }
    }

    Ok(Self {
      epoch,
      upstream: upstream.into(),
      revision: revision.map(Into::into),
    })
  }
}

impl PartialOrd for PkgVersion {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for PkgVersion {
  fn cmp(&self, other: &Self) -> Ordering {
    match self.epoch.cmp(&other.epoch) {
      Equal => {}
      ord => return ord,
    }
    match cmp_version(&self.upstream, &other.upstream) {
      Equal => {}
      ord => return ord,
    }
    cmp_version(
      self.revision.as_deref().unwrap_or(""),
      other.revision.as_deref().unwrap_or(""),
    )
  }
}

impl PartialEq for PkgVersion {
  fn eq(&self, other: &Self) -> bool {
    self.cmp(other) == Equal
  }
}

impl Eq for PkgVersion {}

impl Serialize for PkgVersion {
  fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
    let u = &self.upstream;
    match (self.epoch, &self.revision) {
      (0, None) => ser.serialize_str(u),
      (e, None) => ser.serialize_str(&format!("{e}:{u}")),
      (0, Some(r)) => ser.serialize_str(&format!("{u}-{r}")),
      (e, Some(r)) => ser.serialize_str(&format!("{e}:{u}-{r}")),
    }
  }
}

impl<'de> Deserialize<'de> for PkgVersion {
  fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
    <&'de str>::deserialize(de)?
      .parse()
      .map_err(de::Error::custom)
  }
}

#[cfg(feature = "mlua")]
impl<'lua> FromLua<'lua> for PkgVersion {
  fn from_lua(lua_value: mlua::Value<'lua>, lua: &'lua mlua::Lua) -> mlua::Result<Self> {
    let s: mlua::String = lua.unpack(lua_value)?;
    std::str::from_utf8(s.as_bytes())?.parse().to_lua_err()
  }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ParseVersionError {
  #[error("failed to parse epoch: {0}")]
  Epoch(#[from] ParseIntError),
  #[error("upstream version contains invalid character `{0}`")]
  Upstream(char),
  #[error("revision contains invalid character `{0}`")]
  Revision(char),
}

#[cfg(test)]
mod tests {
  use super::*;

  fn parse_ver(s: &str) -> Result<PkgVersion, ParseVersionError> {
    s.parse()
  }

  fn ver(s: &str) -> PkgVersion {
    s.parse().unwrap()
  }

  #[test]
  fn test_parse_version() {
    assert_eq!(
      parse_ver("1:2.33+beta1-4"),
      Ok(PkgVersion {
        epoch: 1,
        upstream: "2.33+beta1".into(),
        revision: Some("4".into())
      })
    );

    // `-` is not allowed inside versions, only acts as seperator between upstream
    // version and revision.
    assert_eq!(
      parse_ver("2.33-beta1-4"),
      Err(ParseVersionError::Upstream('-'))
    );
  }

  #[test]
  fn test_compare_version() {
    assert_eq!(cmp_lexical("~beta", ""), Less);
    assert_eq!(cmp_lexical("+dfsg", ""), Greater);
    assert_eq!(cmp_numerical("1", "01"), Equal);
    assert_eq!(cmp_numerical("19260817", "19530615"), Less);
    // assert!(ver("1.14.51~beta4-999") < ver("1.14.51-4"));
    assert!(dbg!(ver("0.12.10+dfsg1-3")) == dbg!(ver("0.12.10+dfsg01-3")));
  }
}
