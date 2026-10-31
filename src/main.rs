//! herdr-customize: themes, plugins, keybindings and settings for Herdr in
//! one popup.

mod app;
mod bind;
mod config;
mod fsutil;
mod herdr;
mod keys;
mod pluginconfig;
mod plugins;
mod proc;
mod schema;
mod terminal;
mod theme;
mod ui;

use anyhow::{Context, Result, bail};

const USAGE: &str = "Usage: herdr-customize [command]

  ui [settings|keys|themes|plugins]  open the customize popup (default: settings)
  open [page]                        open the popup as a Herdr plugin pane
  bind [key]                         bind a key to the popup (default: prefix+t)
  apply <slug>                       apply a theme
  reset                              go back to Herdr's built-in theme
  sync [--client <pid>]              re-send the applied colors to local clients
  sync --startup                     the same once the first client appears
  preview <slug>                     print a theme preview

The Herdr app (herdr-app) keeps its own themes, keys and mods:

  app [--home DIR] ui [themes|keys|mods]  open the popup for the app
  app install [key]                  add the popup to the app, on ctrl-shift-m
  app themes [--force]               write every palette to the app's themes
  app apply <name>                   show a theme in the app
  app reset                          back to the app's own themes

Themes are written to this machine's Herdr config. Clients attached with
herdr --remote use their own machine's config: run herdr-customize there.
Browse 400+ additional themes at https://terminalcolors.com";

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        args.push("ui".into());
    }
    if let Err(e) = run(&args) {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

fn page(args: &[String]) -> Result<&str> {
    let page = args.get(1).map_or(ui::PAGES[0], String::as_str);
    if !ui::PAGES.contains(&page) {
        bail!("unknown page {page:?} (want one of {})", ui::PAGES.join(", "));
    }
    Ok(page)
}

fn run(args: &[String]) -> Result<()> {
    let arg = |i: usize| args.get(i).map(String::as_str);
    match (args[0].as_str(), args.len()) {
        ("help" | "--help" | "-h", _) => println!("{USAGE}"),
        ("ui", _) => ui::run(page(args)?)?,
        ("open", _) => {
            let page = page(args)?;
            let status = herdr::command(["plugin", "pane", "open", "--plugin", theme::PLUGIN_ID, "--entrypoint", page, "--placement", "popup", "--focus"]).status();
            if !status.context("open the customize popup")?.success() {
                bail!("open the customize popup: herdr failed");
            }
        }
        ("bind", 1 | 2) => bind::run(arg(1))?,
        ("sync", 2) if arg(1) == Some("--startup") => theme::sync_at_startup()?,
        ("sync", 1 | 3) => {
            let client = match (arg(1), arg(2)) {
                (Some("--client"), Some(pid)) => pid.to_string(),
                (None, _) => std::env::var("HERDR_CUSTOMIZE_CLIENT_PID").unwrap_or_default(),
                _ => bail!("usage: herdr-customize sync [--client <pid>]"),
            };
            if let Err(e) = theme::sync_applied_theme(&client) {
                if terminal::is_unavailable(&e) {
                    bail!("no Herdr client terminal found on this machine");
                }
                return Err(e);
            }
        }
        ("invoke-after-close", 3) => {
            let pid = args[1].parse().with_context(|| format!("invalid pid {:?}", args[1]))?;
            plugins::run_invoke_after_close(pid, &args[2])?;
        }
        ("herdr-run", _) => std::process::exit(plugins::run_interactive(&args[1..])),
        // Internal: writes colors into one client's console (see terminal::windows).
        #[cfg(windows)]
        ("terminal-sync", 2) => std::process::exit(terminal::run_helper(&args[1])),
        ("preview", 2) => print!("{}", theme::preview::render_swatch(&theme::load_look(&args[1])?.tokens, &args[1])),
        ("app", _) => run_app(&args[1..])?,
        ("apply", 2) => theme::apply_theme(&args[1])?,
        ("reset", 1) => theme::reset_theme()?,
        ("bind" | "sync" | "invoke-after-close" | "preview" | "apply" | "reset", _) => bail!("wrong arguments (see herdr-customize help)"),
        (other, _) => bail!("unknown command: {other} (see herdr-customize help)"),
    }
    Ok(())
}

/// The commands for the Herdr app, which never touch Herdr's own config.
fn run_app(mut args: &[String]) -> Result<()> {
    if let [flag, dir, rest @ ..] = args
        && flag == "--home"
    {
        // SAFETY: no other thread has started yet.
        unsafe { std::env::set_var("HERDR_APP_HOME", dir) };
        args = rest;
    }
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        [] | ["ui"] => ui::run_herdr_app(ui::APP_PAGES[0])?,
        ["ui", page] if ui::APP_PAGES.contains(page) => ui::run_herdr_app(page)?,
        ["ui", page] => bail!("unknown page {page:?} (want one of {})", ui::APP_PAGES.join(", ")),
        ["install", key @ ..] if key.len() < 2 => match app::install_popup(key.first().copied())? {
            app::Bound::Added(key) => println!("Added the popup to the Herdr app; press {key} there."),
            app::Bound::Already(key) => println!("Updated the popup in the Herdr app; {key} opens it."),
        },
        ["themes", force @ ..] if force.is_empty() || force == ["--force"] => {
            let s = app::themes::install_all(!force.is_empty())?;
            println!("{} written, {} already there, in {}", s.written, s.same, app::themes::themes_dir().display());
            if !s.kept.is_empty() {
                println!("Kept your edits to {} (--force replaces them)", s.kept.join(", "));
            }
        }
        ["apply", name] => {
            let (themes, _) = app::themes::list()?;
            let t = themes.iter().find(|t| t.name == *name).with_context(|| format!("no theme {name:?}"))?;
            app::themes::apply(&t.name, t.origin)?;
        }
        ["reset"] => app::themes::reset()?,
        _ => bail!("wrong arguments (see herdr-customize help)"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The key `bind` adds runs the manifest's open action, so both must land on the first tab.
    #[test]
    fn open_lands_on_the_first_page() {
        assert_eq!(page(&["ui".into()]).unwrap(), ui::PAGES[0]);
        let manifest: toml_edit::DocumentMut = include_str!("../herdr-plugin.toml").parse().unwrap();
        let actions = manifest["actions"].as_array_of_tables().unwrap();
        let open = actions.iter().find(|a| a["id"].as_str() == Some("open")).unwrap();
        let command = open["command"].as_array().unwrap();
        assert_eq!(command.get(command.len() - 1).and_then(|v| v.as_str()), Some(ui::PAGES[0]));
    }
}
