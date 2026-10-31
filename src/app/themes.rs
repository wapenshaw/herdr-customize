//! Themes in the Herdr app: palettes written as its themes/<name>.json, the
//! theme its picker shows, and which theme files herdr-customize wrote, so
//! one the person edited is never written over.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{digest, home, read_object, read_record, state_dir, write_record};
use crate::fsutil;
use crate::theme::{self, Look, Palette, Saved};

/// The app's own themes, which have no file, and whether each is dark.
pub const PRESETS: [(&str, bool); 4] = [("graphite", true), ("ink", true), ("moss", true), ("paper", false)];

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

/// Reads an app theme file into a palette, for the preview.
pub fn parse(text: &str) -> Result<Palette> {
    #[derive(Deserialize)]
    struct File {
        background: String,
        foreground: String,
        #[serde(default)]
        selection: String,
        #[serde(default)]
        cursor: String,
        ansi: Vec<String>,
    }
    let f: File = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
    let colors: Vec<String> = f.ansi.iter().map(|c| theme::normalize_color(c)).collect::<Result<_>>()?;
    let Ok(colors) = <[String; 16]>::try_from(colors) else { bail!("ansi needs 16 colors") };
    let optional = |c: &str| if c.is_empty() { Ok(String::new()) } else { theme::normalize_color(c) };
    Ok(Palette {
        background: theme::normalize_color(&f.background)?,
        foreground: theme::normalize_color(&f.foreground)?,
        selection_background: optional(&f.selection)?,
        cursor_color: optional(&f.cursor)?,
        colors,
        overrides: BTreeMap::new(),
    })
}

/// The theme the app shows in each appearance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Current {
    pub dark: String,
    pub light: String,
}

/// What a "theme" value names: one theme, or one per appearance.
fn halves(value: &Value) -> Option<Current> {
    match value {
        Value::String(name) => Some(Current { dark: name.clone(), light: name.clone() }),
        Value::Object(o) => {
            let half = |k: &str| o.get(k).and_then(Value::as_str).map(String::from);
            Some(Current { dark: half("dark").unwrap_or_else(|| "graphite".into()), light: half("light").unwrap_or_else(|| "paper".into()) })
        }
        _ => None,
    }
}

/// The shown layout's theme: the picker's choice, else the layout's own,
/// else settings.json's, else graphite and paper.
pub fn current() -> Current {
    let home = home();
    let shared = read_object(&home.join("shared.json")).unwrap_or_default();
    let layout = shared.get("layout").and_then(Value::as_str).filter(|l| theme::is_valid_slug(l)).unwrap_or("main").to_string();
    let layouts = home.join("layouts");
    let sources = [layouts.join(".state").join(format!("{layout}.json")), layouts.join(format!("{layout}.json")), home.join("settings.json")];
    sources.iter().find_map(|path| halves(read_object(path).ok()?.get("theme")?)).unwrap_or_else(|| Current { dark: "graphite".into(), light: "paper".into() })
}

/// Where a theme in the app's list comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// One of the app's own themes.
    Preset,
    /// A theme file herdr-customize did not write: the app's, a mod's or the
    /// person's.
    File,
    /// A palette herdr-customize bundles.
    Bundled,
    /// A theme the person added to herdr-customize.
    Mine,
}

/// One row of the app's theme list.
#[derive(Debug, Clone)]
pub struct AppTheme {
    pub name: String,
    pub origin: Origin,
    /// None for presets, whose colors live in the app.
    pub look: Option<Look>,
    pub dark: bool,
    /// The palette is in themes/ already.
    pub installed: bool,
}

