use std::path::{Component, Path};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};

use crate::fsutil;

fn read(path: &Path) -> Result<Map<String, Value>> {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str(text.trim_start_matches('\u{feff}')).with_context(|| format!("{} is not valid JSON", path.display()))? {
            Value::Object(doc) => Ok(doc),
            _ => bail!("{} is not a JSON object", path.display()),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
    }
}

pub fn describe() -> Value {
    let path = super::home().join("programs.json");
    let (doc, error) = match read(&path) {
        Ok(doc) => (doc, None),
        Err(e) => (Map::new(), Some(format!("{e:#}"))),
    };
    let programs: Vec<Value> = doc.get("programs").and_then(Value::as_object).map_or_else(Vec::new, |entries| {
        entries
            .iter()
            .map(|(key, fields)| {
                let mut row = fields.as_object().cloned().unwrap_or_default();
                row.insert("key".into(), json!(key));
                Value::Object(row)
            })
            .collect()
    });
    json!({ "path": path, "error": error, "programs": programs, "shells": doc.get("shells").cloned().unwrap_or(json!([])), "interpreters": doc.get("interpreters").cloned().unwrap_or(json!([])) })
}

fn key_valid(key: &str) -> bool {
    !key.is_empty() && key.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"._-".contains(&c))
}

fn split_command(line: &str) -> Result<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some('"'), '\\') if matches!(chars.peek(), Some('"' | '\\')) => {
                word.push(chars.next().unwrap_or_default());
            }
            (Some(q), c) if q == c => quote = None,
            (None, '\'' | '"') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            _ => {
                word.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        bail!("unclosed quote in command line");
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

fn validate(home: &Path, key: &str, value: &Value) -> Result<()> {
    if !key_valid(key) {
        bail!("key must contain only lowercase letters, digits, '.', '_' or '-'");
    }
    let fields = value.as_object().context("program must be an object")?;
    if let Some(name) = fields.get("name")
        && !name.as_str().is_some_and(|s| !s.trim().is_empty())
    {
        bail!("name must be non-empty text");
    }
    for field in ["run", "match"] {
        if let Some(value) = fields.get(field) {
            let values = value.as_array().with_context(|| format!("{field} must be a list of non-empty strings"))?;
            if (field == "run" && values.is_empty()) || values.iter().any(|v| !v.as_str().is_some_and(|s| !s.trim().is_empty())) {
                bail!("{field} must be a list of non-empty strings");
            }
        }
    }
    for field in ["tile", "color"] {
        if let Some(value) = fields.get(field) {
            let valid = value.as_str().and_then(|s| s.strip_prefix('#')).is_some_and(|s| (s.len() == 6 || s.len() == 8) && s.bytes().all(|c| c.is_ascii_hexdigit()));
            if !valid {
                bail!("{field} must be #rrggbb or #rrggbbaa");
            }
        }
    }
    if fields.get("tint").is_some_and(|v| !v.is_boolean()) {
        bail!("tint must be a boolean");
    }
    if let Some(icon) = fields.get("icon") {
        let icon = icon.as_str().context("icon must be a relative path")?;
        let path = Path::new(icon);
        if icon.is_empty() || icon.contains('\\') || icon.contains(':') || !path.components().all(|c| matches!(c, Component::Normal(_))) {
            bail!("icon must be a relative path with forward slashes inside the app home");
        }
        let root = home.canonicalize().context("read app home")?;
        let resolved = home.join(path).canonicalize().context("icon does not exist")?;
        if !resolved.starts_with(root) || !resolved.is_file() {
            bail!("icon must be a file inside the app home");
        }
    }
    Ok(())
}

pub fn set(key: &str, text: &str, run_line: Option<&str>) -> Result<()> {
    let mut entry: Value = serde_json::from_str(text).context("program is not valid JSON")?;
    if let Some(line) = run_line {
        let fields = entry.as_object_mut().context("program must be an object")?;
        if line.trim().is_empty() {
            fields.shift_remove("run");
        } else {
            fields.insert("run".into(), json!(split_command(line)?));
        }
    }
    let home = super::home();
    validate(&home, key, &entry)?;
    change(&home, key, Some(entry))
}

pub fn remove(key: &str) -> Result<()> {
    if !key_valid(key) {
        bail!("invalid program key");
    }
    change(&super::home(), key, None)
}

fn change(home: &Path, key: &str, entry: Option<Value>) -> Result<()> {
    let path = home.join("programs.json");
    let _lock = fsutil::write_lock(&path)?;
    let mut doc = read(&path)?;
    if entry.is_some() && !doc.contains_key("programs") {
        doc.insert("programs".into(), json!({}));
    }
    if let Some(programs) = doc.get_mut("programs") {
        let programs = programs.as_object_mut().context("programs must be an object")?;
        if let Some(entry) = entry {
            programs.insert(key.into(), entry);
        } else {
            programs.shift_remove(key);
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
    fn round_trip_preserves_other_content() {
        let _guard = crate::ui::tests::isolated();
        let home = super::super::home();
        std::fs::create_dir_all(&home).unwrap();
        let path = home.join("programs.json");
        std::fs::write(&path, r#"{"other":{"x":1},"shells":["xonsh"],"programs":{"other":{"future":true}}}"#).unwrap();
        set("mine", r#"{"name":"Mine","future":3}"#, Some("agent --name 'my agent'")).unwrap();
        let doc = read(&path).unwrap();
        assert_eq!(doc["programs"]["mine"]["run"], json!(["agent", "--name", "my agent"]));
        assert_eq!(doc["other"]["x"], 1);
        assert_eq!(doc["shells"], json!(["xonsh"]));
        remove("mine").unwrap();
        assert_eq!(read(&path).unwrap()["programs"], json!({"other":{"future":true}}));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn refuses_invalid_files_without_touching_them() {
        let _guard = crate::ui::tests::isolated();
        let path = super::super::home().join("programs.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{broken").unwrap();
        assert!(set("mine", "{}", None).is_err());
        assert!(remove("mine").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{broken");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn refuses_invalid_fields_and_escaping_icons() {
        let _guard = crate::ui::tests::isolated();
        let home = super::super::home();
        std::fs::create_dir_all(&home).unwrap();
        for (key, item) in [
            ("Bad", json!({})),
            ("", json!({})),
            ("ok", json!({"name":" "})),
            ("ok", json!({"run":[]})),
            ("ok", json!({"run":[""]})),
            ("ok", json!({"match":[""]})),
            ("ok", json!({"tint":"true"})),
            ("ok", json!({"tile":"#abc"})),
            ("ok", json!({"color":"red"})),
            ("ok", json!({"icon":"../outside"})),
            ("ok", json!({"icon":"missing.svg"})),
        ] {
            assert!(validate(&home, key, &item).is_err(), "{key}: {item}");
        }
        assert!(split_command("agent 'unfinished").is_err());
        assert_eq!(split_command(r#"agent "a \"quoted\" value" "C:\\path with space""#).unwrap(), ["agent", "a \"quoted\" value", "C:\\path with space"]);
        std::fs::write(home.join("icon.svg"), "svg").unwrap();
        assert!(validate(&home, "ok", &json!({"icon":"icon.svg","tile":"#aabbccdd"})).is_ok());
        std::fs::remove_file(home.join("icon.svg")).unwrap();
    }
}
