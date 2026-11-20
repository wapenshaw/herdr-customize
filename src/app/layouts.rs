use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};

use crate::fsutil;

const FIELDS: &[&str] = &[
    "theme",
    "font",
    "font_size",
    "ui_font",
    "font_weight",
    "terminal_padding",
    "terminal_opacity",
    "terminal_line_height",
    "terminal_cell_width",
    "terminal_scroll_speed",
    "window_blur",
    "titlebar",
    "terminal_menu",
    "toast",
    "link_handlers",
];

fn names(dir: &Path, folders: bool) -> BTreeSet<String> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if folders {
                path.is_dir().then(|| entry.file_name().to_string_lossy().into_owned())
            } else if path.extension().is_some_and(|ext| ext == "json") && path.is_file() {
                path.file_stem().map(|s| s.to_string_lossy().into_owned())
            } else {
                None
            }
        })
        .collect()
}

fn themes() -> BTreeSet<String> {
    let mut result = names(&super::home().join("themes"), false);
    result.extend(["graphite", "ink", "moss", "paper"].map(str::to_owned));
    result
}

fn read(path: &Path) -> Result<Map<String, Value>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    match serde_json::from_str(text.trim_start_matches('\u{feff}')).with_context(|| format!("{} is not valid JSON", path.display()))? {
        Value::Object(object) => Ok(object),
        _ => bail!("{} is not a JSON object", path.display()),
    }
}

pub fn describe() -> Value {
    let home = super::home();
    let shipped = super::shipped();
    let layouts: Vec<Value> = names(&home.join("layouts"), false)
        .into_iter()
        .map(|name| {
            let is_shipped = shipped.contains(&format!("layout:{name}"));
            let (doc, error) = match read(&home.join("layouts").join(format!("{name}.json"))) {
                Ok(doc) => (doc, None),
                Err(e) => (Map::new(), Some(format!("{e:#}"))),
            };
            let root: Map<String, Value> = FIELDS.iter().map(|field| ((*field).into(), doc.get(*field).cloned().unwrap_or(Value::Null))).collect();
            json!({"name": name, "shipped": is_shipped, "personal": !is_shipped, "root": root, "error": error})
        })
        .collect();
    json!({"layouts": layouts, "mods": names(&home.join("mods"), true), "themes": themes()})
}

fn parse(field: &str, text: &str) -> Result<Value> {
    let text = text.trim();
    let mods = || names(&super::home().join("mods"), true);
    let theme_names = themes();
    let valid_theme = |v: &Value| v.as_str().is_some_and(|name| theme_names.contains(name));
    let number = |min: f64, max: f64| -> Result<Value> {
        let n: f64 = text.parse().with_context(|| format!("{field} must be a number"))?;
        if !n.is_finite() || !(min..=max).contains(&n) {
            bail!("{field} must be from {min} to {max}");
        }
        Ok(json!(n))
    };
    Ok(match field {
        "theme" => {
            let value = if text.starts_with('{') { serde_json::from_str(text).context("theme must be a name or light/dark object")? } else { json!(text) };
            let valid =
                if let Some(pair) = value.as_object() { pair.len() == 2 && pair.get("light").is_some_and(valid_theme) && pair.get("dark").is_some_and(valid_theme) } else { valid_theme(&value) };
            if !valid {
                bail!("theme must name installed themes for both appearances");
            }
            value
        }
        "font" | "ui_font" => {
            if text.is_empty() {
                bail!("{field} cannot be empty");
            }
            json!(text)
        }
        "font_size" => number(f64::MIN_POSITIVE, f64::MAX)?,
        "font_weight" => {
            let n: u64 = text.parse().context("font_weight must be an integer")?;
            if !(100..=900).contains(&n) {
                bail!("font_weight must be from 100 to 900");
            }
            json!(n)
        }
        "terminal_padding" => {
            let value: Value = serde_json::from_str(text).context("terminal_padding must be a number or [vertical, horizontal]")?;
            let valid = |v: &Value| v.as_f64().is_some_and(|n| n.is_finite() && n >= 0.0);
            if !(valid(&value) || value.as_array().is_some_and(|v| v.len() == 2 && v.iter().all(valid))) {
                bail!("terminal_padding must contain non-negative numbers");
            }
            value
        }
        "terminal_opacity" => number(0.0, 1.0)?,
        "terminal_line_height" | "terminal_cell_width" => number(0.8, 2.0)?,
        "terminal_scroll_speed" => number(0.1, 5.0)?,
        "window_blur" => match text {
            "true" => json!(true),
            "false" => json!(false),
            _ => bail!("window_blur must be true or false"),
        },
        "titlebar" => match text {
            "system" | "none" => json!(text),
            _ => bail!("titlebar must be system or none"),
        },
        "terminal_menu" | "toast" => {
            if !mods().contains(text) {
                bail!("mod {text:?} is not installed");
            }
            json!(text)
        }
        "link_handlers" => {
            let value: Value = serde_json::from_str(text).context("link_handlers must be a JSON list of mod names")?;
            let available = mods();
            if !value.as_array().is_some_and(|items| items.iter().all(|item| item.as_str().is_some_and(|name| available.contains(name)))) {
                bail!("link_handlers must contain installed mod names");
            }
            value
        }
        _ => bail!("unknown layout field {field:?}"),
    })
}

