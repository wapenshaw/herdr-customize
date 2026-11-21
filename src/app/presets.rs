use anyhow::{Result, bail};
use serde_json::{Value, json};

use super::{AppCommand, keys};

struct Preset {
    id: &'static str,
    title: &'static str,
    about: &'static str,
    bindings: Vec<keys::Binding>,
}

fn binding(section: &str, key: &str, command: &str, args: &Value) -> keys::Binding {
    keys::Binding { section: section.into(), key: key.into(), action: json!({"command": command, "args": args}).to_string() }
}

fn presets() -> Vec<Preset> {
    let directions = ["left", "down", "up", "right"];
    vec![
        Preset {
            id: "pane-resize",
            title: "Pane resize",
            about: "Resize panes with Alt-Shift and the arrow keys.",
            bindings: directions.iter().map(|direction| binding("global", &format!("alt-shift-{direction}"), "pane.resize", &json!({"direction": direction}))).collect(),
        },
        Preset {
            id: "tab-reorder",
            title: "Reorder tabs",
            about: "Move the active tab left or right.",
            bindings: vec![binding("global", "alt-shift-[", "tab.move", &json!({"by": -1})), binding("global", "alt-shift-]", "tab.move", &json!({"by": 1}))],
        },
        Preset {
            id: "select-all",
            title: "Select terminal text",
            about: "Select the focused terminal's text.",
            bindings: vec![binding("terminal", if cfg!(target_os = "macos") { "cmd-a" } else { "ctrl-shift-a" }, "terminal.select_all", &json!({}))],
        },
        Preset {
            id: "search",
            title: "Search terminal",
            about: "Search the focused terminal.",
            bindings: vec![binding("terminal", if cfg!(target_os = "macos") { "cmd-f" } else { "ctrl-shift-f" }, "terminal.search", &json!({}))],
        },
        Preset { id: "pane-rename", title: "Name pane", about: "Ask for a name for the focused pane.", bindings: vec![binding("global", "alt-shift-r", "pane.rename", &json!({}))] },
    ]
}

fn available(commands: &[AppCommand]) -> Vec<Preset> {
    presets()
        .into_iter()
        .filter(|p| {
            p.bindings.iter().all(|b| {
                let action: Value = serde_json::from_str(&b.action).unwrap_or_default();
                commands.iter().any(|c| action["command"].as_str() == Some(c.name.as_str()))
            })
        })
        .collect()
}

fn normalized_action(action: &str, commands: &[AppCommand]) -> Option<(String, Value)> {
    if action.starts_with('{') {
        let value: Value = serde_json::from_str(action).ok()?;
        let name = value.get("command")?.as_str()?.to_owned();
        let args = value.get("args").cloned().unwrap_or_else(|| json!({}));
        return args.is_object().then_some((name, args));
    }
    let (name, argument) = action.split_once(':').map_or((action, None), |(name, value)| (name, Some(value)));
    let mut args = serde_json::Map::new();
    if let Some(value) = argument {
        let first = commands.iter().find(|command| command.name == name)?.args.first()?;
        args.insert(first.name.clone(), Value::String(value.into()));
    }
    Some((name.into(), Value::Object(args)))
}

