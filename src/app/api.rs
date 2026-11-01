//! The JSON commands behind the popup mod's native screen in the app. Each
//! prints one JSON object, or {"error": ...} and fails; the screen keeps no
//! state of its own beyond what it shows.

use std::collections::BTreeMap;

use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};

use super::keys::{self, Binding};
use super::mods::{self, Kind, Plan};
use super::themes;
use crate::{plugins, theme};

/// How many marketplace results one page holds.
const MARKET_PAGE: usize = 20;

/// Runs one command and prints its reply; the exit code says whether it
/// failed.
pub fn main(args: &[String]) -> i32 {
    match run(args) {
        Ok(value) => {
            println!("{value}");
            0
        }
        Err(e) => {
            println!("{}", json!({ "error": format!("{e:#}") }));
            1
        }
    }
}

/// Positional words, then --name value pairs; a name with no value is "true".
struct Args {
    words: Vec<String>,
    flags: BTreeMap<String, String>,
}

impl Args {
    fn parse(args: &[String]) -> Args {
        let (mut words, mut flags) = (Vec::new(), BTreeMap::new());
        let mut it = args.iter().peekable();
        while let Some(arg) = it.next() {
            match arg.strip_prefix("--") {
                Some(name) => {
                    let value = it.next_if(|v| !v.starts_with("--")).cloned().unwrap_or_else(|| "true".into());
                    flags.insert(name.to_string(), value);
                }
                None => words.push(arg.clone()),
            }
        }
        Args { words, flags }
    }

    fn word(&self, i: usize, what: &str) -> Result<&str> {
        self.words.get(i).map(String::as_str).ok_or_else(|| anyhow!("{what} is required"))
    }

    fn flag(&self, name: &str) -> &str {
        self.flags.get(name).map_or("", String::as_str)
    }

    fn need(&self, name: &str) -> Result<&str> {
        self.flags.get(name).map(String::as_str).ok_or_else(|| anyhow!("--{name} is required"))
    }
}

fn run(args: &[String]) -> Result<Value> {
    let (command, rest) = args.split_first().ok_or_else(|| anyhow!("no command"))?;
    let a = Args::parse(rest);
    match command.as_str() {
        "palettes" => palettes(),
        "theme-write" => {
            let wrote = themes::install(a.word(0, "a palette")?, false)?;
            Ok(json!({ "kept": wrote == themes::Wrote::Kept }))
        }
        "key-check" => key_check(&a),
        "key-set" => key_set(&a),
        "key-remove" => {
            keys::remove(a.need("section")?, a.need("key")?)?;
            Ok(json!({}))
        }
        "packages" => packages(),
        "market" => market(&a),
        "review" => Ok(json!({ "plans": mods::fetch(a.word(0, "owner/repo")?)?.iter().map(plan_json).collect::<Vec<_>>() })),
        "install" => install(&a),
        "remove" => Ok(json!({ "kept": mods::remove(a.need("repo")?, a.flag("subdir"))? })),
        other => bail!("unknown command {other:?}"),
    }
}

/// Every palette, with the colors a swatch needs.
fn palettes() -> Result<Value> {
    let (library, warnings) = theme::library()?;
    let mut out = Vec::new();
    for item in library {
        let pal = match theme::resolve_palette(&item.slug) {
            Ok((pal, _)) => pal,
            Err(e) => {
                out.push(json!({ "name": item.slug, "error": format!("{e:#}") }));
                continue;
            }
        };
        out.push(json!({
            "name": item.slug,
            "user": item.user,
            "featured": themes::FEATURED.contains(&item.slug.as_str()),
            "dark": theme::is_dark(&pal.background),
            "background": pal.background,
            "foreground": pal.foreground,
            "ansi": pal.colors,
        }));
    }
    Ok(json!({ "palettes": out, "warnings": warnings }))
}

