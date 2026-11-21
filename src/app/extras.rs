use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use super::{AppCommand, check_mod, digest, home, keys, mods, read_record, state_dir, write_record};
use crate::fsutil;

const RECORD: &str = "extras.json";
const GENERATED: &[&str] = &["gpui-kit.d.ts", "herdr.d.ts", "jsconfig.json"];

struct Extra {
    name: &'static str,
    title: &'static str,
    needs: &'static [&'static str],
    files: &'static [(&'static str, &'static str)],
}

const EXTRAS: &[Extra] = &[
    Extra {
        name: "copymode",
        title: "Copy mode",
        needs: &["terminal.select", "terminal.adjust_selection", "terminal.clear_selection", "terminal.copy", "terminal.scroll"],
        files: &[
            ("mod.json", include_str!("../../app/extras/copymode/mod.json")),
            ("main.js", include_str!("../../app/extras/copymode/main.js")),
            ("keys.json", include_str!("../../app/extras/copymode/keys.json")),
            ("RECIPE.md", include_str!("../../app/extras/copymode/RECIPE.md")),
        ],
    },
    Extra {
        name: "scrollback",
        title: "Scrollback in an editor",
        needs: &["terminal.open"],
        files: &[
            ("mod.json", include_str!("../../app/extras/scrollback/mod.json")),
            ("main.js", include_str!("../../app/extras/scrollback/main.js")),
            ("keys.json", include_str!("../../app/extras/scrollback/keys.json")),
            ("RECIPE.md", include_str!("../../app/extras/scrollback/RECIPE.md")),
        ],
    },
];

#[derive(Default, Serialize, Deserialize)]
struct Installed {
    files: BTreeMap<String, String>,
    keys: Vec<[String; 3]>,
}

type Record = BTreeMap<String, Installed>;

#[derive(Serialize)]
pub struct KeyStatus {
    section: String,
    key: String,
    action: String,
    status: &'static str,
    conflict: Option<String>,
}

#[derive(Serialize)]
pub struct Description {
    name: &'static str,
    title: &'static str,
    about: String,
    installed: bool,
    changed: bool,
    available: bool,
    missing: Vec<&'static str>,
    keys: Vec<KeyStatus>,
}

#[derive(Default, Serialize)]
pub struct Report {
    added: Vec<String>,
    skipped: Vec<String>,
    kept: Vec<String>,
}

fn extra(name: &str) -> Result<&'static Extra> {
    EXTRAS.iter().find(|e| e.name == name).ok_or_else(|| anyhow!("unknown extra {name:?}"))
}

fn extra_keys(extra: &Extra) -> Result<Vec<keys::Binding>> {
    let text = extra.files.iter().find(|(name, _)| *name == "keys.json").ok_or_else(|| anyhow!("missing extra keys"))?.1;
    let map = serde_json::from_str(text)?;
    let modifier = if cfg!(target_os = "macos") { "cmd-" } else { "ctrl-" };
    Ok(keys::bindings(map).into_iter().filter(|b| b.key.starts_with(modifier)).collect())
}

fn missing(extra: &Extra, commands: &[AppCommand]) -> Vec<&'static str> {
    extra.needs.iter().copied().filter(|name| !commands.iter().any(|c| c.name == *name)).collect()
}

fn status(binding: &keys::Binding, current: &[keys::Binding], owned: Option<&Installed>) -> KeyStatus {
    let held = current.iter().find(|b| b.section == binding.section && keys::normalize(&b.key) == keys::normalize(&binding.key));
    let ours = held.is_some_and(|b| owned.is_some_and(|r| r.keys.iter().any(|k| k[0] == b.section && k[1] == b.key && k[2] == b.action)));
    let hidden = keys::hidden_by(current, &binding.section, &binding.key);
    let (status, conflict) = if let Some(b) = held.filter(|_| !ours) {
        ("taken", Some(format!("{}: {}", b.section, b.action)))
    } else if let Some(b) = hidden {
        ("hidden", Some(format!("{}: {}", b.section, b.action)))
    } else if ours {
        ("installed", None)
    } else {
        ("free", None)
    };
    KeyStatus { section: binding.section.clone(), key: binding.key.clone(), action: binding.action.clone(), status, conflict }
}

fn untouched(dir: &Path, record: &Installed) -> Result<bool> {
    fn visit(dir: &Path, root: &Path, files: &BTreeMap<String, String>) -> Result<bool> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let rel = path.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
            let kind = entry.file_type()?;
            if kind.is_dir() {
                if !visit(&path, root, files)? {
                    return Ok(false);
                }
            } else if !kind.is_file() {
                return Ok(false);
            } else if let Some(expected) = files.get(&rel) {
                if digest(&fs::read(&path)?) != *expected {
                    return Ok(false);
                }
            } else if !GENERATED.contains(&rel.as_str()) {
                return Ok(false);
            }
        }
        Ok(true)
    }
    if !fs::symlink_metadata(dir)?.file_type().is_dir() {
        return Ok(false);
    }
    for (path, expected) in &record.files {
        match fs::read(dir.join(path)) {
            Ok(bytes) if digest(&bytes) == *expected => {}
            Ok(_) => return Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        }
    }
    visit(dir, dir, &record.files)
}

