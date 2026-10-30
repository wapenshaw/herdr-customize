//! herdr-customize: themes, plugins, keybindings and settings for Herdr in
//! one popup.

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
  sync [--client <pid>]              re-send the applied colors to local clients
  sync --startup                     the same once the first client appears
  preview <slug>                     print a theme preview

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
        ("apply", 2) => theme::apply_theme(&args[1])?,
        ("bind" | "sync" | "invoke-after-close" | "preview" | "apply", _) => bail!("wrong arguments (see herdr-customize help)"),
        (other, _) => bail!("unknown command: {other} (see herdr-customize help)"),
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