/// What is wrong with binding key in section to action, what it takes from
/// programs, and what the key runs now (other than old).
fn check(a: &Args) -> Result<(Vec<String>, Vec<String>, Option<String>)> {
    let (section, key, action) = (a.need("section")?, a.need("key")?, a.flag("action"));
    let (mut errors, mut warnings) = keys::problems(section, key);
    let bindings = keys::load()?;
    if let Some(b) = keys::hidden_by(&bindings, section, key) {
        warnings.push(format!("{} runs {} in {}, which wins there; this works only elsewhere", b.key, b.action, b.section));
    }
    // Without the app's command list, any action passes.
    let commands = super::commands().unwrap_or_default();
    errors.extend(keys::action_problem(action, &commands, &mods::mod_names()));
    let old = a.flags.get("old").map(|o| keys::normalize(o));
    let held = bindings.into_iter().find(|b| b.section == section && keys::normalize(&b.key) == keys::normalize(key) && old.as_deref() != Some(keys::normalize(&b.key).as_str())).map(|b| b.action);
    Ok((errors, warnings, held))
}

fn key_check(a: &Args) -> Result<Value> {
    let (errors, warnings, held) = check(a)?;
    Ok(json!({ "errors": errors, "warnings": warnings, "held": held }))
}

/// Binds the key, refusing one another binding holds unless --replace.
fn key_set(a: &Args) -> Result<Value> {
    let (errors, warnings, held) = check(a)?;
    if let Some(e) = errors.first() {
        bail!("{e}");
    }
    if let Some(held) = held
        && a.flag("replace").is_empty()
    {
        bail!("{} already runs {held}", a.need("key")?);
    }
    let old = a.flags.get("old").map(String::as_str);
    keys::set(a.need("section")?, old, a.need("key")?, a.need("action")?)?;
    Ok(json!({ "warnings": warnings }))
}

fn packages() -> Result<Value> {
    let packages = mods::packages()?;
    let installed: Vec<Value> = packages
        .iter()
        .map(|p| json!({ "repo": p.repo, "subdir": p.subdir, "label": p.label(), "commit": p.commit, "mods": p.mods, "layouts": p.layouts, "themes": p.themes, "keys": p.keys }))
        .collect();
    let added: Vec<Value> = mods::others(&packages).into_iter().map(|(kind, name)| json!({ "kind": if kind == Kind::Mod { "mod" } else { "layout" }, "name": name })).collect();
    Ok(json!({ "installed": installed, "added": added, "shipped": super::shipped() }))
}

fn market(a: &Args) -> Result<Value> {
    let topic = if a.flag("topic") == "layouts" { mods::LAYOUT_TOPIC } else { mods::MOD_TOPIC };
    let sort = if a.flag("sort") == "updated" { "updated" } else { "stars" };
    let page = a.flag("page").parse().unwrap_or(1).max(1);
    let results = plugins::search(topic, a.flag("query"), sort, page, MARKET_PAGE)?;
    let repos: Vec<Value> = results.repos.iter().map(|r| json!({ "name": r.full_name, "stars": r.stars, "description": r.description })).collect();
    Ok(json!({ "total": results.total, "per_page": MARKET_PAGE, "repos": repos }))
}

fn binding_json(b: &Binding) -> Value {
    json!({ "section": b.section, "key": b.key, "action": b.action })
}

fn plan_json(p: &Plan) -> Value {
    json!({
        "repo": p.repo,
        "subdir": p.subdir,
        "label": p.label(),
        "commit": p.commit,
        "summary": p.summary,
        "update": p.update,
        "mods": p.mods,
        "layouts": p.layouts,
        "themes": p.themes,
        "keys": p.keys.iter().map(binding_json).collect::<Vec<_>>(),
        "key_clashes": p.key_clashes.iter().map(|(b, held)| json!({ "section": b.section, "key": b.key, "action": b.action, "held": held })).collect::<Vec<_>>(),
        "taken": p.taken,
        "edited": p.edited,
        "uses": p.uses,
    })
}

