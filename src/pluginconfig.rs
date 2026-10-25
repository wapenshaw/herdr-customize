//! The files a plugin keeps in its Herdr config directory. Types come from
//! the plugin's config-schema.json, or else from the values and commented-out
//! defaults such as "#POLL_MS=500", since Herdr sets no format.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::config::{self, Doc, Value, split_key};
use crate::fsutil;
use crate::schema::{self, Key, Reference};

/// What a plugin ships in its root to describe its config, in the format of
/// Herdr's config-reference.json plus an optional "file" naming the config
/// file (config.toml when absent).
pub const SCHEMA_FILE: &str = "config-schema.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Toml,
    Env,
    Other,
}

/// One file in a plugin's config directory.
#[derive(Debug, Clone)]
pub struct File {
    pub name: String,
    pub path: PathBuf,
    pub format: Format,
    /// Described by the schema but not created yet.
    pub missing: bool,
    pub entries: Vec<Entry>,
}

/// One setting in a file.
#[derive(Debug, Clone, Default)]
pub struct Entry {
    /// The name (a dotted path in TOML), type, default and description,
    /// documented by a schema or inferred.
    pub key: Key,
    pub value: Option<Value>,
    /// From the plugin's schema.
    pub documented: bool,
    /// Cannot be edited here, such as arrays of tables.
    pub read_only: bool,
}

impl Entry {
    pub fn set(&self) -> bool {
        self.value.is_some()
    }
}

/// A plugin's config directory.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub dir: PathBuf,
    pub files: Vec<File>,
    pub schema: Option<Reference>,
    pub schema_err: Option<String>,
}

/// Reads every file in dir, using root's schema when present.
pub fn load(dir: &Path, root: &Path) -> Result<Config> {
    let mut c = Config { dir: dir.to_path_buf(), ..Default::default() };
    match fs::read_to_string(root.join(SCHEMA_FILE)) {
        Ok(data) => match schema::parse(&data) {
            Ok(s) => c.schema = Some(s),
            Err(e) => c.schema_err = Some(e.to_string()),
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => c.schema_err = Some(e.to_string()),
    }
    match fs::read_dir(dir) {
        Ok(entries) => {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if (name.starts_with('.') && name != ".env") || name.contains(".bak-") || name.ends_with(".lock") {
                    continue;
                }
                let path = dir.join(&name);
                // Follow symlinks: a plugin may link its config from elsewhere.
                if path.is_file() {
                    c.files.push(File { format: format_of(&name), name, path, missing: false, entries: Vec::new() });
                }
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    if c.schema.is_some() {
        let name = c.schema_file();
        if !c.files.iter().any(|f| f.name == name) {
            c.files.push(File { format: format_of(&name), path: dir.join(&name), name, missing: true, entries: Vec::new() });
        }
    }
    // Editable files first, in name order.
    c.files.sort_by(|a, b| (a.format == Format::Other).cmp(&(b.format == Format::Other)).then(a.name.cmp(&b.name)));
    let documented: Vec<Vec<Key>> = c.files.iter().map(|f| c.documented(f)).collect();
    for (f, keys) in c.files.iter_mut().zip(documented) {
        read(f, keys).with_context(|| f.name.clone())?;
    }
    Ok(c)
}

fn format_of(name: &str) -> Format {
    // A leading dot starts a name, not an extension, so .env has none.
    if name == ".env" {
        return Format::Env;
    }
    match Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref() {
        Some("toml") => Format::Toml,
        Some("env") => Format::Env,
        _ => Format::Other,
    }
}

impl Config {
    fn schema_file(&self) -> String {
        match &self.schema {
            Some(s) if !s.file.is_empty() => s.file.clone(),
            _ => "config.toml".into(),
        }
    }

    /// The schema keys for f, if the schema describes it.
    fn documented(&self, f: &File) -> Vec<Key> {
        match &self.schema {
            Some(s) if f.name == self.schema_file() => s.sections.iter().flat_map(|s| s.keys.clone()).collect(),
            _ => Vec::new(),
        }
    }
}

fn read(f: &mut File, documented: Vec<Key>) -> Result<()> {
    // Decided before the read: a binary file in the config directory must
    // not fail the whole screen.
    let parse: fn(&str) -> Result<Vec<Entry>> = match f.format {
        Format::Toml => read_toml,
        Format::Env => |data| Ok(read_env(data)),
        Format::Other => return Ok(()),
    };
    let data = if f.missing { String::new() } else { fs::read_to_string(&f.path)? };
    f.entries = merge(documented, parse(&data)?, f.format);
    Ok(())
}

/// Documented keys first, filled with the values found, then any other keys
/// found in the file. A key found twice keeps its last value.
fn merge(documented: Vec<Key>, mut found: Vec<Entry>, format: Format) -> Vec<Entry> {
    let mut out = Vec::new();
    for k in documented {
        let mut e = Entry { documented: true, ..Default::default() };
        if let Some(i) = found.iter().rposition(|f| f.key.key == k.key) {
            let f = found.remove(i);
            found.retain(|f| f.key.key != k.key);
            e.value = f.value.map(|v| if format == Format::Env { typed(&k.kind, &env_string(&v)) } else { v });
            e.read_only = f.read_only;
        }
        e.key = k;
        out.push(e);
    }
    out.extend(found);
    out
}

/// A schema type inferred from a decoded value.
fn type_of(value: &Value) -> &'static str {
    match value {
        Value::Boolean(_) => schema::BOOLEAN,
        Value::Integer(_) => schema::INTEGER,
        Value::Float(_) => schema::FLOAT,
        Value::String(_) => schema::STRING,
        _ => schema::ARRAY,
    }
}

impl File {
    /// Writes key, creating the file when needed. Load again to see it.
    pub fn set(&self, key: &str, value: &Value) -> Result<()> {
        let format = self.format;
        self.edit(|data| match format {
            Format::Env => set_env(data, key, &env_string(value)),
            _ => {
                let mut doc = Doc::parse(data)?;
                doc.set(&split_key(key), value)?;
                Ok(doc.text())
            }
        })
    }

    /// Removes key, so the plugin's default applies.
    pub fn unset(&self, key: &str) -> Result<()> {
        let format = self.format;
        self.edit(|data| match format {
            Format::Env => Ok(unset_env(data, key)),
            _ => {
                let mut doc = Doc::parse(data)?;
                doc.unset(&split_key(key))?;
                Ok(doc.text())
            }
        })
    }

    fn edit(&self, change: impl FnOnce(&str) -> Result<String>) -> Result<()> {
        if self.format == Format::Other {
            bail!("{} is not a TOML or .env file", self.name);
        }
        let _lock = fsutil::write_lock(&self.path)?;
        let old = match fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.into()),
        };
        let data = change(&old)?;
        if data == old {
            return Ok(());
        }
        fsutil::write_backed_up(&self.path, data.as_bytes(), 0o644)
    }
}

