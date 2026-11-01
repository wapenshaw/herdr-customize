//! The app's keys.json: sections of key → action. A more specific section
//! wins: terminal over layout:<name> over global, and mod:<name> while that
//! mod has focus.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};

use super::{AppCommand, home, read_object, state_dir};
use crate::fsutil;

pub fn path() -> PathBuf {
    home().join("keys.json")
}

/// One key in one section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub section: String,
    pub key: String,
    pub action: String,
}

/// Every binding, in the file's order.
pub fn load() -> Result<Vec<Binding>> {
    let mut out = Vec::new();
    for (section, keys) in read_object(&path())? {
        let Value::Object(keys) = keys else { continue };
        for (key, action) in keys {
            if let Value::String(action) = action {
                out.push(Binding { section: section.clone(), key, action });
            }
        }
    }
    Ok(out)
}

/// Applies edit to keys.json under a lock, keeping a daily backup. The app
/// picks the change up by itself.
fn edit(f: impl FnOnce(&mut Map<String, Value>) -> Result<()>) -> Result<()> {
    std::fs::create_dir_all(state_dir())?;
    let _lock = fsutil::lock(&state_dir().join("keys.lock")).context("lock keys.json")?;
    let mut map = read_object(&path())?;
    f(&mut map)?;
    let text = serde_json::to_string_pretty(&Value::Object(map))? + "\n";
    fsutil::write_backed_up(&path(), text.as_bytes(), 0o644)
}

fn section_mut<'a>(map: &'a mut Map<String, Value>, section: &str) -> Result<&'a mut Map<String, Value>> {
    match map.entry(section).or_insert_with(|| Value::Object(Map::new())) {
        Value::Object(keys) => Ok(keys),
        _ => bail!("keys.json: {section} is not a section of keys"),
    }
}

/// Binds key in section to action. A binding that had the key old takes the
/// new key in its place, and any other binding of key in section goes.
pub fn set(section: &str, old: Option<&str>, key: &str, action: &str) -> Result<()> {
    edit(|map| {
        let keys = section_mut(map, section)?;
        let mut out = Map::new();
        let mut placed = false;
        for (k, v) in std::mem::take(keys) {
            if Some(k.as_str()) == old || k == key {
                if !placed {
                    out.insert(key.into(), Value::String(action.into()));
                    placed = true;
                }
                continue;
            }
            out.insert(k, v);
        }
        if !placed {
            out.insert(key.into(), Value::String(action.into()));
        }
        *keys = out;
        Ok(())
    })
}

/// Unbinds key in section.
pub fn remove(section: &str, key: &str) -> Result<()> {
    edit(|map| {
        section_mut(map, section)?.retain(|k, _| k != key);
        Ok(())
    })
}

/// Modifiers in the order the app writes them.
const MODIFIERS: [&str; 6] = ["ctrl", "alt", "shift", "cmd", "super", "fn"];

const NAMED: [&str; 15] = ["enter", "escape", "tab", "backspace", "delete", "insert", "home", "end", "pageup", "pagedown", "up", "down", "left", "right", "space"];

/// Keys programs rely on with ctrl alone, which the app's own guide asks
/// never to take unasked.
const PROGRAM_KEYS: [&str; 11] = ["c", "d", "l", "r", "w", "a", "e", "u", "k", "z", "["];

/// Splits one keystroke, such as "ctrl-shift-t" or "ctrl--", into its
/// modifiers and its key.
fn keystroke(stroke: &str) -> Result<(Vec<&'static str>, &str), String> {
    let mut rest = stroke;
    let mut mods = Vec::new();
    while let Some(m) = MODIFIERS.iter().find(|m| rest.len() > m.len() + 1 && rest.starts_with(**m) && rest.as_bytes()[m.len()] == b'-') {
        if mods.contains(m) {
            return Err(format!("{m} appears twice in {stroke:?}"));
        }
        mods.push(*m);
        rest = &rest[m.len() + 1..];
    }
    let single = rest.chars().count() == 1 && !rest.starts_with(char::is_whitespace);
    let function = rest.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| (1..=24).contains(&n));
    if single && rest.chars().any(|c| c.is_ascii_uppercase()) {
        return Err(format!("write {rest:?} in lowercase, with shift- if it needs shift"));
    }
    if !(single || function || NAMED.contains(&rest)) {
        return Err(if rest.contains('-') { format!("unknown modifier in {stroke:?} (ctrl, alt, shift, cmd, super)") } else { format!("unknown key {rest:?}") });
    }
    Ok((mods, rest))
}

