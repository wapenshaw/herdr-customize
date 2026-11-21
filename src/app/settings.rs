use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};

use crate::fsutil;

#[derive(Clone, Copy)]
enum Kind {
    Number(f64, f64),
    Positive,
    Integer,
    Text,
    Path,
    Toggle,
    Choice(&'static [&'static str]),
}

struct Setting {
    key: &'static str,
    title: &'static str,
    group: &'static str,
    about: &'static str,
    hint: &'static str,
    default: &'static str,
    kind: Kind,
}

const SETTINGS: &[Setting] = &[
    Setting { key: "zoom", title: "Zoom", group: "Look", about: "Scale the whole app.", hint: "0.5 to 3", default: "1", kind: Kind::Number(0.5, 3.0) },
    Setting { key: "appearance", title: "Appearance", group: "Look", about: "Choose the light or dark appearance.", hint: "Light or dark", default: "system", kind: Kind::Choice(&["light", "dark"]) },
    Setting { key: "ui_font", title: "UI font", group: "Look", about: "Font for the app interface where the layout leaves it unset.", hint: "Font family", default: "Geist", kind: Kind::Text },
    Setting { key: "font", title: "Terminal font", group: "Terminal", about: "Font for terminals where the layout leaves it unset.", hint: "Font family", default: "JetBrains Mono", kind: Kind::Text },
    Setting { key: "font_size", title: "Font size", group: "Terminal", about: "Terminal font size where the layout leaves it unset.", hint: "More than zero", default: "14", kind: Kind::Positive },
    Setting {
        key: "font_weight",
        title: "Font weight",
        group: "Terminal",
        about: "Terminal font weight where the layout leaves it unset.",
        hint: "Integer from 100 to 900",
        default: "500",
        kind: Kind::Integer,
    },
    Setting { key: "terminal_line_height", title: "Line height", group: "Terminal", about: "Terminal row height multiplier.", hint: "0.8 to 2", default: "1", kind: Kind::Number(0.8, 2.0) },
    Setting { key: "terminal_cell_width", title: "Cell width", group: "Terminal", about: "Terminal column width multiplier.", hint: "0.8 to 2", default: "1", kind: Kind::Number(0.8, 2.0) },
    Setting { key: "terminal_scroll_speed", title: "Scroll speed", group: "Terminal", about: "Wheel and trackpad multiplier.", hint: "0.1 to 5", default: "1", kind: Kind::Number(0.1, 5.0) },
    Setting { key: "font_ligatures", title: "Font ligatures", group: "Terminal", about: "Enable programming ligatures in terminal fonts.", hint: "On or off", default: "false", kind: Kind::Toggle },
    Setting {
        key: "minimum_contrast",
        title: "Minimum contrast",
        group: "Terminal",
        about: "Adjust terminal text below this contrast ratio.",
        hint: "1 to 21",
        default: "1",
        kind: Kind::Number(1.0, 21.0),
    },
    Setting { key: "pane_scrollbars", title: "Pane scrollbars", group: "Terminal", about: "Show terminal pane scrollbars.", hint: "On or off", default: "true", kind: Kind::Toggle },
    Setting {
        key: "clipboard_write",
        title: "Clipboard writes",
        group: "Input and clipboard",
        about: "Allow terminal programs to write the clipboard.",
        hint: "On or off",
        default: "true",
        kind: Kind::Toggle,
    },
    Setting { key: "copy_on_select", title: "Copy on select", group: "Input and clipboard", about: "Copy a mouse selection on release.", hint: "On or off", default: "true", kind: Kind::Toggle },
    Setting {
        key: "option_as_alt",
        title: "Option as Alt",
        group: "Input and clipboard",
        about: "Choose how the macOS Option keys behave.",
        hint: "auto, on, off, left or right",
        default: "auto",
        kind: Kind::Choice(&["auto", "on", "off", "left", "right"]),
    },
    Setting {
        key: "worktrees.directory",
        title: "Worktree directory",
        group: "Behavior",
        about: "Directory for new worktrees on the space's machine.",
        hint: "Absolute path or ~",
        default: "~/.herdr/worktrees",
        kind: Kind::Path,
    },
    Setting {
        key: "core",
        title: "Core binary",
        group: "Behavior",
        about: "Path to the herdr-core binary; read when the app starts.",
        hint: "Path to herdr-core",
        default: "automatic",
        kind: Kind::Text,
    },
    Setting { key: "telemetry", title: "Telemetry", group: "Behavior", about: "Allow telemetry.", hint: "On or off", default: "true", kind: Kind::Toggle },
    Setting { key: "sounds.enabled", title: "Sounds", group: "Behavior", about: "Play agent notification sounds.", hint: "On or off", default: "true", kind: Kind::Toggle },
];