/* ---------------------------------------------------------------- TOML */

static TOML_HEADER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\[\s*([A-Za-z0-9_.-]+)\s*\]\s*(#.*)?$").unwrap());
static TOML_ARRAY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\[\[").unwrap());
static TOML_KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*([A-Za-z0-9_.-]+)\s*=").unwrap());
static NOTE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*#\s?(.*)$").unwrap());

/// The keys in file order, then keys that only appear as a commented-out
/// default ("# key = value"), described by the comment above them.
fn read_toml(data: &str) -> Result<Vec<Entry>> {
    let decoded = Doc::parse(data)?.decoded();
    let mut out: Vec<Entry> = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |e: Entry, out: &mut Vec<Entry>| {
        if seen.insert(e.key.key.clone()) {
            out.push(e);
        }
    };
    let mut table = String::new();
    let mut note: Vec<String> = Vec::new();
    let mut defaults: HashMap<String, Entry> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for line in data.lines() {
        if let Some(m) = TOML_HEADER.captures(line) {
            table = m[1].to_string();
            note.clear();
        } else if TOML_ARRAY.is_match(line) {
            // Keys inside an array of tables are not followed.
            table = "\0".into();
            note.clear();
        } else if let Some(m) = TOML_KEY.captures(line) {
            let path = join(&table, &m[1]);
            if let Some(value) = config::lookup(&decoded, &split_key(&path)).filter(|v| !v.is_table()) {
                add(set_entry(&path, value, defaults.get(&path)), &mut out);
            }
            note.clear();
        } else if let Some(m) = NOTE.captures(line) {
            let text = m[1].to_string();
            if let Some((k, v)) = commented_toml(&text) {
                let path = join(&table, &k);
                let default = text[text.find('=').unwrap() + 1..].trim().to_string();
                defaults.insert(path.clone(), Entry { key: Key { key: path.clone(), kind: type_of(&v).into(), default, description: note.join(" "), values: Vec::new() }, ..Default::default() });
                order.push(path);
                note.clear();
            } else if !text.trim().is_empty() {
                note.push(text.trim().to_string());
            }
        } else {
            note.clear();
        }
    }
    // Keys written in forms the scan does not follow, such as inline tables.
    for path in leaves(&decoded, &[]) {
        if let Some(value) = config::lookup(&decoded, &split_key(&path)) {
            add(set_entry(&path, value, defaults.get(&path)), &mut out);
        }
    }
    // Commented-out defaults, in file order, for keys that are not set.
    for path in order {
        if let Some(def) = defaults.remove(&path) {
            add(def, &mut out);
        }
    }
    Ok(out)
}

fn set_entry(path: &str, value: &Value, def: Option<&Entry>) -> Entry {
    let mut e = Entry { key: Key { key: path.into(), kind: type_of(value).into(), ..Default::default() }, value: Some(value.clone()), ..Default::default() };
    if let Some(def) = def {
        e.key.default = def.key.default.clone();
        e.key.description = def.key.description.clone();
    }
    if let Value::Array(items) = value {
        e.read_only = items.first().is_some_and(Value::is_table);
    }
    e
}

/// "key = value" from a comment's text.
fn commented_toml(text: &str) -> Option<(String, Value)> {
    let key = TOML_KEY.captures(text)?[1].to_string();
    let doc: toml::Table = toml::from_str(text).ok()?;
    let value = config::lookup(&doc, &split_key(&key))?.clone();
    (!value.is_table()).then_some((key, value))
}

fn leaves(table: &toml::Table, prefix: &[String]) -> Vec<String> {
    let mut names: Vec<&String> = table.keys().collect();
    names.sort();
    let mut out = Vec::new();
    for name in names {
        let mut path = prefix.to_vec();
        path.push(name.clone());
        match &table[name] {
            Value::Table(sub) => out.extend(leaves(sub, &path)),
            _ => out.push(config::render_key(&path)),
        }
    }
    out
}

fn join(table: &str, key: &str) -> String {
    if table.is_empty() { key.into() } else { format!("{table}.{key}") }
}

/* ----------------------------------------------------------------- env */

static ENV_SET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(export\s+)?([A-Za-z_][A-Za-z0-9_]*)=(.*)$").unwrap());
static ENV_COMMENTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*#\s*(?:export\s+)?([A-Z_][A-Z0-9_]*)=(.*)$").unwrap());

/// KEY=value lines and commented-out "#KEY=default" lines, with the comment
/// lines above each as its description.
fn read_env(data: &str) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    let mut raw: Vec<Option<String>> = Vec::new();
    let mut note: Vec<String> = Vec::new();
    let index = |out: &mut Vec<Entry>, raw: &mut Vec<Option<String>>, name: &str| -> usize {
        if let Some(i) = out.iter().position(|e| e.key.key == name) {
            return i;
        }
        out.push(Entry { key: Key { key: name.into(), ..Default::default() }, ..Default::default() });
        raw.push(None);
        out.len() - 1
    };
    for line in data.lines() {
        if let Some(m) = ENV_COMMENTED.captures(line) {
            let def = unquote(&m[2]);
            let i = index(&mut out, &mut raw, &m[1]);
            let e = &mut out[i];
            e.key.kind = infer_env(&def).into();
            e.key.default = config::encode(&typed(&e.key.kind, &def)).unwrap_or_default();
            e.key.description = note.join(" ");
            note.clear();
        } else if let Some(m) = ENV_SET.captures(line) {
            let i = index(&mut out, &mut raw, &m[2]);
            raw[i] = Some(unquote(&m[3]));
            if out[i].key.description.is_empty() {
                out[i].key.description = note.join(" ");
            }
            note.clear();
        } else if let Some(m) = NOTE.captures(line) {
            let text = m[1].trim();
            if !text.is_empty() {
                note.push(text.into());
            }
        } else {
            note.clear();
        }
    }
    for (e, raw) in out.iter_mut().zip(raw) {
        if e.key.kind.is_empty() {
            e.key.kind = infer_env(raw.as_deref().unwrap_or_default()).into();
        }
        e.value = raw.map(|text| typed(&e.key.kind, &text));
    }
    out
}

fn infer_env(text: &str) -> &'static str {
    if text == "true" || text == "false" {
        schema::BOOLEAN
    } else if !text.is_empty() && text.strip_prefix('-').unwrap_or(text).chars().all(|c| c.is_ascii_digit()) && text != "-" {
        schema::INTEGER
    } else {
        schema::STRING
    }
}

/// Converts env text to the value its type implies.
pub fn typed(kind: &str, text: &str) -> Value {
    match kind {
        schema::BOOLEAN => text.parse().map(Value::Boolean).ok(),
        schema::INTEGER => text.parse().map(Value::Integer).ok(),
        schema::FLOAT => text.parse().map(Value::Float).ok(),
        _ => None,
    }
    .unwrap_or_else(|| Value::String(text.into()))
}

fn env_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => config::encode(other).unwrap_or_default(),
    }
}