/// The key with its modifiers in the app's order, to compare spellings.
pub fn normalize(key: &str) -> String {
    key.split(' ')
        .map(|stroke| match keystroke(stroke) {
            Ok((mut mods, rest)) => {
                mods.sort_by_key(|m| MODIFIERS.iter().position(|x| x == m));
                mods.into_iter().chain([rest]).collect::<Vec<_>>().join("-")
            }
            Err(_) => stroke.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A binding of the same key in a section that wins over section where both
/// count: `terminal` over `layout:<name>` over `global`.
pub fn hidden_by<'a>(bindings: &'a [Binding], section: &str, key: &str) -> Option<&'a Binding> {
    let rank = |s: &str| match s {
        "terminal" => Some(2),
        s if s.starts_with("layout:") => Some(1),
        "global" => Some(0),
        _ => None,
    };
    let mine = rank(section)?;
    let key = normalize(key);
    bindings.iter().find(|b| rank(&b.section).is_some_and(|r| r > mine) && normalize(&b.key) == key)
}

/// What makes key unusable in section, then what it takes from programs.
pub fn problems(section: &str, key: &str) -> (Vec<String>, Vec<String>) {
    if key.trim().is_empty() {
        return (vec!["a key is required".into()], Vec::new());
    }
    let strokes: Vec<&str> = key.split(' ').collect();
    let errors: Vec<String> = strokes.iter().filter_map(|s| keystroke(s).err()).collect();
    let mut warnings = Vec::new();
    let Ok((mods, rest)) = keystroke(strokes[0]) else { return (errors, warnings) };
    if section.starts_with("mod:") {
        return (errors, warnings);
    }
    if mods.is_empty() && rest.chars().count() == 1 {
        warnings.push(format!("{key} alone types text; bound here, programs never get it"));
    }
    if mods == ["ctrl"] && PROGRAM_KEYS.contains(&rest) {
        warnings.push(format!("programs rely on {key}; bound here, they no longer get it"));
    }
    if mods.contains(&"ctrl") && mods.contains(&"alt") {
        warnings.push("ctrl-alt is AltGr on Windows, which types characters on many keyboards".into());
    }
    (errors, warnings)
}

/// What is wrong with an action, given the app's commands and its mods.
pub fn action_problem(action: &str, commands: &[AppCommand], mods: &[String]) -> Option<String> {
    let action = action.trim();
    if action.is_empty() {
        return Some("an action is required".into());
    }
    if action == "terminal" || action.starts_with("send:") || commands.is_empty() {
        return None;
    }
    let name = action.split_once(':').map_or(action, |(n, _)| n);
    if commands.iter().any(|c| c.name == name) {
        return None;
    }
    match name.split_once('.') {
        Some((m, _)) if mods.iter().any(|x| x == m) => None,
        _ => Some(format!("no command {name:?} (see the list below)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keystrokes() {
        let errors = |k: &str| problems("global", k).0;
        let warnings = |s: &str, k: &str| problems(s, k).1;
        assert_eq!(normalize("shift-ctrl-t"), "ctrl-shift-t");
        assert_eq!(normalize("ctrl--"), "ctrl--");
        assert_eq!(problems("global", "ctrl-shift-t"), (vec![], vec![]));
        assert!(errors("ctrl-+").is_empty() && errors("shift-f2").is_empty() && errors("alt-pagedown").is_empty());
        assert!(errors("ctrl-T")[0].contains("lowercase"));
        assert!(errors("hyper-x")[0].contains("unknown modifier"));
        assert!(errors("ctrl-ctrl-x")[0].contains("twice"));
        assert!(errors("").len() == 1);
        assert!(warnings("global", "x")[0].contains("types text"));
        assert!(warnings("mod:editor", "x").is_empty());
        assert!(warnings("terminal", "ctrl-c")[0].contains("programs rely"));
        assert!(warnings("global", "ctrl-alt-x")[0].contains("AltGr"));
        assert_eq!(problems("global", "ctrl-shift-k s"), (vec![], vec![]), "a sequence of strokes");
        assert!(errors("ctrl-shift-k nope")[0].contains("unknown key"));
    }

    // A global key that a terminal binding also takes does nothing while a
    // terminal has focus, which is most of the time.
    #[test]
    fn a_more_specific_section_hides_a_key() {
        let b = |section: &str, key: &str| Binding { section: section.into(), key: key.into(), action: "x".into() };
        let all = [b("terminal", "shift-ctrl-c"), b("layout:main", "ctrl-shift-j"), b("mod:editor", "ctrl-shift-y")];
        assert_eq!(hidden_by(&all, "global", "ctrl-shift-c").map(|b| b.section.as_str()), Some("terminal"));
        assert_eq!(hidden_by(&all, "global", "ctrl-shift-j").map(|b| b.section.as_str()), Some("layout:main"));
        assert!(hidden_by(&all, "terminal", "ctrl-shift-c").is_none() && hidden_by(&all, "layout:main", "ctrl-shift-c").is_some());
        assert!(hidden_by(&all, "global", "ctrl-shift-y").is_none() && hidden_by(&all, "mod:editor", "ctrl-shift-c").is_none());
    }

    #[test]
    fn actions() {
        let commands = vec![AppCommand { name: "tab.new".into() }, AppCommand { name: "space.select".into() }];
        let mods = vec!["editor".to_string()];
        for ok in ["tab.new", "space.select:3", "send:\u{17}", "terminal", "editor.save"] {
            assert_eq!(action_problem(ok, &commands, &mods), None, "{ok}");
        }
        assert!(action_problem("tab.nope", &commands, &mods).is_some());
        assert!(action_problem("", &commands, &mods).is_some());
        assert_eq!(action_problem("anything", &[], &mods), None, "unknown without the list");
    }

    // Edits keep every other binding where it was, and a renamed key keeps
    // its place.
    #[test]
    fn edits_keep_the_order() {
        let _guard = crate::ui::tests::isolated();
        let _ = std::fs::remove_dir_all(home());
        std::fs::create_dir_all(home()).unwrap();
        std::fs::write(path(), r#"{"global":{"alt-1":"space.select:1","ctrl-shift-t":"tab.new","f2":"tab.rename"},"terminal":{"ctrl-v":"terminal.paste"}}"#).unwrap();
        set("global", Some("ctrl-shift-t"), "ctrl-shift-y", "tab.new").unwrap();
        set("global", None, "alt-2", "space.select:2").unwrap();
        remove("terminal", "ctrl-v").unwrap();
        let keys: Vec<String> = load().unwrap().into_iter().map(|b| format!("{} {}", b.key, b.action)).collect();
        assert_eq!(keys, ["alt-1 space.select:1", "ctrl-shift-y tab.new", "f2 tab.rename", "alt-2 space.select:2"]);
        // Taking a key from another binding in the section replaces it.
        set("global", Some("alt-2"), "f2", "space.select:2").unwrap();
        let keys: Vec<String> = load().unwrap().into_iter().map(|b| b.key).collect();
        assert_eq!(keys, ["alt-1", "ctrl-shift-y", "f2"]);
        assert_eq!(load().unwrap()[2].action, "space.select:2");
        let _ = std::fs::remove_dir_all(home());
    }
}
