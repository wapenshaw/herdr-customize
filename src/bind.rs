//! Binds a key to the popup from the command line, since Herdr does not bind
//! keys from plugin manifests and a new install has no way in otherwise.

use std::collections::HashSet;
use std::io::IsTerminal;

use anyhow::{Result, bail};

use crate::config::{self, Doc, Field, Store, Value};
use crate::keys::{self, Binding};
use crate::schema::{self, Reference};
use crate::{herdr, plugins, theme};

const DEFAULT_KEY: &str = "prefix+t";

const COMMAND_PATH: [&str; 2] = ["keys", "command"];

fn command_path() -> Vec<String> {
    COMMAND_PATH.iter().map(ToString::to_string).collect()
}

fn text(entry: &toml::Table, name: &str) -> String {
    entry.get(name).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn open_action() -> String {
    format!("{}.open", theme::PLUGIN_ID)
}

#[derive(Debug, PartialEq)]
enum Plan {
    /// The popup already opens from this key.
    Bound(String),
    /// Append a binding for this key, in canonical form.
    Add(String),
    /// Rewrite command entry index, whose action no installed plugin has, to
    /// open the popup on key.
    Replace { index: usize, key: String, old: String },
}

/// Every key the config uses: Herdr's actions with their defaults, and each
/// [[keys.command]] entry but the skipped ones.
fn existing(doc: &Doc, reference: &Reference, skip: &[usize]) -> Vec<Binding> {
    let table = doc.decoded();
    let mut out = Vec::new();
    for k in reference.section("keys").map(|s| s.keys.as_slice()).unwrap_or_default() {
        let action = k.key.trim_start_matches("keys.");
        let current = config::lookup(&table, &k.path()).cloned().or_else(|| k.default_value());
        out.push(Binding { owner: k.key.clone(), keys: keys::indexed(action, keys::values(current.as_ref())), navigate: keys::navigate(action) });
    }
    for (i, entry) in doc.array_table(&command_path()).iter().enumerate() {
        if skip.contains(&i) {
            continue;
        }
        let name = [text(entry, "description"), text(entry, "command")].into_iter().find(|s| !s.is_empty()).unwrap_or_default();
        out.push(Binding { owner: format!("command {} ({name})", i + 1), keys: keys::values(entry.get("key")), navigate: false });
    }
    out
}

/// Decides what bind does. Without a key, any of this plugin's actions that
/// has one is enough, since tab moves between pages. A key that breaks
/// Herdr's rules or that another binding uses is refused.
fn plan(doc: &Doc, reference: &Reference, key: Option<&str>, installed: Option<&HashSet<String>>) -> Result<Plan> {
    let prefix = format!("{}.", theme::PLUGIN_ID);
    let ours: Vec<(String, String)> = doc
        .array_table(&command_path())
        .iter()
        .filter(|e| text(e, "type") == "plugin_action" && text(e, "command").starts_with(&prefix) && !text(e, "key").is_empty())
        .map(|e| (text(e, "key"), text(e, "command")))
        .collect();
    let asked = match key {
        Some(k) => k,
        None if !ours.is_empty() => return Ok(Plan::Bound(ours[0].0.clone())),
        None => DEFAULT_KEY,
    };
    let combo = keys::parse(asked)?.to_string();
    if let Some(problem) = keys::problems("command", &combo).into_iter().next() {
        bail!("{problem}");
    }
    if ours.iter().any(|(k, c)| *c == open_action() && keys::parse(k).is_ok_and(|p| p.to_string() == combo)) {
        return Ok(Plan::Bound(combo));
    }
    // A binding left by an uninstalled plugin, such as one this plugin
    // replaces, would otherwise block the popup's only way in.
    let dead: Vec<(usize, String)> = match installed {
        Some(installed) => doc
            .array_table(&command_path())
            .iter()
            .enumerate()
            .filter(|(_, e)| text(e, "type") == "plugin_action" && keys::parse(&text(e, "key")).is_ok_and(|p| p.to_string() == combo) && !plugins::has_action(installed, &text(e, "command")))
            .map(|(i, e)| (i, text(e, "command")))
            .collect(),
        None => Vec::new(),
    };
    let skip: Vec<usize> = dead.iter().map(|(i, _)| *i).collect();
    let me = Binding { owner: theme::PLUGIN_ID.into(), keys: vec![combo.clone()], navigate: false };
    let mut all = existing(doc, reference, &skip);
    all.push(me.clone());
    let clashes = keys::clashes(&keys::conflicts(&all), &me);
    if !clashes.is_empty() {
        bail!("{combo} is already used by {}; pick another key with: herdr-customize bind <key>", clashes.join(", "));
    }
    Ok(match dead.into_iter().next() {
        Some((index, old)) => Plan::Replace { index, key: combo, old },
        None => Plan::Add(combo),
    })
}

/// Adds the binding through store, so herdr config check gates it and Herdr
/// reloads, and returns what happened in a sentence.
pub fn bind(store: &Store, key: Option<&str>) -> Result<String> {
    let reference = schema::bundled();
    // Without the plugin list, no binding counts as left behind.
    let installed = plugins::list().ok().map(|p| plugins::action_names(&p));
    let fields = |k: &str| -> Vec<Field> {
        vec![
            ("key".into(), Value::String(k.into())),
            ("type".into(), Value::String("plugin_action".into())),
            ("command".into(), Value::String(open_action())),
            ("description".into(), Value::String("Herdr Customize".into())),
        ]
    };
    let mut planned = None;
    let saved = store.update(|doc| {
        let p = plan(doc, &reference, key, installed.as_ref())?;
        match &p {
            Plan::Add(k) => doc.append_array_table(&command_path(), &fields(k))?,
            Plan::Replace { index, key, .. } => doc.update_array_table(&command_path(), *index, &fields(key))?,
            Plan::Bound(_) => {}
        }
        planned = Some(p);
        Ok(())
    })?;
    let msg = match planned {
        Some(Plan::Add(k)) => format!("Bound {k} to the customize popup."),
        Some(Plan::Replace { key, old, .. }) => format!("Bound {key} to the customize popup in place of {old}, which no installed plugin has."),
        Some(Plan::Bound(k)) => return Ok(format!("The customize popup already opens with {k}.")),
        None => unreachable!("update runs its edit or fails"),
    };
    Ok(match saved.reload_err {
        Some(_) => format!("{msg} Herdr did not reload; press prefix+shift+r."),
        None => msg,
    })
}

/// Runs bind and reports the outcome, also as a Herdr notification when there
/// is no terminal to print to (a plugin action's output goes to its log).
pub fn run(key: Option<&str>) -> Result<()> {
    let result = bind(&Store::default_store(), key);
    if !std::io::stdout().is_terminal() {
        let (title, body) = match &result {
            Ok(msg) => ("Herdr Customize", msg.clone()),
            Err(e) => ("Herdr Customize: bind failed", format!("{e:#}")),
        };
        let _ = herdr::run(&["notification", "show", title, "--body", &body]);
    }
    println!("{}", result?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan_for(config: &str, key: Option<&str>) -> Result<Plan> {
        plan_with(config, key, None)
    }

    fn plan_with(config: &str, key: Option<&str>, installed: Option<&[&str]>) -> Result<Plan> {
        let installed: Option<HashSet<String>> = installed.map(|a| a.iter().map(ToString::to_string).collect());
        plan(&Doc::parse(config).unwrap(), &schema::bundled(), key, installed.as_ref())
    }

    fn command(key: &str, command: &str) -> String {
        format!("[[keys.command]]\nkey = \"{key}\"\ntype = \"plugin_action\"\ncommand = \"{command}\"\n")
    }

    #[test]
    fn defaults_to_prefix_t() {
        assert_eq!(plan_for("", None).unwrap(), Plan::Add("prefix+t".into()));
        assert_eq!(plan_for("", Some("Alt+T")).unwrap(), Plan::Add("alt+shift+t".into()));
    }

    #[test]
    fn any_binding_to_the_popup_is_enough() {
        // Running bind again, or after binding another page, must not add a second key.
        assert_eq!(plan_for(&command("prefix+k", "herdr-customize.keys"), None).unwrap(), Plan::Bound("prefix+k".into()));
        assert_eq!(plan_for(&command("prefix+t", "herdr-customize.open"), Some("prefix+t")).unwrap(), Plan::Bound("prefix+t".into()));
        assert_eq!(plan_for(&command("prefix+t", "herdr-customize.open"), Some("alt+t")).unwrap(), Plan::Add("alt+t".into()));
    }

    #[test]
    fn refuses_a_taken_key() {
        let err = plan_for(&command("prefix+t", "herdr-theme-picker.open"), None).unwrap_err().to_string();
        assert!(err.contains("command 1 (herdr-theme-picker.open)"), "{err}");
        // A default Herdr binding counts until the config moves it elsewhere.
        assert!(plan_for("", Some("prefix+shift+t")).unwrap_err().to_string().contains("keys.rename_tab"));
        assert_eq!(plan_for("[keys]\nrename_tab = \"prefix+r\"\n", Some("prefix+shift+t")).unwrap(), Plan::Add("prefix+shift+t".into()));
    }

    #[test]
    fn refuses_a_key_that_intercepts_typing() {
        assert!(plan_for("", Some("t")).unwrap_err().to_string().contains("intercepts typing"));
        assert!(plan_for("", Some("prefix+zz")).is_err());
    }

    // A key left by an uninstalled plugin, as after switching from Theme
    // Picker, must not block the first bind: the popup has no other way in.
    #[test]
    fn takes_over_a_key_whose_action_is_gone() {
        let config = command("prefix+t", "herdr-theme-picker.open");
        let replace = Plan::Replace { index: 0, key: "prefix+t".into(), old: "herdr-theme-picker.open".into() };
        assert_eq!(plan_with(&config, None, Some(&["herdr-customize.open"])).unwrap(), replace);
        assert!(plan_with(&config, None, Some(&["herdr-theme-picker.open"])).is_err());
        // An unqualified id runs the one installed action that has it.
        assert!(plan_with(&command("prefix+t", "open"), None, Some(&["herdr-theme-picker.open"])).is_err());
    }

    // The dead binding is rewritten in place, so the config keeps one entry
    // for the key and passes Herdr's check.
    #[test]
    fn bind_rewrites_the_dead_binding() {
        let _env = crate::ui::tests::isolated();
        let bin = std::path::PathBuf::from(std::env::var("HERDR_BIN_PATH").unwrap());
        std::fs::write(bin.with_file_name("plugins.json"), r#"{"result":{"plugins":[]}}"#).unwrap();
        let cfg = config::path();
        std::fs::write(&cfg, format!("{}description = \"theme picker\"\n", command("prefix+t", "herdr-theme-picker.open"))).unwrap();
        let msg = bind(&Store::default_store(), None).unwrap();
        assert!(msg.contains("in place of herdr-theme-picker.open"), "{msg}");
        let written = std::fs::read_to_string(&cfg).unwrap();
        assert_eq!(written.matches("[[keys.command]]").count(), 1, "{written}");
        assert!(written.contains("command = \"herdr-customize.open\"") && !written.contains("theme picker"), "{written}");
    }
}