pub fn describe(commands: &[AppCommand]) -> Result<Vec<Description>> {
    let record: Record = read_record(RECORD)?;
    let current = keys::load()?;
    EXTRAS
        .iter()
        .map(|e| {
            let dir = home().join("mods").join(e.name);
            let installed = dir.exists();
            let owned = record.get(e.name);
            let changed = installed
                && match owned {
                    Some(r) => !untouched(&dir, r)?,
                    None => true,
                };
            let missing = missing(e, commands);
            let recipe = e.files.iter().find(|(name, _)| *name == "RECIPE.md").map_or("", |(_, text)| *text);
            Ok(Description {
                name: e.name,
                title: e.title,
                about: mods::summary(recipe),
                installed,
                changed,
                available: missing.is_empty(),
                missing,
                keys: extra_keys(e)?.iter().map(|b| status(b, &current, owned)).collect(),
            })
        })
        .collect()
}

pub fn install(name: &str, commands: &[AppCommand]) -> Result<Report> {
    install_with(name, commands, check_mod)
}

fn install_with(name: &str, commands: &[AppCommand], check: impl FnOnce(&Path) -> Result<()>) -> Result<Report> {
    let extra = extra(name)?;
    let missing = missing(extra, commands);
    if !missing.is_empty() {
        bail!("Needs Herdr 0.1.16 or newer; missing commands: {}", missing.join(", "));
    }
    fs::create_dir_all(state_dir())?;
    let _lock = fsutil::lock(&state_dir().join("extras.lock"))?;
    let mut record: Record = read_record(RECORD)?;
    let mods_dir = home().join("mods");
    fs::create_dir_all(&mods_dir)?;
    let dir = mods_dir.join(name);
    let existed = dir.exists();
    if existed {
        let old = record.get(name).ok_or_else(|| anyhow!("{name} already exists and was not installed here"))?;
        if !untouched(&dir, old)? {
            bail!("{name} has edited or unowned files; update refused");
        }
    }
    let stage = fsutil::temp_path(&state_dir(), "extra-stage");
    fs::create_dir(&stage)?;
    let backup = fsutil::temp_path(&state_dir(), "extra-backup");
    let mut next = Installed::default();
    let prepare = (|| -> Result<()> {
        for (path, text) in extra.files {
            let dest = stage.join(path);
            fs::create_dir_all(dest.parent().unwrap_or(&stage))?;
            fsutil::atomic_write(&dest, text.as_bytes(), 0o644)?;
            next.files.insert((*path).into(), digest(text.as_bytes()));
        }
        if existed {
            fs::rename(&dir, &backup)?;
        }
        if let Err(e) = fs::rename(&stage, &dir) {
            if existed {
                fs::rename(&backup, &dir)?;
            }
            return Err(e.into());
        }
        Ok(())
    })();
    if let Err(e) = prepare {
        if stage.exists() {
            fs::remove_dir_all(&stage).with_context(|| format!("{e:#}; remove failed extra staging directory"))?;
        }
        return Err(e);
    }
    let mut added = Vec::new();
    let result = (|| -> Result<Report> {
        check(&dir).context("extra check failed")?;
        let mut current = keys::load()?;
        let mut report = Report::default();
        let old = record.get(name);
        if let Some(old) = old {
            next.keys = old.keys.iter().filter(|k| current.iter().any(|b| b.section == k[0] && b.key == k[1] && b.action == k[2])).cloned().collect();
        }
        for b in extra_keys(extra)? {
            let s = status(&b, &current, old);
            if s.status == "free" {
                if let Some(conflict) = keys::set_if_free(&b)? {
                    report.skipped.push(format!("{} ({}: {})", b.key, conflict.section, conflict.action));
                } else {
                    next.keys.push([b.section.clone(), b.key.clone(), b.action.clone()]);
                    report.added.push(b.key.clone());
                    added.push(b.clone());
                    current.push(b);
                }
            } else if s.status != "installed" {
                report.skipped.push(format!("{} ({})", b.key, s.conflict.unwrap_or_else(|| s.status.into())));
            }
        }
        record.insert(name.into(), next);
        write_record(RECORD, &record)?;
        Ok(report)
    })();
    match result {
        Err(error) => {
            let mut failures = Vec::new();
            for b in added {
                if let Err(e) = keys::remove_if_bound(&b.section, &b.key, &b.action) {
                    failures.push(format!("restore key {}: {e:#}", b.key));
                }
            }
            match fs::remove_dir_all(&dir) {
                Ok(()) => {
                    if existed && let Err(e) = fs::rename(&backup, &dir) {
                        failures.push(format!("restore previous extra from {}: {e}", backup.display()));
                    }
                }
                Err(e) => failures.push(format!("remove failed extra install: {e}")),
            }
            if failures.is_empty() { Err(error) } else { Err(anyhow!("{error:#}; rollback failed: {}", failures.join("; "))) }
        }
        Ok(report) => {
            if existed {
                fs::remove_dir_all(&backup).context("remove extra backup")?;
            }
            Ok(report)
        }
    }
}

