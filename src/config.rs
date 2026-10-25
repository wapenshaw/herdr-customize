//! Edits Herdr's config.toml in place. toml_edit keeps comments, ordering,
//! spacing and line endings of everything an edit does not touch.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result, anyhow, bail};
use toml_edit::{ArrayOfTables, DocumentMut, InlineTable, Item, Table, TableLike};

use crate::{fsutil, herdr};

/// A decoded TOML value: what Go's `any` held.
pub type Value = toml::Value;

/// One key of an array-table entry, in output order.
pub type Field = (String, Value);

/// The location of Herdr's config.toml, with Herdr's precedence:
/// HERDR_CONFIG_PATH, XDG_CONFIG_HOME, %APPDATA% on Windows, then ~/.config.
pub fn path() -> PathBuf {
    if let Some(custom) = env("HERDR_CONFIG_PATH") {
        return custom.into();
    }
    if let Some(xdg) = env("XDG_CONFIG_HOME") {
        return Path::new(&xdg).join("herdr").join("config.toml");
    }
    if cfg!(windows) {
        if let Some(app_data) = env("APPDATA") {
            return Path::new(&app_data).join("herdr").join("config.toml");
        }
        if let Some(profile) = env("USERPROFILE") {
            return Path::new(&profile).join("AppData").join("Roaming").join("herdr").join("config.toml");
        }
    }
    match home() {
        Some(home) => home.join(".config").join("herdr").join("config.toml"),
        None => "config.toml".into(),
    }
}

/// The user's home directory: HOME, or USERPROFILE on Windows.
pub fn home() -> Option<PathBuf> {
    env("HOME").or_else(|| if cfg!(windows) { env("USERPROFILE") } else { None }).map(PathBuf::from)
}

pub fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// Splits a dotted key; quoted parts are not supported.
pub fn split_key(key: &str) -> Vec<String> {
    key.split('.').map(String::from).collect()
}

/// The value at path in a decoded table.
pub fn lookup<'a>(table: &'a toml::Table, path: &[String]) -> Option<&'a Value> {
    let (first, rest) = path.split_first()?;
    let mut current = table.get(first)?;
    for key in rest {
        current = current.as_table()?.get(key)?;
    }
    Some(current)
}