/// The quote character wrapping s, if any.
fn quote_of(s: &str) -> Option<char> {
    let b = s.as_bytes();
    (b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0]).then(|| b[0] as char)
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    match quote_of(s) {
        Some(_) => s[1..s.len() - 1].into(),
        None => s.into(),
    }
}

fn newline(data: &str) -> &'static str {
    if data.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Writes KEY=value: in place of the last existing line, the one that takes
/// effect (keeping its quotes), else just below the commented-out default,
/// else at the end.
fn set_env(data: &str, key: &str, value: &str) -> Result<String> {
    if value.contains(['\r', '\n']) {
        bail!("a value cannot span lines");
    }
    let nl = newline(data);
    let normalized = data.replace("\r\n", "\n");
    let mut lines: Vec<String> = normalized.split('\n').map(String::from).collect();
    if let Some(line) = lines.iter_mut().rev().find(|line| ENV_SET.captures(line).is_some_and(|m| &m[2] == key)) {
        let old = ENV_SET.captures(line).unwrap()[3].trim().to_string();
        let quoted = match quote_of(&old) {
            Some(q) => format!("{q}{value}{q}"),
            None => value.to_string(),
        };
        *line = format!("{}{quoted}", &line[..=line.find('=').unwrap()]);
        return Ok(lines.join(nl));
    }
    let commented = lines.iter().position(|line| ENV_COMMENTED.captures(line).is_some_and(|m| &m[1] == key));
    let entry = format!("{key}={value}");
    if let Some(i) = commented {
        lines.insert(i + 1, entry);
        return Ok(lines.join(nl));
    }
    let mut text = lines.join(nl);
    if !text.is_empty() && !text.ends_with(nl) {
        text += nl;
    }
    Ok(format!("{text}{entry}{nl}"))
}