fn read(path: &Path) -> Result<Map<String, Value>> {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str(text.trim_start_matches('\u{feff}')).with_context(|| format!("{} is not valid JSON", path.display()))? {
            Value::Object(map) => Ok(map),
            _ => bail!("{} is not a JSON object", path.display()),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
    }
}

fn get<'a>(doc: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    match key.split_once('.') {
        Some((parent, child)) => doc.get(parent)?.as_object()?.get(child),
        None => doc.get(key),
    }
}

pub fn describe() -> Value {
    let path = super::home().join("settings.json");
    let (doc, error) = match read(&path) {
        Ok(doc) => (doc, None),
        Err(e) => (Map::new(), Some(format!("{e:#}"))),
    };
    let settings: Vec<Value> = SETTINGS.iter().map(|s| {
        let kind = match s.kind { Kind::Toggle => "toggle", Kind::Choice(_) => "choice", _ => "input" };
        let choices = match s.kind { Kind::Choice(values) => values, _ => &[] };
        let value = get(&doc, s.key).map(shown);
        json!({ "key": s.key, "title": s.title, "group": s.group, "about": s.about, "hint": s.hint, "default": s.default, "kind": kind, "choices": choices, "value": value, "macos_only": s.key == "option_as_alt", "restart": s.key == "core" })
    }).collect();
    json!({ "path": path, "error": error, "settings": settings })
}

/// A setting as the page shows it. The app writes some numbers from single
/// precision, so 1.4 arrives as 1.399999976158142 and is shown as 1.4.
fn shown(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(n) if n.is_f64() => n.as_f64().map_or_else(|| n.to_string(), |f| if f64::from(f as f32) == f { format!("{:?}", f as f32) } else { f.to_string() }),
        _ => value.to_string(),
    }
}

fn parse(s: &Setting, text: &str) -> Result<Value> {
    let text = text.trim();
    Ok(match s.kind {
        Kind::Text => {
            if text.is_empty() {
                bail!("{} cannot be empty", s.title);
            }
            json!(text)
        }
        Kind::Path => {
            let bytes = text.as_bytes();
            let windows_absolute = bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/');
            if !(Path::new(text).is_absolute() || windows_absolute || text.starts_with("\\\\") || text == "~" || text.starts_with("~/") || text.starts_with("~\\")) {
                bail!("{} must be an absolute path or start with ~/", s.title);
            }
            json!(text)
        }
        Kind::Toggle => match text {
            "true" | "on" => json!(true),
            "false" | "off" => json!(false),
            _ => bail!("{} must be on or off", s.title),
        },
        Kind::Choice(values) => {
            if !values.contains(&text) {
                bail!("{} must be {}", s.title, values.join(", "));
            }
            json!(text)
        }
        Kind::Integer => {
            let n: u64 = text.parse().with_context(|| format!("{} must be an integer", s.title))?;
            if !(100..=900).contains(&n) {
                bail!("{} must be from 100 to 900", s.title);
            }
            json!(n)
        }
        Kind::Number(min, max) => {
            let n: f64 = text.parse().with_context(|| format!("{} must be a number", s.title))?;
            if !n.is_finite() || !(min..=max).contains(&n) {
                bail!("{} must be from {min} to {max}", s.title);
            }
            json!(n)
        }
        Kind::Positive => {
            let n: f64 = text.parse().with_context(|| format!("{} must be a number", s.title))?;
            if !n.is_finite() || n <= 0.0 {
                bail!("{} must be more than zero", s.title);
            }
            json!(n)
        }
    })
}

