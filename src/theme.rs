//! The bundled Ghostty palettes in themes/, listed by themes/index.txt.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;

/// The folder holding themes/: beside the binary in target/release, or
/// around the working folder.
fn root() -> PathBuf {
    let has_themes = |dir: &Path| dir.join("themes").is_dir();
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf))
        && let Some(found) = dir.ancestors().take(4).find(|d| has_themes(d))
    {
        return found.to_path_buf();
    }
    if let Ok(cwd) = std::env::current_dir()
        && let Some(found) = cwd.ancestors().take(3).find(|d| has_themes(d))
    {
        return found.to_path_buf();
    }
    ".".into()
}

static SLUG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").unwrap());

/// Lowercase letters, digits and single hyphens only.
fn is_valid_slug(slug: &str) -> bool {
    slug.len() <= 120 && SLUG_RE.is_match(slug)
}

/// Whether color is rrggbb with an optional leading #.
fn is_hex(color: &str) -> bool {
    let hex = color.strip_prefix('#').unwrap_or(color);
    hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())
}

fn normalize_color(color: &str) -> Result<String> {
    if !is_hex(color) {
        bail!("invalid color {color:?}: expected #rrggbb");
    }
    Ok(format!("#{}", color.trim_start_matches('#').to_lowercase()))
}

/// The red, green and blue parts of #rrggbb; black when malformed.
pub fn parse_rgb(hex: &str) -> (u8, u8, u8) {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 || !hex.is_ascii() {
        return (0, 0, 0);
    }
    let part = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
    (part(0), part(2), part(4))
}

/// Whether a background reads as dark.
pub fn is_dark(hex: &str) -> bool {
    let (r, g, b) = parse_rgb(hex);
    0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b) < 128.0
}

/// The entries of a Ghostty palette file.
#[derive(Debug, Clone, Default)]
pub struct Palette {
    pub background: String,
    pub foreground: String,
    pub selection_background: String,
    pub cursor_color: String,
    pub colors: [String; 16],
}

static PALETTE_ENTRY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^palette\s*=\s*(\d+)\s*=\s*(#?[A-Fa-f0-9]{6})(?:\s+#.*)?\s*$").unwrap());

const MAX_PALETTE_BYTES: u64 = 1 << 20;

fn parse_palette_file(path: &Path) -> Result<Palette> {
    if fs::metadata(path)?.len() > MAX_PALETTE_BYTES {
        bail!("palette exceeds {MAX_PALETTE_BYTES} bytes");
    }
    parse_palette(&String::from_utf8_lossy(&fs::read(path)?))
}

/// Parses a Ghostty palette. Background, foreground and all 16 palette
/// entries are required.
pub fn parse_palette(content: &str) -> Result<Palette> {
    let mut pal = Palette::default();
    let mut colors: [Option<String>; 16] = Default::default();
    for (n, line) in content.trim_start_matches('\u{feff}').lines().enumerate() {
        let n = n + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| anyhow!("line {n}: expected key = value"))?;
        match key.trim() {
            "palette" => {
                let m = PALETTE_ENTRY.captures(line).ok_or_else(|| anyhow!("line {n}: invalid palette entry"))?;
                let idx: usize = m[1].parse().ok().filter(|i| *i < 16).ok_or_else(|| anyhow!("line {n}: palette index must be 0..15"))?;
                colors[idx] = Some(normalize_color(&m[2])?);
            }
            key @ ("background" | "foreground" | "selection-background" | "cursor-color") => {
                let fields: Vec<&str> = value.split_whitespace().collect();
                if fields.is_empty() || (fields.len() > 1 && !fields[1].starts_with('#')) {
                    bail!("line {n}: invalid color value");
                }
                let v = normalize_color(fields[0]).map_err(|e| anyhow!("line {n}: {e}"))?;
                match key {
                    "background" => pal.background = v,
                    "foreground" => pal.foreground = v,
                    "selection-background" => pal.selection_background = v,
                    _ => pal.cursor_color = v,
                }
            }
            _ => {}
        }
    }
    if pal.background.is_empty() {
        bail!("background: missing color");
    }
    if pal.foreground.is_empty() {
        bail!("foreground: missing color");
    }
    for (i, color) in colors.into_iter().enumerate() {
        pal.colors[i] = color.ok_or_else(|| anyhow!("palette {i}: missing color"))?;
    }
    Ok(pal)
}

/// The bundled palette named slug.
pub fn resolve_palette(slug: &str) -> Result<Palette> {
    if !is_valid_slug(slug) {
        bail!("invalid slug: {slug}");
    }
    let path = root().join("themes").join(slug);
    if !path.is_file() {
        bail!("theme {slug:?} not found");
    }
    parse_palette_file(&path).context("invalid bundled theme")
}

/// The bundled palettes' slugs in index order.
pub fn library() -> Result<Vec<String>> {
    let index = root().join("themes/index.txt");
    let text = fs::read_to_string(&index).with_context(|| index.display().to_string())?;
    let mut slugs: Vec<String> = Vec::new();
    for slug in text.lines().map(str::trim).filter(|s| !s.is_empty()) {
        if !is_valid_slug(slug) {
            bail!("invalid slug {slug:?} in {}", index.display());
        }
        if !slugs.iter().any(|s| s == slug) {
            slugs.push(slug.to_string());
        }
    }
    Ok(slugs)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const BODY: &str = "background = #1e1e2e\nforeground = #CDD6F4\nselection-background = #585b70 # sel\n\
palette = 0=#45475a\npalette = 1=#f38ba8\npalette = 2=#a6e3a1\npalette = 3=#f9e2af\npalette = 4=#89b4fa\npalette = 5=#f5c2e7\npalette = 6=#94e2d5\npalette = 7=#bac2de\n\
palette = 8=#585b70\npalette = 9=#f38ba8\npalette = 10=#a6e3a1\npalette = 11=#f9e2af\npalette = 12=#89b4fa\npalette = 13=#f5c2e7\npalette = 14=#94e2d5\npalette = 15=#a6adc8\n";

    #[test]
    fn oversized_palettes_are_refused() {
        let path = std::env::temp_dir().join(format!("hc-big-palette-{}", std::process::id()));
        fs::write(&path, format!("{BODY}{}", "#".repeat(MAX_PALETTE_BYTES as usize))).unwrap();
        assert!(parse_palette_file(&path).unwrap_err().to_string().contains("exceeds"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn palette_rejects_missing_and_bad_entries() {
        let pal = parse_palette(BODY).unwrap();
        assert_eq!((pal.foreground.as_str(), pal.selection_background.as_str()), ("#cdd6f4", "#585b70"));
        assert!(parse_palette(&BODY.replace("palette = 15=#a6adc8\n", "")).is_err());
        assert!(parse_palette(&format!("{BODY}palette = 16=#000000\n")).is_err());
        assert!(parse_palette(&format!("{BODY}garbage\n")).is_err());
        assert!(parse_palette(&format!("\u{feff}{BODY}# comment\n")).is_ok());
    }

    #[test]
    fn every_bundled_palette_loads() {
        let slugs = library().unwrap();
        assert!(!slugs.is_empty());
        for slug in slugs {
            resolve_palette(&slug).unwrap_or_else(|e| panic!("{slug}: {e:#}"));
        }
        assert!(resolve_palette("../x").is_err());
    }

    #[test]
    fn colors() {
        assert_eq!(parse_rgb("#102030"), (16, 32, 48));
        assert_eq!(parse_rgb("bad"), (0, 0, 0));
        assert!(is_dark("#1e1e2e") && !is_dark("#fafafa"));
    }
}
