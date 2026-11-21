use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use super::{check_mod, digest, home, keys, read_record, state_dir, write_record};
use crate::fsutil;

const RECORD: &str = "popups.json";
const GENERATED: &[&str] = &["gpui-kit.d.ts", "herdr.d.ts", "jsconfig.json"];
const TEMPLATE: &[(&str, &str)] = &[
    ("main.js", include_str!("../../app/popup-template/main.js")),
    ("mod.json", include_str!("../../app/popup-template/mod.json")),
    ("RECIPE.md", include_str!("../../app/popup-template/RECIPE.md")),
];

#[derive(Clone, Serialize, Deserialize)]
pub struct Popup {
    pub name: String,
    pub command: String,
    pub argv: Vec<String>,
    pub width: f64,
    pub height: f64,
    pub key: String,
    pub persistent: bool,
    files: BTreeMap<String, String>,
}

type Record = BTreeMap<String, Popup>;

#[derive(Serialize)]
pub struct Description {
    #[serde(flatten)]
    popup: Popup,
    changed: bool,
}

#[derive(Default, Serialize)]
pub struct Report {
    kept: Vec<String>,
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn split_command(text: &str) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let mut arg = String::new();
    let mut chars = text.chars().peekable();
    let mut quote = None;
    let mut started = false;
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek().is_some_and(|c| *c == '\'' || *c == '"') {
            arg.push(chars.next().unwrap());
            started = true;
        } else if c == '\'' || c == '"' {
            if quote == Some(c) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(c);
            } else {
                arg.push(c);
            }
            started = true;
        } else if c.is_whitespace() && quote.is_none() {
            if started {
                args.push(std::mem::take(&mut arg));
                started = false;
            }
        } else {
            arg.push(c);
            started = true;
        }
    }
    if quote.is_some() {
        bail!("unclosed quote in command");
    }
    if started {
        args.push(arg);
    }
    if args.first().is_none_or(String::is_empty) {
        bail!("command is required");
    }
    Ok(args)
}