/// Presets, then theme files, then every palette, with warnings about
/// files that could not be read.
pub fn list() -> Result<(Vec<AppTheme>, Vec<String>)> {
    let mut warnings = Vec::new();
    let record: BTreeMap<String, String> = read_record(RECORD)?;
    let mut out: Vec<AppTheme> = PRESETS.iter().map(|&(name, dark)| AppTheme { name: name.into(), origin: Origin::Preset, look: None, dark, installed: true }).collect();
    let mut ours = Vec::new();
    let mut files: Vec<(String, String)> = match fs::read_dir(themes_dir()) {
        Ok(dir) => dir
            .filter_map(|e| {
                let path = e.ok()?.path();
                let name = path.file_stem()?.to_str()?.to_string();
                (path.extension()? == "json" && !name.starts_with('.')).then_some((name, fs::read_to_string(&path).unwrap_or_default()))
            })
            .collect(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e).context("read the app's themes folder"),
    };
    files.sort();
    for (name, text) in files {
        if record.get(&name) == Some(&digest(text.as_bytes())) {
            ours.push(name);
            continue;
        }
        match parse(&text) {
            Ok(pal) => out.push(AppTheme { dark: theme::is_dark(&pal.background), look: Some(Look::of(&pal)), name, origin: Origin::File, installed: true }),
            Err(e) => warnings.push(format!("themes/{name}.json: {e:#}")),
        }
    }
    let (library, more) = theme::library()?;
    warnings.extend(more);
    for item in library {
        if out.iter().any(|t| t.name == item.slug) {
            continue;
        }
        let look = theme::load_look(&item.slug).ok();
        let dark = look.as_ref().is_none_or(|l| l.dark);
        let origin = if item.user { Origin::Mine } else { Origin::Bundled };
        out.push(AppTheme { installed: ours.contains(&item.slug), name: item.slug, origin, look, dark });
    }
    Ok((out, warnings))
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

/// What install_all did, by palette.
#[derive(Debug, Default)]
pub struct Summary {
    pub written: usize,
    pub same: usize,
    pub kept: Vec<String>,
}

/// Writes every palette, bundled and the person's own, to the app.
pub fn install_all(force: bool) -> Result<Summary> {
    let (library, _) = theme::library()?;
    let _lock = lock()?;
    let mut record = read_record(RECORD)?;
    let mut summary = Summary::default();
    for item in library {
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

/// Shows name in the app; a palette not yet in themes/ is written first.
pub fn apply(name: &str, origin: Origin) -> Result<()> {
    if matches!(origin, Origin::Bundled | Origin::Mine) {
        install(name, false)?;
    }
    super::run("app.theme", &[("name", name)])
}

/// Back to the app's own themes in both appearances.
pub fn reset() -> Result<()> {
    super::run("app.theme", &[("name", "graphite")])?;
    super::run("app.theme", &[("name", "paper")])
}

/// Adds a Ghostty palette to the app as a theme file of the person's own.
pub fn add(name: &str, body: &str, overwrite: bool) -> Result<Saved> {
    theme::validate_theme_body(body)?;
    let slug = theme::slugify(name);
    if !theme::is_valid_slug(&slug) {
        bail!("invalid theme name: {name:?}");
    }
    if PRESETS.iter().any(|(p, _)| *p == slug) {
        bail!("{slug:?} is one of the app's own themes; pick another name");
    }
    let path = theme_file(&slug);
    if path.exists() && !overwrite {
        return Ok(Saved::Exists(slug));
    }
    fs::create_dir_all(themes_dir())?;
    let _lock = lock()?;
    fsutil::atomic_write(&path, to_json(&theme::parse_palette(body)?).as_bytes(), 0o644)?;
    // Written for the person, so later installs never replace it.
    let mut record: BTreeMap<String, String> = read_record(RECORD)?;
    if record.remove(&slug).is_some() {
        write_record(RECORD, &record)?;
    }
    Ok(Saved::Saved(slug))
}

/// Deletes a theme file from the app.
pub fn remove(name: &str) -> Result<()> {
    if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
        bail!("invalid theme name: {name:?}");
    }
    let _lock = lock()?;
    fs::remove_file(theme_file(name)).with_context(|| format!("delete themes/{name}.json"))?;
    let mut record: BTreeMap<String, String> = read_record(RECORD)?;
    if record.remove(name).is_some() {
        write_record(RECORD, &record)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::tests::BODY;

    // The app refuses a theme file with any key it does not know.
    #[test]
    fn json_round_trips_with_only_known_keys() {
        let pal = theme::parse_palette(BODY).unwrap();
        let text = to_json(&pal);
        let keys: Vec<String> = serde_json::from_str::<serde_json::Map<String, Value>>(&text).unwrap().keys().cloned().collect();
        assert_eq!(keys, ["background", "foreground", "selection", "cursor", "ansi"]);
        let back = parse(&text).unwrap();
        assert_eq!((back.background, back.colors), (pal.background, pal.colors));
        assert_eq!(back.cursor_color, "#cdd6f4", "no cursor-color falls back to the foreground");
        assert!(parse(r##"{"background":"#000000","foreground":"#ffffff","ansi":["#000000"]}"##).is_err());
    }

    // The picker's choice wins over the layout's and settings' own, and a
    // single name covers both appearances.
    #[test]
    fn current_follows_the_picker_then_the_layout() {
        let _guard = crate::ui::tests::isolated();
        let home = home();
        let _ = fs::remove_dir_all(&home);
        assert_eq!(current(), Current { dark: "graphite".into(), light: "paper".into() });
        fs::create_dir_all(home.join("layouts/.state")).unwrap();
        fs::write(home.join("shared.json"), r#"{"layout":"terminals"}"#).unwrap();
        fs::write(home.join("settings.json"), r#"{"theme":"moss"}"#).unwrap();
        assert_eq!(current().dark, "moss");
        fs::write(home.join("layouts/terminals.json"), r#"{"id":"root","theme":{"dark":"ink"}}"#).unwrap();
        assert_eq!(current(), Current { dark: "ink".into(), light: "paper".into() });
        fs::write(home.join("layouts/.state/terminals.json"), r#"{"theme":{"light":"x","dark":"y"}}"#).unwrap();
        assert_eq!(current(), Current { dark: "y".into(), light: "x".into() });
        let _ = fs::remove_dir_all(&home);
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
        let (themes, _) = list().unwrap();
        let t = themes.iter().find(|t| t.name == "dracula-default").unwrap();
        assert!(t.installed && t.origin == Origin::Bundled);
        // A theme the person added is theirs, even under a palette's name.
        add("dracula-default", BODY, true).unwrap();
        let (themes, _) = list().unwrap();
        assert_eq!(themes.iter().find(|t| t.name == "dracula-default").unwrap().origin, Origin::File);
        assert_eq!(install("dracula-default", false).unwrap(), Wrote::Kept);
        let _ = fs::remove_dir_all(home());
    }
}
