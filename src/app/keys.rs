//! The app's keys.json: sections of key → action. A more specific section
//! wins: terminal over layout:<name> over global, and mod:<name> while that
//! mod has focus.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};

use super::{AppCommand, home, read_object, read_record, state_dir, write_record};
use crate::config::env;
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
    Ok(bindings(read_object(&path())?))
}

/// The bindings in a keys.json's sections. An object binding's action is its
/// JSON text, as the app's keys() reports it.
pub fn bindings(sections: Map<String, Value>) -> Vec<Binding> {
    let mut out = Vec::new();
    for (section, keys) in sections {
        let Value::Object(keys) = keys else { continue };
        for (key, action) in keys {
            let action = match action {
                Value::String(action) => action,
                Value::Object(_) => action.to_string(),
                _ => continue,
            };
            out.push(Binding { section: section.clone(), key, action });
        }
    }
    out
}

/// An action as keys.json holds it: an object binding's JSON text goes back
/// as the object.
fn action_value(action: &str) -> Value {
    match serde_json::from_str(action) {
        Ok(Value::Object(o)) => Value::Object(o),
        _ => Value::String(action.into()),
    }
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
                    out.insert(key.into(), action_value(action));
                    placed = true;
                }
                continue;
            }
            out.insert(k, v);
        }
        if !placed {
            out.insert(key.into(), action_value(action));
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

/// The shipped keys file for this OS in the app's repository.
const SHIPPED_FILE: &str = if cfg!(target_os = "macos") { "keys.macos.json" } else { "keys.json" };

/// Shipped keys the person turned down, as section, key and action.
const DISMISSED: &str = "keys-dismissed.json";

/// The keys the app at version ships. A tag never changes, so the file is
/// fetched once and kept with the other records.
fn shipped(version: &str) -> Result<Vec<Binding>> {
    let cache = state_dir().join(format!("shipped-{version}-{SHIPPED_FILE}"));
    if let Ok(Value::Object(map)) = std::fs::read_to_string(&cache).map_err(anyhow::Error::from).and_then(|t| Ok(serde_json::from_str(&t)?)) {
        return Ok(bindings(map));
    }
    let base = env("HERDR_CUSTOMIZE_RAW_BASE").unwrap_or_else(|| "https://raw.githubusercontent.com".into());
    let url = format!("{base}/herdrdev/herdr-app/v{version}/defaults/{SHIPPED_FILE}");
    let text = ureq::get(&url).config().timeout_global(Some(Duration::from_secs(5))).build().call().with_context(|| url.clone())?.body_mut().read_to_string()?;
    let Value::Object(map) = serde_json::from_str(&text).with_context(|| url.clone())? else { bail!("{url} is not a JSON object") };
    std::fs::create_dir_all(state_dir())?;
    fsutil::atomic_write(&cache, text.as_bytes(), 0o644)?;
    Ok(bindings(map))
}

/// The shipped keys the person's file lacks. One whose key or action the file
/// already uses is left out, so a key moved or removed on purpose stays so.
fn missing(shipped: &[Binding], current: &[Binding]) -> Vec<Binding> {
    let taken = |s: &Binding| current.iter().any(|c| c.action == s.action || (c.section == s.section && normalize(&c.key) == normalize(&s.key)));
    shipped.iter().filter(|s| !taken(s)).cloned().collect()
}

fn triple(b: &Binding) -> [String; 3] {
    [b.section.clone(), b.key.clone(), b.action.clone()]
}

/// The keys the app at version ships that keys.json lacks and the person has
/// not turned down. The app stops adding its new keys once keys.json is edited.
pub fn new_shipped(version: &str) -> Result<Vec<Binding>> {
    let dismissed: Vec<[String; 3]> = read_record(DISMISSED)?;
    Ok(missing(&shipped(version)?, &load()?).into_iter().filter(|b| !dismissed.contains(&triple(b))).collect())
}

/// Adds every key new_shipped finds.
pub fn add_shipped(version: &str) -> Result<()> {
    let fresh = new_shipped(version)?;
    if fresh.is_empty() {
        return Ok(());
    }
    edit(|map| {
        for b in &fresh {
            section_mut(map, &b.section)?.insert(b.key.clone(), action_value(&b.action));
        }
        Ok(())
    })
}

/// Stops offering every key new_shipped finds.
pub fn dismiss_shipped(version: &str) -> Result<()> {
    let mut dismissed: Vec<[String; 3]> = read_record(DISMISSED)?;
    dismissed.extend(new_shipped(version)?.iter().map(triple));
    write_record(DISMISSED, &dismissed)
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
    let name = if action.starts_with('{') {
        let Ok(Value::Object(o)) = serde_json::from_str::<Value>(action) else { return Some("not a JSON object".into()) };
        if let Some(k) = o.keys().find(|k| !matches!(k.as_str(), "command" | "args")) {
            return Some(format!("unknown field {k:?}; an object binding has command and args"));
        }
        if o.get("args").is_some_and(|a| !a.is_object()) {
            return Some("args must be an object".into());
        }
        match o.get("command") {
            Some(Value::String(c)) => c.clone(),
            _ => return Some("command must name a command".into()),
        }
    } else if action == "terminal" || action.starts_with("send:") {
        return None;
    } else {
        action.split_once(':').map_or(action, |(n, _)| n).to_string()
    };
    if commands.is_empty() {
        return None;
    }
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
        assert_eq!(action_problem(r#"{"command":"tab.new","args":{"name":"x"}}"#, &commands, &mods), None);
        assert!(action_problem(r#"{"command":"tab.nope"}"#, &commands, &mods).unwrap().contains("tab.nope"));
        for bad in [r#"{"command":"tab.new","extra":1}"#, r#"{"command":"tab.new","args":[]}"#, r#"{"args":{}}"#, "{nope"] {
            assert!(action_problem(bad, &[], &mods).is_some(), "{bad}");
        }
    }

    // A binding with typed arguments is listed, and moving its key keeps it an
    // object; written back as a string, the app would refuse it.
    #[test]
    fn object_bindings_stay_objects() {
        let _guard = crate::ui::tests::isolated();
        let _ = std::fs::remove_dir_all(home());
        std::fs::create_dir_all(home()).unwrap();
        std::fs::write(path(), r#"{"global":{"ctrl-shift-r":{"command":"layout.resize","args":{"id":"sidebar","px":240}}}}"#).unwrap();
        let action = load().unwrap()[0].action.clone();
        assert_eq!(action, r#"{"command":"layout.resize","args":{"id":"sidebar","px":240}}"#);
        set("global", Some("ctrl-shift-r"), "alt-r", &action).unwrap();
        let file: Value = serde_json::from_str(&std::fs::read_to_string(path()).unwrap()).unwrap();
        assert_eq!(file["global"]["alt-r"]["args"]["px"], 240);
        let _ = std::fs::remove_dir_all(home());
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

    // An edited keys.json gets the app's new keys only from here, and a key
    // the person moved, removed or turned down is never offered back.
    #[test]
    fn new_shipped_keys_are_offered_once() {
        let _guard = crate::ui::tests::isolated();
        let _ = std::fs::remove_dir_all(home());
        std::fs::create_dir_all(state_dir()).unwrap();
        let shipped = r#"{"global":{"ctrl-shift-t":"tab.new","ctrl-shift-w":"tab.close","ctrl-?":"app.help"},"terminal":{"ctrl-shift-f":"terminal.search"}}"#;
        std::fs::write(state_dir().join(format!("shipped-9.9.9-{SHIPPED_FILE}")), shipped).unwrap();
        // tab.new moved to alt-t, tab.close's key now opens a mod.
        std::fs::write(path(), r#"{"global":{"alt-t":"tab.new","shift-ctrl-w":"mod.open:x"}}"#).unwrap();
        let keys = |v: Vec<Binding>| v.into_iter().map(|b| b.key).collect::<Vec<_>>();
        assert_eq!(keys(new_shipped("9.9.9").unwrap()), ["ctrl-?", "ctrl-shift-f"]);
        add_shipped("9.9.9").unwrap();
        assert!(new_shipped("9.9.9").unwrap().is_empty());
        assert_eq!(load().unwrap().iter().find(|b| b.key == "ctrl-shift-f").unwrap().section, "terminal");
        remove("global", "ctrl-?").unwrap();
        assert_eq!(keys(new_shipped("9.9.9").unwrap()), ["ctrl-?"]);
        dismiss_shipped("9.9.9").unwrap();
        assert!(new_shipped("9.9.9").unwrap().is_empty());
        let _ = std::fs::remove_dir_all(home());
    }
}
