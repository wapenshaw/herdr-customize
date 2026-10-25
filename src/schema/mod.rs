//! Herdr's config keys, from the config reference Herdr publishes with each
//! release. A copy for the version this plugin was built against is
//! embedded; for_installed prefers a copy that matches the installed Herdr.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Result, bail};
use regex::Regex;
use serde::Deserialize;

use crate::config::{self, Value};
use crate::herdr;

/// The Herdr release the embedded reference comes from.
const BUNDLED_VERSION: &str = "0.9.5";

static BUNDLED: LazyLock<Reference> = LazyLock::new(|| {
    let mut r = parse(include_str!("reference.json")).expect("embedded config reference");
    r.version = BUNDLED_VERSION.into();
    r
});

// Key types used by the reference.
pub const BOOLEAN: &str = "boolean";
pub const INTEGER: &str = "integer";
pub const STRING: &str = "string";
pub const ENUM: &str = "enum";
pub const KEYBINDING: &str = "keybinding";
pub const ARRAY: &str = "array";
pub const STRING_LIST: &str = "list of strings";
pub const TOKEN_ROWS: &str = "list of token rows";
pub const TOKEN_TABLES: &str = "table of token rows";
/// Not in Herdr's reference; plugin values can be decimals.
pub const FLOAT: &str = "float";

/// The whole config reference.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Reference {
    /// The Herdr release it describes.
    #[serde(skip)]
    pub version: String,
    /// The config file the keys live in, for plugin schemas that use this
    /// format; Herdr's own reference leaves it empty.
    #[serde(default)]
    pub file: String,
    pub sections: Vec<Section>,
}

/// Related keys, such as "ui" or "keys".
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Section {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub keys: Vec<Key>,
}

/// One config key.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Key {
    pub key: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub default: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub values: Vec<String>,
}

static ACCEPTS_LIST: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Accepts ((?:[a-z_]+, )+[a-z_]+)\b").unwrap());

impl Key {
    pub fn path(&self) -> Vec<String> {
        config::split_key(&self.key)
    }

    /// Decodes the default. None for defaults that are prose, such as
    /// "unset" or a value with a platform note.
    pub fn default_value(&self) -> Option<Value> {
        config::decode_value(&self.default).ok()
    }

    /// The values the key takes: an enum's values, or for a string key the
    /// names its description lists after "Accepts" (as in "Accepts ctrl, alt,
    /// cmd, or a + separated combination"). Only enums are closed; see open.
    pub fn choices(&self) -> Vec<String> {
        if !self.values.is_empty() {
            return self.values.clone();
        }
        if self.kind != STRING {
            return Vec::new();
        }
        let Some(m) = ACCEPTS_LIST.captures(&self.description) else { return Vec::new() };
        let mut choices = Vec::new();
        // An empty value comes first when the description says it disables
        // the setting.
        if self.description.contains("Empty disables") {
            choices.push(String::new());
        }
        // "a, b, or a combination" ends the list at "or".
        choices.extend(m[1].split(", ").take_while(|n| *n != "or" && *n != "and").map(String::from));
        choices
    }

    /// Whether choices are only suggestions: other values are valid too.
    pub fn open(&self) -> bool {
        self.kind != ENUM && !self.choices().is_empty()
    }
}

impl Reference {
    pub fn section(&self, id: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.id == id)
    }

    pub fn lookup(&self, key: &str) -> Option<&Key> {
        self.sections.iter().flat_map(|s| &s.keys).find(|k| k.key == key)
    }

    /// Whether some key lives under prefix.
    pub fn covers_prefix(&self, prefix: &str) -> bool {
        let prefix = format!("{prefix}.");
        self.sections.iter().flat_map(|s| &s.keys).any(|k| k.key.starts_with(&prefix))
    }
}

/// Reads a reference in Herdr's config-reference.json format.
pub fn parse(data: &str) -> Result<Reference> {
    let r: Reference = serde_json::from_str(data)?;
    if r.sections.is_empty() {
        bail!("config reference has no sections");
    }
    Ok(r)
}

pub fn bundled() -> Reference {
    BUNDLED.clone()
}

/// The reference for the installed Herdr: the bundled one when versions
/// match, else a cached or freshly downloaded copy, else the bundled one.
pub fn for_installed() -> Reference {
    let Ok(version) = herdr::version() else { return bundled() };
    if version == BUNDLED_VERSION {
        return bundled();
    }
    let dir = cache_dir();
    cached(&dir, &version).or_else(|| fetch(&dir, &version).ok()).unwrap_or_else(bundled)
}

fn cache_dir() -> PathBuf {
    crate::theme::state_dir().join("schema")
}

fn cache_path(dir: &Path, version: &str) -> PathBuf {
    dir.join(format!("config-reference-{version}.json"))
}

/// The cached reference for version, if there is one.
fn cached(dir: &Path, version: &str) -> Option<Reference> {
    let mut r = parse(&std::fs::read_to_string(cache_path(dir, version)).ok()?).ok()?;
    r.version = version.into();
    Some(r)
}

/// Where Herdr publishes the reference for a release.
fn url(version: &str) -> String {
    format!("https://raw.githubusercontent.com/herdrdev/herdr/v{version}/docs/next/website/src/data/config-reference.json")
}

/// Downloads the reference for version into dir and returns it.
fn fetch(dir: &Path, version: &str) -> Result<Reference> {
    let body = ureq::get(&url(version)).config().timeout_global(Some(Duration::from_secs(5))).build().call()?.body_mut().read_to_string()?;
    let mut r = parse(&body)?;
    r.version = version.into();
    if std::fs::create_dir_all(dir).is_ok() {
        let _ = std::fs::write(cache_path(dir, version), body);
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_reference_parses() {
        let r = bundled();
        assert!(r.section("keys").is_some());
        assert!(r.lookup("ui.sidebar_width").is_some() || r.sections.len() > 3);
    }

    #[test]
    fn choices_from_enums_and_descriptions() {
        let k = Key { key: "a".into(), kind: ENUM.into(), values: vec!["x".into(), "y".into()], ..Default::default() };
        assert_eq!(k.choices(), ["x", "y"]);
        assert!(!k.open());
        let k = Key { key: "b".into(), kind: STRING.into(), description: "Modifier. Accepts ctrl, alt, cmd, or a + separated combination. Empty disables it.".into(), ..Default::default() };
        assert_eq!(k.choices(), ["", "ctrl", "alt", "cmd"]);
        assert!(k.open());
        let k = Key { key: "c".into(), kind: STRING.into(), description: "Accepts anything".into(), ..Default::default() };
        assert!(k.choices().is_empty());
    }

    #[test]
    fn defaults_decode_or_stay_prose() {
        let k = Key { default: "120".into(), ..Default::default() };
        assert_eq!(k.default_value(), Some(Value::Integer(120)));
        let k = Key { default: "unset".into(), ..Default::default() };
        assert_eq!(k.default_value(), None);
    }
}