pub fn remove(name: &str) -> Result<Report> {
    extra(name)?;
    fs::create_dir_all(state_dir())?;
    let _lock = fsutil::lock(&state_dir().join("extras.lock"))?;
    let mut record: Record = read_record(RECORD)?;
    let old = record.get(name).ok_or_else(|| anyhow!("{name} was not installed here"))?;
    let dir = home().join("mods").join(name);
    let mut report = Report::default();
    if dir.exists() {
        if untouched(&dir, old)? {
            fs::remove_dir_all(&dir)?;
        } else {
            report.kept.push(dir.to_string_lossy().into_owned());
        }
    }
    for [section, key, action] in &old.keys {
        keys::remove_if_bound(section, key, action)?;
    }
    record.remove(name);
    write_record(RECORD, &record)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commands() -> Vec<AppCommand> {
        EXTRAS.iter().flat_map(|e| e.needs.iter()).map(|name| AppCommand { name: (*name).into(), ..Default::default() }).collect()
    }

    #[test]
    fn extras_do_not_collide_with_default_keys() {
        let defaults = keys::app_defaults();
        for extra in EXTRAS {
            for binding in extra_keys(extra).unwrap() {
                assert_eq!(status(&binding, &defaults, None).status, "free", "{} {}", extra.name, binding.key);
            }
        }
    }

    #[test]
    fn install_conflicts_updates_and_edited_removal() {
        let _guard = crate::testenv::isolated();
        let _ = fs::remove_dir_all(home());
        let available = commands();
        let key = extra_keys(extra("copymode").unwrap()).unwrap().remove(0).key;
        let first = install_with("copymode", &available, |_| Ok(())).unwrap();
        assert_eq!(first.added, vec![key]);
        assert!(home().join("mods/copymode/main.js").is_file());
        assert_eq!(keys::load().unwrap()[0].action, "mod.open:copymode");
        assert!(install_with("copymode", &available, |_| Ok(())).unwrap().added.is_empty());
        assert!(install_with("copymode", &available, |_| bail!("check failed")).is_err());
        assert!(home().join("mods/copymode/main.js").is_file());
        assert_eq!(keys::load().unwrap()[0].action, "mod.open:copymode");
        fs::write(home().join("mods/copymode/main.js"), "edited").unwrap();
        assert!(install_with("copymode", &available, |_| Ok(())).is_err());
        assert!(!remove("copymode").unwrap().kept.is_empty());
        assert_eq!(fs::read_to_string(home().join("mods/copymode/main.js")).unwrap(), "edited");
        assert!(keys::load().unwrap().is_empty());
        let _ = fs::remove_dir_all(home());
    }

    #[test]
    fn conflict_missing_and_failed_check() {
        let _guard = crate::testenv::isolated();
        let _ = fs::remove_dir_all(home());
        let available = commands();
        let key = extra_keys(extra("scrollback").unwrap()).unwrap().remove(0).key;
        keys::set("global", None, &key, "tab.new").unwrap();
        assert!(!describe(&[]).unwrap().iter().find(|e| e.name == "scrollback").unwrap().available);
        assert!(install_with("scrollback", &[], |_| Ok(())).is_err());
        assert!(install_with("scrollback", &available, |_| bail!("check failed")).is_err());
        assert!(!home().join("mods/scrollback").exists());
        let report = install_with("scrollback", &available, |_| Ok(())).unwrap();
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(keys::load().unwrap()[0].action, "tab.new");
        remove("scrollback").unwrap();
        assert_eq!(keys::load().unwrap()[0].action, "tab.new");
        let _ = fs::remove_dir_all(home());
    }

    #[test]
    fn changed_binding_after_status_is_skipped() {
        let _guard = crate::testenv::isolated();
        let _ = fs::remove_dir_all(home());
        let available = commands();
        let binding = extra_keys(extra("scrollback").unwrap()).unwrap().remove(0);
        let snapshot = keys::load().unwrap();
        assert_eq!(status(&binding, &snapshot, None).status, "free");
        keys::set(&binding.section, None, &binding.key, "tab.new").unwrap();
        let conflict = keys::set_if_free(&binding).unwrap().unwrap();
        assert_eq!(conflict.action, "tab.new");
        let report = install_with("scrollback", &available, |_| Ok(())).unwrap();
        assert_eq!(report.skipped.len(), 1);
        assert!(report.skipped[0].contains("tab.new"));
        assert_eq!(keys::load().unwrap()[0].action, "tab.new");
        let _ = fs::remove_dir_all(home());
    }

    #[test]
    fn removal_keeps_a_reassigned_key() {
        let _guard = crate::testenv::isolated();
        let _ = fs::remove_dir_all(home());
        let binding = extra_keys(extra("copymode").unwrap()).unwrap().remove(0);
        install_with("copymode", &commands(), |_| Ok(())).unwrap();
        keys::set(&binding.section, None, &binding.key, "tab.new").unwrap();
        remove("copymode").unwrap();
        assert_eq!(keys::load().unwrap()[0].action, "tab.new");
        let _ = fs::remove_dir_all(home());
    }
}