/// Renders a dotted key, quoting parts that are not bare keys.
pub fn render_key(path: &[String]) -> String {
    path.iter().map(|p| if !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') { p.clone() } else { quote(p) }).collect::<Vec<_>>().join(".")
}

/// Renders value as one line of TOML; tables become inline tables.
pub fn encode(value: &Value) -> Result<String> {
    Ok(match value {
        Value::String(s) => quote(s),
        Value::Boolean(b) => b.to_string(),
        Value::Integer(n) => n.to_string(),
        Value::Float(f) if !f.is_finite() => bail!("unsupported number {f}"),
        Value::Float(f) => format!("{f:?}"),
        Value::Datetime(d) => d.to_string(),
        Value::Array(items) => {
            let parts = items.iter().map(encode).collect::<Result<Vec<_>>>()?;
            format!("[{}]", parts.join(", "))
        }
        Value::Table(t) => {
            let fields: Vec<Field> = t.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            encode_fields(&fields)?
        }
    })
}

fn encode_fields(fields: &[Field]) -> Result<String> {
    if fields.is_empty() {
        return Ok("{}".into());
    }
    let parts = fields.iter().map(|(k, v)| Ok(format!("{} = {}", render_key(std::slice::from_ref(k)), encode(v)?))).collect::<Result<Vec<_>>>()?;
    Ok(format!("{{ {} }}", parts.join(", ")))
}

/// A TOML basic string.
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Decodes the text of one TOML value, such as `[1, 2]` or `"x"`.
pub fn decode_value(text: &str) -> Result<Value> {
    let mut table: toml::Table = toml::from_str(&format!("v = {text}"))?;
    table.remove("v").ok_or_else(|| anyhow!("no value"))
}

/// The editable form of a value, with the default inline formatting.
fn edit_value(value: &Value) -> Result<toml_edit::Value> {
    let encoded = encode(value)?;
    let mut v: toml_edit::Value = encoded.parse().map_err(|e| anyhow!("invalid value {encoded}: {e}"))?;
    v.decor_mut().clear();
    Ok(v)
}

/// A parsed config.toml that can be edited without losing formatting.
#[derive(Clone, Default)]
pub struct Doc {
    doc: DocumentMut,
}

impl Doc {
    pub fn parse(text: &str) -> Result<Doc> {
        let doc: DocumentMut = text.parse().map_err(|e| anyhow!("invalid TOML: {e}"))?;
        Ok(Doc { doc })
    }

    pub fn text(&self) -> String {
        self.doc.to_string()
    }

    /// The underlying document, for edits this type has no method for.
    pub fn raw(&mut self) -> &mut DocumentMut {
        &mut self.doc
    }

    /// The decoded document.
    pub fn decoded(&self) -> toml::Table {
        toml::from_str(&self.text()).unwrap_or_default()
    }

    pub fn get(&self, path: &[String]) -> Option<Value> {
        lookup(&self.decoded(), path).cloned()
    }

    /// The length of the prefix of path that already exists.
    fn existing(&self, path: &[String]) -> usize {
        let mut item = self.doc.as_item();
        for (i, key) in path.iter().enumerate() {
            match item.get(key) {
                Some(next) => item = next,
                None => return i,
            }
        }
        path.len()
    }

    /// Comments after the last item belong above anything added after them,
    /// as they would be when appending to the file. Moves them to the prefix
    /// of the standard table at path.
    fn adopt_trailing(&mut self, path: &[String], entry: Option<usize>) {
        let trailing = self.doc.trailing().as_str().unwrap_or("").to_string();
        if trailing.trim().is_empty() {
            return;
        }
        let mut item = self.doc.as_item_mut();
        for key in path {
            match item.get_mut(key) {
                Some(next) => item = next,
                None => return,
            }
        }
        let decor = match (item, entry) {
            (Item::Table(t), None) => t.decor_mut(),
            (Item::ArrayOfTables(aot), Some(i)) => match aot.get_mut(i) {
                Some(t) => t.decor_mut(),
                None => return,
            },
            _ => return,
        };
        let prefix = decor.prefix().and_then(|p| p.as_str()).unwrap_or("").to_string();
        decor.set_prefix(format!("{trailing}{prefix}"));
        self.doc.set_trailing("");
    }

    /// The table-like at path, created as needed when create is set.
    fn table_at(&mut self, path: &[String], create: bool) -> Result<&mut dyn TableLike> {
        let mut table: &mut dyn TableLike = self.doc.as_table_mut();
        let mut inline = false;
        for (i, key) in path.iter().enumerate() {
            if table.get(key).is_none() {
                if !create {
                    bail!("{} is not set", render_key(&path[..=i]));
                }
                let item = if inline {
                    Item::Value(toml_edit::Value::InlineTable(InlineTable::new()))
                } else {
                    let mut t = Table::new();
                    t.set_implicit(true);
                    t.set_dotted(table.is_dotted());
                    Item::Table(t)
                };
                table.insert(key, item);
            }
            let item = table.get_mut(key).expect("inserted above");
            inline = inline || item.is_inline_table();
            table = item.as_table_like_mut().ok_or_else(|| anyhow!("{} is not a table", render_key(&path[..=i])))?;
        }
        Ok(table)
    }

    /// Writes value at path: in place of an existing value, keeping its
    /// comment, or as a new key in the closest table.
    pub fn set(&mut self, path: &[String], value: &Value) -> Result<()> {
        let (last, parents) = path.split_last().ok_or_else(|| anyhow!("empty key path"))?;
        let new = edit_value(value)?;
        let created = self.existing(parents) < parents.len();
        let table = self.table_at(parents, true)?;
        match table.get_mut(last) {
            Some(Item::Value(old)) => {
                let decor = old.decor().clone();
                *old = new;
                *old.decor_mut() = decor;
            }
            Some(Item::None) | None => {
                table.insert(last, Item::Value(new));
            }
            Some(_) => bail!("{} is a table; edit it in the config file", render_key(path)),
        }
        if created {
            self.adopt_trailing(parents, None);
        }
        if lookup(&self.decoded(), path) != Some(value) {
            bail!("cannot place {} in this config; edit it in the config file", render_key(path));
        }
        Ok(())
    }

    /// Removes the key at path and reports whether it was there.
    pub fn unset(&mut self, path: &[String]) -> Result<bool> {
        let (last, parents) = path.split_last().ok_or_else(|| anyhow!("empty key path"))?;
        match self.table_at(parents, false) {
            Ok(table) => Ok(table.remove(last).is_some()),
            Err(_) => Ok(false),
        }
    }

    /// The decoded entries of an array of tables such as keys.command,
    /// whether written as [[headers]] or inline.
    pub fn array_table(&self, path: &[String]) -> Vec<toml::Table> {
        match lookup(&self.decoded(), path) {
            Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_table().cloned()).collect(),
            _ => Vec::new(),
        }
    }

    /// Adds a [[path]] entry after the existing ones.
    pub fn append_array_table(&mut self, path: &[String], fields: &[Field]) -> Result<()> {
        let (last, parents) = path.split_last().ok_or_else(|| anyhow!("empty key path"))?;
        let parent = self.table_at(parents, true)?;
        let entry = match parent.get_mut(last) {
            None | Some(Item::None) => {
                let mut aot = ArrayOfTables::new();
                aot.push(table_of(fields)?);
                parent.insert(last, Item::ArrayOfTables(aot));
                0
            }
            Some(Item::ArrayOfTables(aot)) => {
                aot.push(table_of(fields)?);
                aot.len() - 1
            }
            Some(Item::Value(toml_edit::Value::Array(array))) => {
                let mut entry = InlineTable::new();
                for (k, v) in fields {
                    entry.insert(k, edit_value(v)?);
                }
                array.push(entry);
                return Ok(());
            }
            Some(_) => bail!("{} is not an array of tables", render_key(path)),
        };
        self.adopt_trailing(path, Some(entry));
        Ok(())
    }

    /// Rewrites entry i of [[path]] to exactly fields. Existing keys keep
    /// their position and trailing comments; missing keys are removed.
    pub fn update_array_table(&mut self, path: &[String], i: usize, fields: &[Field]) -> Result<()> {
        let entry = self.array_entry(path, i)?;
        let stale: Vec<String> = entry.iter().map(|(k, _)| k.to_string()).filter(|k| !fields.iter().any(|(f, _)| f == k)).collect();
        for key in stale {
            entry.remove(&key);
        }
        for (key, value) in fields {
            let new = edit_value(value)?;
            match entry.get_mut(key) {
                Some(Item::Value(old)) => {
                    let decor = old.decor().clone();
                    *old = new;
                    *old.decor_mut() = decor;
                }
                _ => {
                    entry.insert(key, Item::Value(new));
                }
            }
        }
        Ok(())
    }

    /// Removes entry i of [[path]].
    pub fn delete_array_table(&mut self, path: &[String], i: usize) -> Result<()> {
        let (last, parents) = path.split_last().ok_or_else(|| anyhow!("empty key path"))?;
        let missing = || anyhow!("{} entry {} does not exist", render_key(path), i + 1);
        let parent = self.table_at(parents, false)?;
        match parent.get_mut(last) {
            Some(Item::ArrayOfTables(aot)) if i < aot.len() => aot.remove(i),
            Some(Item::Value(toml_edit::Value::Array(array))) if i < array.len() => {
                array.remove(i);
            }
            _ => return Err(missing()),
        }
        Ok(())
    }

    fn array_entry(&mut self, path: &[String], i: usize) -> Result<&mut dyn TableLike> {
        let (last, parents) = path.split_last().ok_or_else(|| anyhow!("empty key path"))?;
        let name = render_key(path);
        let parent = self.table_at(parents, false)?;
        let entry: Option<&mut dyn TableLike> = match parent.get_mut(last) {
            Some(Item::ArrayOfTables(aot)) => aot.get_mut(i).map(|t| t as &mut dyn TableLike),
            Some(Item::Value(toml_edit::Value::Array(array))) => array.get_mut(i).and_then(|v| v.as_inline_table_mut()).map(|t| t as &mut dyn TableLike),
            _ => None,
        };
        entry.ok_or_else(|| anyhow!("{name} entry {} does not exist", i + 1))
    }
}

