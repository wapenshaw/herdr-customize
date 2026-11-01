//! Palettes written as the Herdr app's themes/<name>.json, which its
//! Settings > Look lists by themselves, and a record of which files
//! herdr-customize wrote, so one the person edited is never written over.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::json;

use super::{digest, home, read_record, state_dir, write_record};
use crate::fsutil;
use crate::theme::{self, Palette};

/// The palettes written to the app. Settings > Look draws a tile per theme
/// file on every render and fails with all of them, so only these.
pub const FEATURED: [&str; 26] = [
    "dracula-default",
    "gruvbox-dark",
    "gruvbox-light",
    "nord-default",
    "tokyo-night-default",
    "tokyo-night-storm",
    "tokyo-night-day",
    "catppuccin-mocha",
    "catppuccin-macchiato",
    "catppuccin-frappe",
    "catppuccin-latte",
    "solarized-dark",
    "solarized-light",
    "one-dark",
    "one-light",
    "everforest-dark",
    "everforest-light",
    "rose-pine-moon",
    "rose-pine-dawn",
    "kanagawa-wave",
    "kanagawa-dragon",
    "ayu-dark",
    "ayu-mirage",
    "ayu-light",
    "github-dark",
    "github-light",
];

/// Theme file name → digest of what herdr-customize last wrote there.
const RECORD: &str = "themes.json";

pub fn themes_dir() -> PathBuf {
    home().join("themes")
}

fn theme_file(name: &str) -> PathBuf {
    themes_dir().join(format!("{name}.json"))
}

/// A palette as the app's theme file. The app refuses a file with keys it
/// does not know, so nothing else goes in.
pub fn to_json(pal: &Palette) -> String {
    let pick = |v: &str, fallback: &str| if v.is_empty() { fallback.to_string() } else { v.to_string() };
    let value = json!({
        "background": pal.background,
        "foreground": pal.foreground,
        "selection": pick(&pal.selection_background, &pal.colors[8]),
        "cursor": pick(&pal.cursor_color, &pal.foreground),
        "ansi": pal.colors,
    });
    serde_json::to_string_pretty(&value).unwrap_or_default() + "\n"
}

/// What install did with one palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wrote {
    Written,
    /// The file already held this palette.
    Same,
    /// The file was edited, or is someone else's, and was left alone.
    Kept,
}

/// Holds the record's lock while it is read and written.
fn lock() -> Result<fs::File> {
    fs::create_dir_all(state_dir())?;
    fsutil::lock(&state_dir().join("themes.lock")).context("lock the theme record")
}

fn install_locked(slug: &str, force: bool, record: &mut BTreeMap<String, String>) -> Result<Wrote> {
    let text = to_json(&theme::resolve_palette(slug)?.0);
    let path = theme_file(slug);
    match fs::read_to_string(&path) {
        Ok(old) if old == text => {
            record.insert(slug.into(), digest(text.as_bytes()));
            return Ok(Wrote::Same);
        }
        Ok(old) if !force && record.get(slug) != Some(&digest(old.as_bytes())) => return Ok(Wrote::Kept),
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => fs::create_dir_all(themes_dir())?,
        Err(e) => return Err(e.into()),
    }
    fsutil::atomic_write(&path, text.as_bytes(), 0o644)?;
    record.insert(slug.into(), digest(text.as_bytes()));
    Ok(Wrote::Written)
}

/// Writes slug's palette to themes/<slug>.json. A file changed since
/// herdr-customize wrote it, or one it never wrote, stays unless force.
pub fn install(slug: &str, force: bool) -> Result<Wrote> {
    let _lock = lock()?;
    let mut record = read_record(RECORD)?;
    let wrote = install_locked(slug, force, &mut record)?;
    write_record(RECORD, &record)?;
    Ok(wrote)
}

/// What install_featured did, by palette.
#[derive(Debug, Default)]
pub struct Summary {
    pub written: usize,
    pub same: usize,
    pub kept: Vec<String>,
}

/// Writes the FEATURED palettes and the person's own to the app.
pub fn install_featured(force: bool) -> Result<Summary> {
    let (library, _) = theme::library()?;
    let _lock = lock()?;
    let mut record = read_record(RECORD)?;
    let mut summary = Summary::default();
    for item in library.into_iter().filter(|i| i.user || FEATURED.contains(&i.slug.as_str())) {
        match install_locked(&item.slug, force, &mut record) {
            Ok(Wrote::Written) => summary.written += 1,
            Ok(Wrote::Same) => summary.same += 1,
            Ok(Wrote::Kept) => summary.kept.push(item.slug),
            Err(e) => {
                write_record(RECORD, &record)?;
                return Err(e.context(format!("write {}", item.slug)));
            }
        }
    }
    write_record(RECORD, &record)?;
    Ok(summary)
}

/// Shows name in the app; a palette is written to themes/ first.
pub fn apply(name: &str) -> Result<()> {
    if theme::library()?.0.iter().any(|i| i.slug == name) {
        install(name, false)?;
    }
    super::run("app.theme", &[("name", name)])
}

/// Back to the app's own themes in both appearances.
pub fn reset() -> Result<()> {
    super::run("app.theme", &[("name", "graphite")])?;
    super::run("app.theme", &[("name", "paper")])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::tests::BODY;

    // The app refuses a theme file with any key it does not know.
    #[test]
    fn json_has_only_known_keys() {
        let text = to_json(&theme::parse_palette(BODY).unwrap());
        let value: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text).unwrap();
        assert_eq!(value.keys().collect::<Vec<_>>(), ["background", "foreground", "selection", "cursor", "ansi"]);
        assert_eq!(value["cursor"], "#cdd6f4", "no cursor-color falls back to the foreground");
    }
    // A palette edited in the app's folder stays; one herdr-customize wrote
    // and nobody touched is brought up to date.
    #[test]
    fn install_keeps_edited_files() {
        let _guard = crate::ui::tests::isolated();
        let _ = fs::remove_dir_all(home());
        assert_eq!(install("dracula-default", false).unwrap(), Wrote::Written);
        assert_eq!(install("dracula-default", false).unwrap(), Wrote::Same);
        let path = theme_file("dracula-default");
        fs::write(&path, "{}").unwrap();
        assert_eq!(install("dracula-default", false).unwrap(), Wrote::Kept);
        assert_eq!(fs::read_to_string(&path).unwrap(), "{}");
        assert_eq!(install("dracula-default", true).unwrap(), Wrote::Written);
        // An outdated file of ours, recorded by its digest, is replaced.
        fs::write(&path, "old").unwrap();
        write_record(RECORD, &BTreeMap::from([("dracula-default".to_string(), digest(b"old"))])).unwrap();
        assert_eq!(install("dracula-default", false).unwrap(), Wrote::Written);
        let all = install_featured(false).unwrap();
        assert_eq!((all.written, all.same, all.kept.len()), (FEATURED.len() - 1, 1, 0));
        let _ = fs::remove_dir_all(home());
    }
}
