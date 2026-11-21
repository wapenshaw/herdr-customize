//! herdr-customize: the Customize screen for the Herdr desktop app, and the
//! commands that install it and write its themes.

mod app;
mod fsutil;
mod proc;
#[cfg(test)]
mod testenv;
mod theme;

use anyhow::{Result, bail};

const USAGE: &str = "Usage: herdr-customize app <command>

  app install [key]     add the Customize screen to the Herdr app, on ctrl-shift-y, and its themes
  app themes [--force]  write the featured palettes to the app
  app apply <name>      show a theme in the app
  app reset             back to the app's own themes

The Herdr terminal (TUI) plugin was retired in 1.0.0; v0.10.0 is its last release.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

/// An environment variable, treating an empty one as unset.
pub fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        None | Some("help" | "--help" | "-h") => println!("{USAGE}"),
        Some("app") => run_app(&args[1..])?,
        Some(other) => bail!("unknown command: {other} (see herdr-customize help)"),
    }
    Ok(())
}

fn run_app(mut args: &[String]) -> Result<()> {
    if let [flag, dir, rest @ ..] = args
        && flag == "--home"
    {
        // SAFETY: no other thread has started yet.
        unsafe { std::env::set_var("HERDR_APP_HOME", dir) };
        args = rest;
    }
    if let [api, rest @ ..] = args
        && api == "api"
    {
        std::process::exit(app::api::main(rest));
    }
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["install", key @ ..] if key.len() < 2 => {
            match app::install_popup(key.first().copied())? {
                app::Bound::Added(key) => println!("Added the popup to the Herdr app; press {key} there."),
                app::Bound::Already(key) => println!("Updated the popup in the Herdr app; {key} opens it."),
            }
            write_app_themes(false)?;
        }
        ["themes", force @ ..] if force.is_empty() || force == ["--force"] => write_app_themes(!force.is_empty())?,
        ["apply", name] => app::themes::apply(name)?,
        ["reset"] => app::themes::reset()?,
        _ => bail!("wrong arguments (see herdr-customize help)"),
    }
    Ok(())
}

fn write_app_themes(force: bool) -> Result<()> {
    let s = app::themes::install_featured(force)?;
    println!("{} themes written, {} already there, in {}", s.written, s.same, app::themes::themes_dir().display());
    if !s.kept.is_empty() {
        println!("Kept your edits to {} (--force replaces them)", s.kept.join(", "));
    }
    Ok(())
}
