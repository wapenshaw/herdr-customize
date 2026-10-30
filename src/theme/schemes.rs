//! Scheme files that terminal emulators load on start, so new windows open in
//! the applied theme. docs/themes.md lists each file and what it takes to use.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::{Palette, parse_rgb};
use crate::config::{self, env};
use crate::fsutil;

/// A terminal's scheme file and how it is written.
struct Target {
    terminal: &'static str,
    file: PathBuf,
    /// A folder the terminal reads other apps' files from: the file is then
    /// written whenever it exists. Without one, the file is in the user's own
    /// config and is only refreshed once they have created it.
    app_dir: Option<PathBuf>,
    render: fn(&Palette, &Path) -> Result<Vec<u8>>,
}

/// The eight ANSI color names in palette order; the bright ones follow.
const ANSI: [&str; 8] = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];

fn targets() -> Vec<Target> {
    let home = config::home();
    let config_home = env("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| home.as_ref().map(|h| h.join(".config")));
    let mut out = Vec::new();
    let own = |terminal, file: PathBuf, render| Target { terminal, file, app_dir: None, render };
    if let Some(dir) = &config_home {
        out.push(own("Ghostty", dir.join("ghostty").join("herdr-theme"), ghostty as fn(&Palette, &Path) -> Result<Vec<u8>>));
        out.push(own("kitty", dir.join("kitty").join("herdr-theme.conf"), kitty));
        out.push(own("Alacritty", dir.join("alacritty").join("herdr-theme.toml"), alacritty));
        out.push(own("WezTerm", dir.join("wezterm").join("colors").join("herdr.toml"), wezterm));
    }
    if let Some(dir) = env("APPDATA").map(PathBuf::from) {
        out.push(own("Ghostty", dir.join("ghostty").join("herdr-theme"), ghostty));
        out.push(own("Alacritty", dir.join("alacritty").join("herdr-theme.toml"), alacritty));
    }
    if let Some(home) = &home {
        let iterm = home.join("Library").join("Application Support").join("iTerm2");
        out.push(Target { terminal: "iTerm2", file: iterm.join("DynamicProfiles").join("herdr-customize.json"), app_dir: Some(iterm), render: iterm2 });
    }
    if let Some(local) = env("LOCALAPPDATA") {
        let wt = Path::new(&local).join("Microsoft").join("Windows Terminal");
        out.push(Target { terminal: "Windows Terminal", file: wt.join("Fragments").join("herdr-customize").join("herdr-theme.json"), app_dir: Some(wt), render: windows_terminal });
    }
    out
}

/// Writes the palette into every scheme file a terminal on this machine will
/// load, returning one message per file that could not be written.
pub(super) fn refresh(pal: &Palette, palette_path: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    for t in targets() {
        let wanted = match &t.app_dir {
            Some(dir) => dir.is_dir(),
            None => t.file.is_file(),
        };
        if !wanted {
            continue;
        }
        let written = (|| {
            if let (Some(_), Some(dir)) = (&t.app_dir, t.file.parent()) {
                fs::create_dir_all(dir)?;
            }
            fsutil::atomic_write(&t.file, &(t.render)(pal, palette_path)?, 0o644)
        })();
        if let Err(e) = written {
            errors.push(format!("{} scheme: {e:#}", t.terminal));
        }
    }
    errors
}

/// Bundled and added palettes are already Ghostty themes.
fn ghostty(_: &Palette, palette_path: &Path) -> Result<Vec<u8>> {
    Ok(fs::read(palette_path)?)
}

fn kitty(pal: &Palette, _: &Path) -> Result<Vec<u8>> {
    let mut out = format!("foreground {}\nbackground {}\n", pal.foreground, pal.background);
    for (key, color) in [("cursor", &pal.cursor_color), ("selection_background", &pal.selection_background)] {
        if !color.is_empty() {
            let _ = writeln!(out, "{key} {color}");
        }
    }
    for (i, color) in pal.colors.iter().enumerate() {
        let _ = writeln!(out, "color{i} {color}");
    }
    Ok(out.into_bytes())
}

fn alacritty(pal: &Palette, _: &Path) -> Result<Vec<u8>> {
    let mut out = format!("[colors.primary]\nbackground = \"{}\"\nforeground = \"{}\"\n", pal.background, pal.foreground);
    for (table, color) in [("cursor", &pal.cursor_color), ("selection", &pal.selection_background)] {
        let key = if table == "cursor" { "cursor" } else { "background" };
        if !color.is_empty() {
            let _ = write!(out, "\n[colors.{table}]\n{key} = \"{color}\"\n");
        }
    }
    for (table, colors) in [("normal", &pal.colors[..8]), ("bright", &pal.colors[8..])] {
        let _ = write!(out, "\n[colors.{table}]\n");
        for (name, color) in ANSI.iter().zip(colors) {
            let _ = writeln!(out, "{name} = \"{color}\"");
        }
    }
    Ok(out.into_bytes())
}

fn wezterm(pal: &Palette, _: &Path) -> Result<Vec<u8>> {
    let mut out = format!("[metadata]\nname = \"Herdr\"\n\n[colors]\nforeground = \"{}\"\nbackground = \"{}\"\n", pal.foreground, pal.background);
    if !pal.cursor_color.is_empty() {
        let _ = write!(out, "cursor_bg = \"{0}\"\ncursor_border = \"{0}\"\n", pal.cursor_color);
    }
    if !pal.selection_background.is_empty() {
        let _ = writeln!(out, "selection_bg = \"{}\"", pal.selection_background);
    }
    let list = |colors: &[String]| colors.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    let _ = write!(out, "ansi = [{}]\nbrights = [{}]\n", list(&pal.colors[..8]), list(&pal.colors[8..]));
    Ok(out.into_bytes())
}

/// A dynamic profile named "Herdr"; iTerm2 fills its other settings from the
/// default profile.
fn iterm2(pal: &Palette, _: &Path) -> Result<Vec<u8>> {
    let color = |hex: &str| {
        let (r, g, b) = parse_rgb(hex);
        let c = |v: u8| f64::from(v) / 255.0;
        serde_json::json!({ "Red Component": c(r), "Green Component": c(g), "Blue Component": c(b), "Alpha Component": 1.0, "Color Space": "sRGB" })
    };
    let mut profile = serde_json::Map::new();
    profile.insert("Name".into(), "Herdr".into());
    profile.insert("Guid".into(), "herdr-customize-theme".into());
    profile.insert("Foreground Color".into(), color(&pal.foreground));
    profile.insert("Background Color".into(), color(&pal.background));
    for (key, value) in [("Cursor Color", &pal.cursor_color), ("Selection Color", &pal.selection_background)] {
        if !value.is_empty() {
            profile.insert(key.into(), color(value));
        }
    }
    for (i, c) in pal.colors.iter().enumerate() {
        profile.insert(format!("Ansi {i} Color"), color(c));
    }
    json(&serde_json::json!({ "Profiles": [profile] }))
}

/// A settings fragment with the color scheme "Herdr".
fn windows_terminal(pal: &Palette, _: &Path) -> Result<Vec<u8>> {
    let mut scheme = serde_json::Map::new();
    scheme.insert("name".into(), "Herdr".into());
    scheme.insert("background".into(), pal.background.clone().into());
    scheme.insert("foreground".into(), pal.foreground.clone().into());
    for (key, color) in [("cursorColor", &pal.cursor_color), ("selectionBackground", &pal.selection_background)] {
        if !color.is_empty() {
            scheme.insert(key.into(), color.clone().into());
        }
    }
    for (i, color) in pal.colors.iter().enumerate() {
        // Windows Terminal says purple where the others say magenta.
        let name = ANSI[i % 8].replace("magenta", "purple");
        let key = if i < 8 { name } else { format!("bright{}{}", name[..1].to_uppercase(), &name[1..]) };
        scheme.insert(key, color.clone().into());
    }
    json(&serde_json::json!({ "schemes": [scheme] }))
}

fn json(value: &serde_json::Value) -> Result<Vec<u8>> {
    let mut out = serde_json::to_vec_pretty(value)?;
    out.push(b'\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::parse_palette;
    use crate::theme::tests::BODY;

    fn pal() -> Palette {
        parse_palette(BODY).unwrap()
    }

    fn render(f: fn(&Palette, &Path) -> Result<Vec<u8>>) -> String {
        String::from_utf8(f(&pal(), Path::new("unused")).unwrap()).unwrap()
    }

    // Each terminal names the 16 colors its own way; a wrong name silently
    // leaves that color at the terminal's default.
    #[test]
    fn every_scheme_names_every_color() {
        let p = pal();
        let kitty = render(kitty);
        assert!(kitty.contains("background #1e1e2e\n") && kitty.contains("color15 #a6adc8\n") && !kitty.contains("cursor "), "{kitty}");

        let alacritty: toml::Table = render(alacritty).parse().unwrap();
        assert_eq!(alacritty["colors"]["normal"]["magenta"].as_str(), Some(p.colors[5].as_str()));
        assert_eq!(alacritty["colors"]["bright"]["black"].as_str(), Some(p.colors[8].as_str()));
        assert_eq!(alacritty["colors"]["selection"]["background"].as_str(), Some("#585b70"));

        let wezterm: toml::Table = render(wezterm).parse().unwrap();
        assert_eq!(wezterm["metadata"]["name"].as_str(), Some("Herdr"));
        assert_eq!(wezterm["colors"]["brights"].as_array().unwrap().len(), 8);
        assert_eq!(wezterm["colors"]["ansi"][0].as_str(), Some(p.colors[0].as_str()));

        let iterm: serde_json::Value = serde_json::from_str(&render(iterm2)).unwrap();
        let profile = &iterm["Profiles"][0];
        assert_eq!(profile["Name"], "Herdr");
        assert_eq!(profile["Ansi 15 Color"]["Red Component"].as_f64(), Some(f64::from(0xa6u8) / 255.0));
        assert!(profile.get("Cursor Color").is_none());

        let wt: serde_json::Value = serde_json::from_str(&render(windows_terminal)).unwrap();
        let scheme = wt["schemes"][0].as_object().unwrap();
        assert_eq!(scheme["purple"], p.colors[5].as_str());
        assert_eq!(scheme["brightBlack"], p.colors[8].as_str());
        assert_eq!(scheme["brightWhite"], "#a6adc8");
        assert_eq!(scheme.len(), 20, "name, background, foreground, selection and 16 colors");
    }

    // Files in the user's own config are refreshed only once created; a
    // terminal's folder for other apps' files is written to on its own.
    #[test]
    fn refreshes_opted_in_files_and_installed_terminals_only() {
        let _env = crate::ui::tests::isolated();
        let config = PathBuf::from(env("XDG_CONFIG_HOME").unwrap());
        let home = config::home().unwrap();
        let wt = Path::new(&env("LOCALAPPDATA").unwrap()).join("Microsoft").join("Windows Terminal");
        let kitty = config.join("kitty").join("herdr-theme.conf");
        let iterm = home.join("Library").join("Application Support").join("iTerm2");
        let _ = fs::remove_dir_all(&config);
        let _ = fs::remove_dir_all(&iterm);
        fs::create_dir_all(kitty.parent().unwrap()).unwrap();
        fs::write(&kitty, "").unwrap();
        fs::create_dir_all(&wt).unwrap();
        let palette = config.join("palette");
        fs::write(&palette, BODY).unwrap();

        assert!(refresh(&pal(), &palette).is_empty());
        assert!(fs::read_to_string(&kitty).unwrap().contains("color0 #45475a"));
        assert!(wt.join("Fragments").join("herdr-customize").join("herdr-theme.json").is_file());
        assert!(!config.join("alacritty").exists() && !config.join("ghostty").exists());
        assert!(!iterm.exists(), "iTerm2 is not installed in the sandbox");
    }
}