fn table_of(fields: &[Field]) -> Result<Table> {
    let mut t = Table::new();
    for (k, v) in fields {
        t.insert(k, Item::Value(edit_value(v)?));
    }
    Ok(t)
}

/// What a save did besides writing the file.
#[derive(Debug, Default, Clone)]
pub struct SaveResult {
    pub changed: bool,
    /// Set when 'herdr config check' could not run.
    pub check_skipped: String,
    /// Set when the config was saved but Herdr did not reload.
    pub reload_err: Option<String>,
}

/// Problems Herdr found in an edit. The file is unchanged.
#[derive(Debug)]
pub struct CheckError {
    pub issues: Vec<String>,
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "herdr rejected the change: {}", self.issues.join("; "))
    }
}

impl std::error::Error for CheckError {}

/// Keeps saves in order, so one cannot load the file before another has
/// written it and undo it.
pub static SAVING: Mutex<()> = Mutex::new(());

/// Loads and saves one config file.
#[derive(Debug, Clone)]
pub struct Store {
    pub path: PathBuf,
    /// Skips 'herdr server reload-config' after a save.
    pub no_reload: bool,
}

impl Store {
    pub fn default_store() -> Store {
        Store { path: path(), no_reload: false }
    }

    /// Parses the config. A missing file is an empty document.
    pub fn load(&self) -> Result<Doc> {
        match fs::read_to_string(&self.path) {
            Ok(text) => Doc::parse(&text).with_context(|| self.path.display().to_string()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Doc::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Loads the config, applies edit, and saves the result.
    pub fn update(&self, edit: impl FnOnce(&mut Doc) -> Result<()>) -> Result<SaveResult> {
        let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
        let mut doc = self.load()?;
        edit(&mut doc)?;
        self.save(&doc)
    }

    /// Validates doc with 'herdr config check' before replacing the file, so
    /// a rejected edit never reaches disk. Problems the config already had do
    /// not block a save. The first change each day backs up the original.
    pub fn save(&self, doc: &Doc) -> Result<SaveResult> {
        let mut result = SaveResult::default();
        let data = doc.text();
        let original = match fs::read(&self.path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        if original.as_deref() == Some(data.as_bytes()) {
            return Ok(result);
        }
        Doc::parse(&data)?;
        let dir = self.path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir)?;
        let (issues, skipped) = self.new_issues(dir, &data, original.is_none())?;
        if !issues.is_empty() {
            return Err(CheckError { issues }.into());
        }
        result.check_skipped = skipped;
        if let Some(original) = &original {
            fsutil::daily_backup(&self.path, original)?;
        }
        fsutil::atomic_write(&self.path, data.as_bytes(), 0o600)?;
        result.changed = true;
        if !self.no_reload {
            result.reload_err = herdr::reload().err().map(|e| e.to_string());
        }
        Ok(result)
    }

    /// Checks data in a temporary file beside the config, so relative paths
    /// resolve the same way, and returns issues the current file lacks.
    fn new_issues(&self, dir: &Path, data: &str, missing: bool) -> Result<(Vec<String>, String)> {
        let tmp = fsutil::temp_path(dir, ".herdr-customize-check-").with_extension("toml");
        fs::write(&tmp, data)?;
        let after = herdr::check_config(&tmp);
        let _ = fs::remove_file(&tmp);
        if !after.ran {
            return Ok((Vec::new(), after.skipped));
        }
        if after.issues.is_empty() {
            return Ok((Vec::new(), String::new()));
        }
        let known: Vec<String> = if missing { Vec::new() } else { herdr::check_config(&self.path).issues.iter().map(|i| normalize_issue(i)).collect() };
        let fresh = after.issues.iter().filter(|i| !known.contains(&normalize_issue(i))).map(|i| i.trim().to_string()).collect();
        Ok((fresh, String::new()))
    }
}

/// Ignores line and column numbers, which an edit can shift.
fn normalize_issue(issue: &str) -> String {
    let mut out = String::new();
    let mut digits = false;
    for c in issue.trim().chars() {
        if c.is_ascii_digit() {
            if !digits {
                out.push('#');
            }
            digits = true;
        } else {
            digits = false;
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn additions_follow_trailing_comments() {
        let mut d = Doc::parse("# my config\n").unwrap();
        d.append_array_table(&split_key("keys.command"), &[("key".into(), Value::String("prefix+g".into()))]).unwrap();
        assert_eq!(d.text(), "# my config\n[[keys.command]]\nkey = \"prefix+g\"\n");
        let mut d = Doc::parse("a = 1\n\n# the end\n").unwrap();
        d.set(&split_key("ui.x"), &Value::Integer(1)).unwrap();
        assert_eq!(d.text(), "a = 1\n\n# the end\n[ui]\nx = 1\n");
        let mut d = Doc::parse("[ui]\nx = 1\n# the end\n").unwrap();
        d.set(&split_key("ui.y"), &Value::Integer(2)).unwrap();
        assert_eq!(d.text(), "[ui]\nx = 1\ny = 2\n# the end\n");
    }

    fn p(key: &str) -> Vec<String> {
        split_key(key)
    }

    #[test]
    fn set_replaces_in_place_and_keeps_comments() {
        let mut doc = Doc::parse("# top\n[ui]\nsidebar_width = 30 # wide\nother = true\n").unwrap();
        doc.set(&p("ui.sidebar_width"), &Value::Integer(40)).unwrap();
        assert_eq!(doc.text(), "# top\n[ui]\nsidebar_width = 40 # wide\nother = true\n");
    }

    #[test]
    fn set_inserts_into_existing_table_and_creates_new_ones() {
        let mut doc = Doc::parse("[ui]\na = 1\n\n[keys]\nzoom = \"prefix+z\"\n").unwrap();
        doc.set(&p("ui.b"), &Value::Boolean(true)).unwrap();
        doc.set(&p("server.headless_cols"), &Value::Integer(100)).unwrap();
        let text = doc.text();
        assert!(text.starts_with("[ui]\na = 1\nb = true\n"), "{text}");
        assert!(text.contains("[server]\nheadless_cols = 100\n"), "{text}");
        assert_eq!(doc.get(&p("ui.b")), Some(Value::Boolean(true)));
    }

    #[test]
    fn set_edits_inline_and_dotted_tables() {
        let mut doc = Doc::parse("ui = { a = 1 }\nterminal.shell = \"zsh\"\n").unwrap();
        doc.set(&p("ui.b"), &Value::Integer(2)).unwrap();
        doc.set(&p("terminal.login"), &Value::Boolean(false)).unwrap();
        assert_eq!(doc.get(&p("ui.b")), Some(Value::Integer(2)));
        assert_eq!(doc.get(&p("terminal.login")), Some(Value::Boolean(false)));
        assert!(doc.text().contains("terminal.login = false"), "{}", doc.text());
    }

    #[test]
    fn unset_removes_only_the_key() {
        let mut doc = Doc::parse("[ui]\na = 1 # note\nb = 2\n").unwrap();
        assert!(doc.unset(&p("ui.a")).unwrap());
        assert!(!doc.unset(&p("ui.missing")).unwrap());
        assert!(!doc.unset(&p("nope.x")).unwrap());
        assert_eq!(doc.text(), "[ui]\nb = 2\n");
    }

    #[test]
    fn array_tables_append_update_delete() {
        let mut doc = Doc::parse("[keys]\nzoom = \"prefix+z\"\n\n[[keys.command]]\nkey = \"prefix+g\" # git\ntype = \"popup\"\ncommand = \"lazygit\"\nwidth = 80\n").unwrap();
        let path = p("keys.command");
        doc.append_array_table(&path, &[("key".into(), Value::String("prefix+h".into())), ("type".into(), Value::String("shell".into())), ("command".into(), Value::String("htop".into()))]).unwrap();
        assert_eq!(doc.array_table(&path).len(), 2);
        doc.update_array_table(&path, 0, &[("key".into(), Value::String("prefix+G".into())), ("type".into(), Value::String("popup".into())), ("command".into(), Value::String("lazygit".into()))])
            .unwrap();
        let text = doc.text();
        assert!(text.contains("key = \"prefix+G\" # git"), "{text}");
        assert!(!text.contains("width"), "{text}");
        doc.delete_array_table(&path, 0).unwrap();
        let entries = doc.array_table(&path);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["command"].as_str(), Some("htop"));
        Doc::parse(&doc.text()).unwrap();
    }

    #[test]
    fn inline_array_tables() {
        let mut doc = Doc::parse("keys.command = [{ key = \"prefix+g\", type = \"shell\", command = \"x\" }]\n").unwrap();
        let path = p("keys.command");
        doc.append_array_table(&path, &[("key".into(), Value::String("prefix+y".into()))]).unwrap();
        doc.update_array_table(&path, 0, &[("key".into(), Value::String("prefix+z".into()))]).unwrap();
        let entries = doc.array_table(&path);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].len(), 1);
        doc.delete_array_table(&path, 1).unwrap();
        assert_eq!(doc.array_table(&path).len(), 1);
    }

    #[test]
    fn encode_values() {
        assert_eq!(encode(&Value::Float(2.0)).unwrap(), "2.0");
        assert_eq!(encode(&Value::String("a\"b\n".into())).unwrap(), "\"a\\\"b\\n\"");
        let v = decode_value("[1, \"x\", { b = 2, a = true }]").unwrap();
        assert_eq!(encode(&v).unwrap(), "[1, \"x\", { a = true, b = 2 }]");
        assert_eq!(render_key(&["keys".into(), "a b".into()]), "keys.\"a b\"");
    }

    #[test]
    fn normalize_ignores_positions() {
        assert_eq!(normalize_issue(" line 12, col 4: bad "), normalize_issue("line 3, col 40: bad"));
    }
}