fn untouched(dir: &Path, files: &BTreeMap<String, String>) -> Result<bool> {
    if !fs::symlink_metadata(dir)?.file_type().is_dir() {
        return Ok(false);
    }
    for (name, expected) in files {
        match fs::read(dir.join(name)) {
            Ok(bytes) if digest(&bytes) == *expected => {}
            Ok(_) => return Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        }
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.file_type()?.is_file() || (!files.contains_key(&name) && !GENERATED.contains(&name.as_str())) {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn describe() -> Result<Vec<Description>> {
    let record: Record = read_record(RECORD)?;
    record
        .into_values()
        .map(|popup| {
            let dir = home().join("mods").join(format!("popup-{}", popup.name));
            let changed = if dir.exists() { !untouched(&dir, &popup.files)? } else { true };
            Ok(Description { popup, changed })
        })
        .collect()
}

pub fn add(name: &str, command: &str, width: f64, height: f64, key: &str, persistent: bool) -> Result<()> {
    add_with(name, command, width, height, key, persistent, check_mod)
}

fn add_with(name: &str, command: &str, width: f64, height: f64, key: &str, persistent: bool, check: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    if !valid_name(name) {
        bail!("name must contain only lowercase letters, digits and hyphens");
    }
    let argv = split_command(command)?;
    if !(0.2..=1.0).contains(&width) || !(0.2..=1.0).contains(&height) {
        bail!("width and height must be between 0.2 and 1");
    }
    let key = key.trim();
    if !key.is_empty() {
        let (errors, _) = keys::problems("global", key);
        if !errors.is_empty() {
            bail!("{}", errors.join("; "));
        }
    }
    fs::create_dir_all(state_dir())?;
    let _lock = fsutil::lock(&state_dir().join("popups.lock"))?;
    let mut record: Record = read_record(RECORD)?;
    if record.contains_key(name) {
        bail!("popup {name:?} already exists");
    }
    let mods = home().join("mods");
    fs::create_dir_all(&mods)?;
    let dir = mods.join(format!("popup-{name}"));
    if dir.exists() {
        bail!("{} already exists", dir.display());
    }
    let stage = fsutil::temp_path(&mods, "popup-stage");
    fs::create_dir(&stage)?;
    let mut popup = Popup { name: name.into(), command: command.into(), argv, width, height, key: key.into(), persistent, files: BTreeMap::new() };
    let preparation = (|| -> Result<()> {
        for (file, text) in TEMPLATE {
            let content = text.replace("__POPUP_NAME__", &serde_json::to_string(name)?);
            fsutil::atomic_write(&stage.join(file), content.as_bytes(), 0o644)?;
            popup.files.insert((*file).into(), digest(content.as_bytes()));
        }
        let config = serde_json::to_string_pretty(&serde_json::json!({"name": name, "argv": popup.argv, "width": width, "height": height, "persistent": persistent}))? + "\n";
        fsutil::atomic_write(&stage.join("popup.json"), config.as_bytes(), 0o644)?;
        popup.files.insert("popup.json".into(), digest(config.as_bytes()));
        fs::rename(&stage, &dir)?;
        Ok(())
    })();
    if let Err(e) = preparation {
        if stage.exists() {
            fs::remove_dir_all(&stage).context("remove popup staging directory")?;
        }
        return Err(e);
    }
    let action = format!("mod.open:popup-{name}");
    let mut bound = false;
    let result = (|| -> Result<()> {
        check(&dir).context("popup check failed")?;
        if !key.is_empty() {
            let binding = keys::Binding { section: "global".into(), key: key.into(), action: action.clone() };
            if let Some(conflict) = keys::set_if_free(&binding)? {
                bail!("{key} is already used by {} in {}", conflict.action, conflict.section);
            }
            bound = true;
        }
        record.insert(name.into(), popup);
        write_record(RECORD, &record)
    })();
    if let Err(error) = result {
        let mut failures = Vec::new();
        if bound && let Err(e) = keys::remove_if_bound("global", key, &action) {
            failures.push(format!("remove key: {e:#}"));
        }
        if let Err(e) = fs::remove_dir_all(&dir) {
            failures.push(format!("remove popup folder: {e}"));
        }
        if !failures.is_empty() {
            bail!("{error:#}; rollback failed: {}", failures.join("; "));
        }
        return Err(error);
    }
    Ok(())
}

pub fn remove(name: &str) -> Result<Report> {
    if !valid_name(name) {
        bail!("invalid popup name");
    }
    fs::create_dir_all(state_dir())?;
    let _lock = fsutil::lock(&state_dir().join("popups.lock"))?;
    let mut record: Record = read_record(RECORD)?;
    let popup = record.get(name).ok_or_else(|| anyhow!("popup {name:?} was not installed here"))?;
    let dir = home().join("mods").join(format!("popup-{name}"));
    let mut report = Report::default();
    if dir.exists() {
        if untouched(&dir, &popup.files)? {
            fs::remove_dir_all(&dir)?;
        } else {
            report.kept.push(dir.to_string_lossy().into_owned());
        }
    }
    if !popup.key.is_empty() {
        keys::remove_if_bound("global", &popup.key, &format!("mod.open:popup-{name}"))?;
    }
    record.remove(name);
    write_record(RECORD, &record)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_quoting_and_windows_paths() {
        assert_eq!(split_command(r#""C:\Program Files\tool.exe" 'two words' a\"b """#).unwrap(), vec![r"C:\Program Files\tool.exe", "two words", "a\"b", ""]);
        assert!(split_command("'unfinished").is_err());
        assert!(split_command("  ").is_err());
    }

    #[test]
    fn install_remove_and_preserve_edits() {
        let _guard = crate::testenv::isolated();
        let _ = fs::remove_dir_all(home());
        add_with("files", "lazygit", 0.8, 0.8, "alt-shift-f", true, |_| Ok(())).unwrap();
        assert_eq!(describe().unwrap()[0].popup.argv, ["lazygit"]);
        assert!(add_with("files", "other", 0.8, 0.8, "", false, |_| Ok(())).is_err());
        fs::write(home().join("mods/popup-files/main.js"), "changed").unwrap();
        assert!(describe().unwrap()[0].changed);
        assert_eq!(remove("files").unwrap().kept.len(), 1);
        assert_eq!(fs::read_to_string(home().join("mods/popup-files/main.js")).unwrap(), "changed");
        assert!(keys::load().unwrap().is_empty());
        let _ = fs::remove_dir_all(home());
    }

    #[test]
    fn clean_removal_keeps_reassigned_key() {
        let _guard = crate::testenv::isolated();
        let _ = fs::remove_dir_all(home());
        add_with("clean", "fzf", 1.0, 0.2, "alt-shift-f", false, |_| Ok(())).unwrap();
        keys::set("global", None, "alt-shift-f", "tab.new").unwrap();
        assert!(remove("clean").unwrap().kept.is_empty());
        assert!(!home().join("mods/popup-clean").exists());
        assert_eq!(keys::load().unwrap()[0].action, "tab.new");
        let _ = fs::remove_dir_all(home());
    }

    #[test]
    fn refuses_invalid_conflicts_and_failed_check() {
        let _guard = crate::testenv::isolated();
        let _ = fs::remove_dir_all(home());
        assert!(add_with("../bad", "x", 0.8, 0.8, "", false, |_| Ok(())).is_err());
        assert!(add_with("valid", "x", 0.1, 0.8, "", false, |_| Ok(())).is_err());
        assert!(add_with("valid", "x", 0.8, 0.8, "", false, |_| bail!("bad mod")).is_err());
        assert!(!home().join("mods/popup-valid").exists());
        keys::set("global", None, "alt-shift-f", "tab.new").unwrap();
        assert!(add_with("valid", "x", 0.8, 0.8, "alt-shift-f", false, |_| Ok(())).is_err());
        assert_eq!(keys::load().unwrap()[0].action, "tab.new");
        assert!(!home().join("mods/popup-valid").exists());
        let _ = fs::remove_dir_all(home());
    }
}