fn same_action(a: &str, b: &str, commands: &[AppCommand]) -> bool {
    match (normalized_action(a, commands), normalized_action(b, commands)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn status(binding: &keys::Binding, current: &[keys::Binding], commands: &[AppCommand]) -> (&'static str, String) {
    if let Some(held) = current.iter().find(|b| b.section == binding.section && keys::normalize(&b.key) == keys::normalize(&binding.key)) {
        if same_action(&binding.action, &held.action, commands) {
            return ("set", String::new());
        }
        return ("taken", format!("runs {} in {}", held.action, held.section));
    }
    if let Some(held) = keys::hidden_by(current, &binding.section, &binding.key) {
        return ("hidden", format!("runs {} in {}, which wins", held.action, held.section));
    }
    ("free", String::new())
}

fn describe(p: &Preset, current: &[keys::Binding], commands: &[AppCommand]) -> Value {
    let bindings: Vec<Value> = p
        .bindings
        .iter()
        .map(|b| {
            let (state, reason) = status(b, current, commands);
            json!({"section": b.section, "key": b.key, "action": b.action, "status": state, "reason": reason})
        })
        .collect();
    json!({"id": p.id, "title": p.title, "about": p.about, "bindings": bindings})
}

pub fn list(commands: &[AppCommand]) -> Result<Value> {
    let current = keys::load()?;
    Ok(json!({"presets": available(commands).iter()
        .filter(|p| p.bindings.iter().any(|b| status(b, &current, commands).0 != "set"))
        .map(|p| describe(p, &current, commands)).collect::<Vec<_>>() }))
}

pub fn apply(id: &str, commands: &[AppCommand]) -> Result<Value> {
    let Some(preset) = available(commands).into_iter().find(|p| p.id == id) else { bail!("preset {id:?} is unavailable") };
    let mut outcomes = Vec::new();
    for b in &preset.bindings {
        let conflict = keys::set_if_free(b)?;
        let (state, reason) = conflict.as_ref().map_or(("added", String::new()), |held| {
            if held.section == b.section && same_action(&b.action, &held.action, commands) {
                ("set", String::new())
            } else {
                let state = if held.section == b.section { "taken" } else { "hidden" };
                (state, format!("runs {} in {}", held.action, held.section))
            }
        });
        outcomes.push(json!({"section": b.section, "key": b.key, "status": state, "reason": reason}));
    }
    Ok(json!({"bindings": outcomes}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commands() -> Vec<AppCommand> {
        serde_json::from_str(r#"[{"name":"pane.go","args":[{"name":"direction","required":true}]},{"name":"pane.resize","args":[{"name":"direction","required":true}]},{"name":"tab.move","args":[{"name":"by"}]},{"name":"terminal.select_all"},{"name":"terminal.search"},{"name":"pane.rename","args":[{"name":"name"}]}]"#).unwrap()
    }

    #[test]
    fn presets_are_valid_and_gated() {
        let commands = commands();
        assert_eq!(available(&commands).len(), 5);
        assert!(!available(&commands[0..1]).iter().any(|p| p.id == "pane-resize"));
        for p in available(&commands) {
            for b in p.bindings {
                assert_eq!(keys::problems(&b.section, &b.key), (vec![], vec![]));
                assert_eq!(keys::action_problem(&b.action, &commands, &[]), None);
            }
        }
    }

    #[test]
    fn apply_preserves_taken_and_hidden_keys() {
        let _guard = crate::testenv::isolated();
        let _ = std::fs::remove_dir_all(super::super::home());
        std::fs::create_dir_all(super::super::home()).unwrap();
        std::fs::write(keys::path(), r#"{"global":{"alt-shift-left":"tab.new"},"terminal":{"alt-shift-down":"terminal.copy"}}"#).unwrap();
        let result = apply("pane-resize", &commands()).unwrap();
        assert_eq!(result["bindings"][0]["status"], "taken");
        assert_eq!(result["bindings"][1]["status"], "hidden");
        assert_eq!(result["bindings"][2]["status"], "added");
        assert_eq!(keys::load().unwrap().iter().find(|b| b.key == "alt-shift-left").unwrap().action, "tab.new");
        let _ = std::fs::remove_dir_all(super::super::home());
    }

    #[test]
    fn already_set_bindings_are_not_conflicts_or_offered() {
        let _guard = crate::testenv::isolated();
        let _ = std::fs::remove_dir_all(super::super::home());
        std::fs::create_dir_all(super::super::home()).unwrap();
        let key = if cfg!(target_os = "macos") { "cmd-f" } else { "ctrl-shift-f" };
        let contents = json!({"terminal": {(key): "terminal.search"}});
        std::fs::write(keys::path(), contents.to_string()).unwrap();
        let commands = commands();
        let preset = available(&commands).into_iter().find(|p| p.id == "search").unwrap();
        assert_eq!(status(&preset.bindings[0], &keys::load().unwrap(), &commands).0, "set");
        let listed = list(&commands).unwrap();
        assert!(!listed["presets"].as_array().unwrap().iter().any(|p| p["id"] == "search"));
        assert_eq!(apply("search", &commands).unwrap()["bindings"][0]["status"], "set");
        assert!(same_action("pane.go:left", r#"{"args":{"direction":"left"},"command":"pane.go"}"#, &commands));
        assert!(!same_action("pane.go:left", r#"{"command":"pane.go","args":{"direction":"right"}}"#, &commands));
        let _ = std::fs::remove_dir_all(super::super::home());
    }

    #[test]
    fn presets_do_not_collide_with_default_keys() {
        let defaults = keys::app_defaults();
        for preset in available(&commands()) {
            for binding in &preset.bindings {
                let expected = if preset.id == "search" { "set" } else { "free" };
                assert_eq!(status(binding, &defaults, &commands()).0, expected, "{} {}", preset.id, binding.key);
            }
        }
    }
}