pub fn set(key: &str, text: Option<&str>) -> Result<()> {
    let setting = SETTINGS.iter().find(|s| s.key == key).with_context(|| format!("no app setting {key:?}"))?;
    let item = text.map(|text| parse(setting, text)).transpose()?;
    let path = super::home().join("settings.json");
    let _lock = fsutil::write_lock(&path)?;
    let mut doc = read(&path)?;
    if let Some((parent, child)) = key.split_once('.') {
        if item.is_some() && !doc.contains_key(parent) {
            doc.insert(parent.into(), json!({}));
        }
        if let Some(value) = doc.get_mut(parent) {
            let object = value.as_object_mut().with_context(|| format!("{parent} must be an object"))?;
            match item {
                Some(item) => {
                    object.insert(child.into(), item);
                }
                None => {
                    object.shift_remove(child);
                }
            }
        }
    } else {
        match item {
            Some(item) => {
                doc.insert(key.into(), item);
            }
            None => {
                doc.shift_remove(key);
            }
        }
    }
    let mut bytes = serde_json::to_vec_pretty(&doc)?;
    bytes.push(b'\n');
    fsutil::write_backed_up(&path, &bytes, 0o644)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_defaults_match_the_app() {
        for (key, default) in [("font", "JetBrains Mono"), ("ui_font", "Geist"), ("font_size", "14"), ("font_weight", "500")] {
            let setting = SETTINGS.iter().find(|s| s.key == key).unwrap();
            assert_eq!(setting.default, default);
            assert!(parse(setting, default).is_ok());
        }
    }

    #[test]
    fn single_precision_numbers_show_short() {
        assert_eq!(shown(&serde_json::json!(1.399999976158142)), "1.4");
        assert_eq!(shown(&serde_json::json!(1.4)), "1.4");
        assert_eq!(shown(&serde_json::json!(1.0)), "1.0");
        assert_eq!(shown(&serde_json::json!(14)), "14");
        assert_eq!(shown(&serde_json::json!("dark")), "dark");
    }

    #[test]
    fn preserves_unknown_fields_and_nested_values() {
        let _guard = crate::testenv::isolated();
        let path = super::super::home().join("settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"other":{"x":1},"sounds":{"done":"bell","enabled":true}}"#).unwrap();
        set("sounds.enabled", Some("false")).unwrap();
        set("worktrees.directory", Some("~/work")).unwrap();
        let doc = read(&path).unwrap();
        assert_eq!(doc["other"]["x"], 1);
        assert_eq!(doc["sounds"]["done"], "bell");
        assert_eq!(doc["sounds"]["enabled"], false);
        assert_eq!(doc["worktrees"]["directory"], "~/work");
        set("sounds.enabled", None).unwrap();
        assert!(read(&path).unwrap()["sounds"].get("enabled").is_none());
        set("worktrees.directory", None).unwrap();
        assert!(read(&path).unwrap()["worktrees"].get("directory").is_none());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn refuses_broken_files_and_invalid_values() {
        let _guard = crate::testenv::isolated();
        let path = super::super::home().join("settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{broken").unwrap();
        assert!(set("zoom", Some("1")).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{broken");
        assert!(set("zoom", Some("3.1")).is_err());
        assert!(set("font_weight", Some("100.5")).is_err());
        assert!(set("appearance", Some("system")).is_err());
        assert!(set("worktrees.directory", Some("relative")).is_err());
        std::fs::remove_file(&path).unwrap();
    }
}