/// Removes KEY=value lines, leaving commented-out defaults.
fn unset_env(data: &str, key: &str) -> String {
    let nl = newline(data);
    data.replace("\r\n", "\n").split('\n').filter(|line| ENV_SET.captures(line).is_none_or(|m| &m[2] != key)).collect::<Vec<_>>().join(nl)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_files_infer_types_and_defaults() {
        let entries = read_env("# How often to poll.\n#POLL_MS=500\nNAME=\"bob\"\n# Turn it on\nENABLED=true\n");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].key.key, "POLL_MS");
        assert_eq!(entries[0].key.kind, schema::INTEGER);
        assert_eq!(entries[0].key.default, "500");
        assert_eq!(entries[0].key.description, "How often to poll.");
        assert!(!entries[0].set());
        assert_eq!(entries[1].value, Some(Value::String("bob".into())));
        assert_eq!(entries[2].value, Some(Value::Boolean(true)));
        assert_eq!(entries[2].key.description, "Turn it on");
    }

    #[test]
    fn env_edits_keep_quotes_and_placement() {
        let data = "A=\"x\"\n#B=1\nC=3\n";
        assert_eq!(set_env(data, "A", "y").unwrap(), "A=\"y\"\n#B=1\nC=3\n");
        assert_eq!(set_env(data, "B", "2").unwrap(), "A=\"x\"\n#B=1\nB=2\nC=3\n");
        assert_eq!(set_env(data, "D", "4").unwrap(), "A=\"x\"\n#B=1\nC=3\nD=4\n");
        assert_eq!(unset_env(data, "C"), "A=\"x\"\n#B=1\n");
        assert!(set_env(data, "A", "a\nb").is_err());
    }

    // The reader takes the last of duplicate assignments, so that is the one
    // an edit must change.
    #[test]
    fn env_edits_change_the_effective_duplicate() {
        let data = set_env("A=1\nA=2\n", "A", "3").unwrap();
        assert_eq!(data, "A=1\nA=3\n");
        assert_eq!(read_env(&data)[0].value, Some(Value::Integer(3)));
    }

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hc-pluginconfig-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn dot_env_is_an_env_file() {
        let dir = tempdir("dotenv");
        fs::write(dir.join(".env"), "A=1\n").unwrap();
        fs::write(dir.join(".hidden"), "B=2\n").unwrap();
        let c = load(&dir, &dir).unwrap();
        assert_eq!(c.files.len(), 1);
        assert_eq!((c.files[0].name.as_str(), c.files[0].format), (".env", Format::Env));
        assert_eq!(c.files[0].entries[0].value, Some(Value::Integer(1)));
    }

    // Another process holds the same lock while it edits; an edit here must
    // wait rather than load the file under it.
    #[test]
    fn edits_wait_for_the_file_lock() {
        let _env = crate::ui::tests::isolated();
        let dir = tempdir("lock");
        let path = dir.join("a.env");
        let f = File { name: "a.env".into(), path: path.clone(), format: Format::Env, missing: true, entries: Vec::new() };
        let held = fsutil::write_lock(&path).unwrap();
        let edit = std::thread::spawn(move || f.set("A", &Value::Integer(1)));
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(!path.exists());
        drop(held);
        edit.join().unwrap().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "A=1\n");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "the lock stays out of the plugin's config folder");
    }

    #[test]
    fn quoted_toml_keys_stay_whole() {
        let entries = read_toml("\"a.b\" = 1\n").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].value, Some(Value::Integer(1)));
        let mut doc = Doc::parse("\"a.b\" = 1\n").unwrap();
        doc.set(&split_key(&entries[0].key.key), &Value::Integer(2)).unwrap();
        assert_eq!(doc.text(), "\"a.b\" = 2\n");
    }

    #[test]
    fn toml_files_list_set_and_commented_keys() {
        let data = "# Theme name\ntheme = \"dark\"\n\n[poll]\n# Milliseconds between polls\n# interval = 500\nretries = 3\nnested = { a = 1 }\n[[hooks]]\ncmd = \"x\"\n";
        let entries = read_toml(data).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.key.key.as_str()).collect();
        assert_eq!(names, ["theme", "poll.retries", "hooks", "poll.nested.a", "poll.interval"]);
        let interval = entries.iter().find(|e| e.key.key == "poll.interval").unwrap();
        assert_eq!(interval.key.default, "500");
        assert_eq!(interval.key.description, "Milliseconds between polls");
        assert!(entries.iter().find(|e| e.key.key == "hooks").unwrap().read_only);
    }

    #[test]
    fn schema_keys_come_first() {
        let documented = vec![Key { key: "POLL_MS".into(), kind: schema::INTEGER.into(), ..Default::default() }];
        let merged = merge(documented, read_env("OTHER=1\nPOLL_MS=250\n"), Format::Env);
        assert_eq!(merged[0].key.key, "POLL_MS");
        assert!(merged[0].documented);
        assert_eq!(merged[0].value, Some(Value::Integer(250)));
        assert_eq!(merged[1].key.key, "OTHER");
    }

    #[test]
    fn load_reads_a_directory() {
        let _env = crate::ui::tests::isolated();
        let dir = std::env::temp_dir().join(format!("hc-pluginconfig-{}", std::process::id()));
        let root = dir.join("root");
        let cfg = dir.join("cfg");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&cfg).unwrap();
        fs::write(cfg.join("settings.env"), "A=1\n").unwrap();
        fs::write(cfg.join("notes.txt"), "hi").unwrap();
        // A binary file is listed but never read, so it cannot fail the load.
        fs::write(cfg.join("icon.png"), [0x89, b'P', b'N', b'G', 0xff, 0xfe]).unwrap();
        fs::write(cfg.join("settings.env.bak-20250101"), "A=0\n").unwrap();
        fs::write(root.join(SCHEMA_FILE), r#"{"file":"config.toml","sections":[{"id":"x","title":"X","keys":[{"key":"poll","type":"integer","default":"5"}]}]}"#).unwrap();
        let c = load(&cfg, &root).unwrap();
        let names: Vec<&str> = c.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["config.toml", "settings.env", "icon.png", "notes.txt"]);
        assert!(c.files[0].missing);
        assert_eq!(c.files[0].entries[0].key.key, "poll");
        c.files[0].set("poll", &Value::Integer(9)).unwrap();
        c.files[1].set("A", &Value::Integer(2)).unwrap();
        let c = load(&cfg, &root).unwrap();
        assert_eq!(c.files[0].entries[0].value, Some(Value::Integer(9)));
        assert_eq!(c.files[1].entries[0].value, Some(Value::Integer(2)));
        c.files[1].unset("A").unwrap();
        assert_eq!(fs::read_to_string(cfg.join("settings.env")).unwrap(), "");
    }
}