pub fn set(name: &str, field: &str, text: Option<&str>) -> Result<()> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':']) {
        bail!("invalid layout name");
    }
    if !FIELDS.contains(&field) {
        bail!("unknown layout field {field:?}");
    }
    let path = super::home().join("layouts").join(format!("{name}.json"));
    let _lock = fsutil::write_lock(&path)?;
    if super::shipped().contains(&format!("layout:{name}")) {
        bail!("{name} is shipped; clone it before editing");
    }
    let mut doc = read(&path)?;
    match text {
        Some(text) => {
            doc.insert(field.into(), parse(field, text)?);
        }
        None => {
            doc.shift_remove(field);
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
    fn edits_only_personal_root_and_refuses_invalid_input() {
        let _guard = crate::ui::tests::isolated();
        let home = super::super::home();
        let dir = home.join("layouts");
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(home.join("mods/menu-test")).unwrap();
        fs::create_dir_all(home.join("themes")).unwrap();
        fs::write(home.join("themes/hc-layout-test-theme.json"), "{}").unwrap();
        let installed = home.join("installed.json");
        let previous = fs::read(&installed).ok();
        fs::write(&installed, r#"{"layout:hc-layout-test-shipped":{}}"#).unwrap();
        let path = dir.join("hc-layout-test-personal.json");
        fs::write(&path, r#"{"type":"row","unknown":{"x":1},"children":[{"font":"child"}]}"#).unwrap();
        fs::write(dir.join("hc-layout-test-shipped.json"), "{}").unwrap();
        set("hc-layout-test-personal", "theme", Some(r#"{"light":"paper","dark":"hc-layout-test-theme"}"#)).unwrap();
        set("hc-layout-test-personal", "terminal_menu", Some("menu-test")).unwrap();
        set("hc-layout-test-personal", "font", Some("Mono")).unwrap();
        let doc = read(&path).unwrap();
        assert_eq!(doc["unknown"]["x"], 1);
        assert_eq!(doc["children"][0]["font"], "child");
        assert_eq!(doc["theme"]["dark"], "hc-layout-test-theme");
        assert!(set("hc-layout-test-shipped", "font", Some("Mono")).is_err());
        for (field, value) in [
            ("font_weight", "99"),
            ("terminal_opacity", "1.1"),
            ("terminal_line_height", "2.1"),
            ("terminal_scroll_speed", "0"),
            ("terminal_menu", "missing"),
            ("theme", "missing"),
            ("link_handlers", "[\"missing\"]"),
        ] {
            assert!(set("hc-layout-test-personal", field, Some(value)).is_err());
        }
        set("hc-layout-test-personal", "font", None).unwrap();
        assert!(!read(&path).unwrap().contains_key("font"));
        fs::write(&path, "{broken").unwrap();
        assert!(set("hc-layout-test-personal", "font", Some("Mono")).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{broken");
        match previous {
            Some(bytes) => fs::write(installed, bytes).unwrap(),
            None => fs::remove_file(installed).unwrap(),
        }
        fs::remove_file(path).unwrap();
        fs::remove_file(dir.join("hc-layout-test-shipped.json")).unwrap();
        fs::remove_file(home.join("themes/hc-layout-test-theme.json")).unwrap();
        fs::remove_dir(home.join("mods/menu-test")).unwrap();
    }
}