/// Installs the package the screen reviewed, refusing one that changed on
/// GitHub since, so what is written is what the person saw.
fn install(a: &Args) -> Result<Value> {
    let repo = a.need("repo")?;
    let subdir = a.flag("subdir");
    let plans = mods::fetch(&format!("{repo}/{subdir}"))?;
    let plan = plans.iter().find(|p| p.subdir == subdir).ok_or_else(|| anyhow!("{repo} has no package at {subdir:?}"))?;
    if plan.commit != a.need("commit")? {
        bail!("{} changed since the review; review it again", plan.label());
    }
    let report = mods::install(plan, !a.flag("take-keys").is_empty())?;
    let checks: Vec<Value> = report.checks.iter().map(|(name, error)| json!({ "name": name, "error": error })).collect();
    Ok(json!({ "checks": checks, "kept_keys": report.kept_keys }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::home;
    use std::fs;

    fn call(args: &[&str]) -> Result<Value> {
        run(&args.iter().map(ToString::to_string).collect::<Vec<_>>())
    }

    // A key is checked before it is written, and one another binding holds
    // is taken only when the screen asked the person first.
    #[test]
    fn keys_are_checked_then_set() {
        let _guard = crate::ui::tests::isolated();
        let _ = fs::remove_dir_all(home());
        fs::create_dir_all(home()).unwrap();
        fs::write(keys::path(), r#"{"global":{"ctrl-shift-t":"tab.new"}}"#).unwrap();
        let reply = call(&["key-check", "--section", "global", "--key", "hyper-1", "--action", "tab.nope"]).unwrap();
        assert_eq!(reply["errors"].as_array().unwrap().len(), 2, "{reply}");
        let reply = call(&["key-check", "--section", "global", "--key", "shift-ctrl-t", "--action", "tab.new"]).unwrap();
        assert_eq!(reply["held"], "tab.new");
        fs::write(keys::path(), r#"{"global":{"ctrl-shift-t":"tab.new"},"terminal":{"ctrl-shift-c":"terminal.copy"}}"#).unwrap();
        let reply = call(&["key-check", "--section", "global", "--key", "ctrl-shift-c", "--action", "tab.new"]).unwrap();
        assert!(reply["warnings"][0].as_str().unwrap().contains("terminal.copy in terminal"), "{reply}");
        fs::write(keys::path(), r#"{"global":{"ctrl-shift-t":"tab.new"}}"#).unwrap();
        assert!(call(&["key-set", "--section", "global", "--key", "ctrl-shift-t", "--action", "app.theme:ink"]).unwrap_err().to_string().contains("already runs tab.new"));
        call(&["key-set", "--section", "global", "--key", "ctrl-shift-t", "--action", "app.theme:ink", "--replace"]).unwrap();
        call(&["key-set", "--section", "global", "--old", "ctrl-shift-t", "--key", "alt-1", "--action", "app.theme:ink"]).unwrap();
        call(&["key-remove", "--section", "global", "--key", "alt-1"]).unwrap();
        assert_eq!(fs::read_to_string(keys::path()).unwrap().trim(), "{\n  \"global\": {}\n}");
        let _ = fs::remove_dir_all(home());
    }

    // What the screen showed in the review is what gets installed: a
    // package that moved on since is refused.
    #[test]
    fn review_then_install() {
        let _guard = crate::ui::tests::isolated();
        let _ = fs::remove_dir_all(home());
        fs::create_dir_all(home()).unwrap();
        let base = std::path::PathBuf::from(crate::config::env("HERDR_CUSTOMIZE_GIT_BASE").unwrap());
        mods::tests::repo(&base, "o/board", &[("mods/board/main.js", "fetch(x)"), ("README.md", "# Board\n\nA board.\n")]);
        let reply = call(&["review", "o/board"]).unwrap();
        let plan = &reply["plans"][0];
        assert_eq!((plan["summary"].as_str(), plan["uses"][0].as_str()), (Some("A board."), Some("reaches the network")));
        assert!(!home().join("mods/board").exists(), "a review writes nothing");
        assert!(call(&["install", "--repo", "o/board", "--subdir", "", "--commit", "0000"]).unwrap_err().to_string().contains("review it again"));
        let reply = call(&["install", "--repo", "o/board", "--subdir", "", "--commit", plan["commit"].as_str().unwrap()]).unwrap();
        assert_eq!(reply["checks"][0]["name"], "board");
        assert_eq!(call(&["packages"]).unwrap()["installed"][0]["label"], "o/board");
        call(&["remove", "--repo", "o/board"]).unwrap();
        assert!(!home().join("mods/board/main.js").exists());
        let _ = fs::remove_dir_all(home());
    }

    #[test]
    fn palettes_carry_their_swatch() {
        let _guard = crate::ui::tests::isolated();
        let reply = call(&["palettes"]).unwrap();
        let all = reply["palettes"].as_array().unwrap();
        let dracula = all.iter().find(|p| p["name"] == "dracula-default").unwrap();
        assert!(dracula["dark"].as_bool().unwrap() && dracula["featured"].as_bool().unwrap() && dracula["ansi"].as_array().unwrap().len() == 16);
        assert!(call(&["nope"]).is_err());
    }
}
