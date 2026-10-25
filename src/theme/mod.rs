//! Palettes: bundled and user-added Ghostty themes, their mapping to Herdr's
//! theme tokens, and the user theme store.

mod apply;
pub mod preview;

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;

pub use apply::{apply_theme, only_remote_clients, sync_applied_theme};

use crate::config::env;
use crate::{fsutil, proc};

/// The Herdr plugin id from herdr-plugin.toml.
pub const PLUGIN_ID: &str = "herdr-customize";

/// The ordering of Herdr theme tokens written to config.toml.
pub const TOKEN_ORDER: [&str; 19] = [
    "panel_bg",
    "sidebar_bg",
    "active_row_bg",
    "selection_bg",
    "text",
    "surface0",
    "surface1",
    "overlay0",
    "overlay1",
    "subtext0",
    "accent",
    "blue",
    "mauve",
    "green",
    "yellow",
    "red",
    "teal",
    "peach",
    "surface_dim",
];

/// Herdr theme tokens by name.
pub type Tokens = HashMap<&'static str, String>;

fn token_name(key: &str) -> Option<&'static str> {
    TOKEN_ORDER.iter().copied().find(|t| *t == key)
}

/// Where a token comes from in a Ghostty palette.
pub fn token_origin(token: &str) -> &'static str {
    match token {
        "panel_bg" => "background",
        "sidebar_bg" => "background −15%",
        "active_row_bg" | "surface0" => "palette 0",
        "selection_bg" => "selection-bg",
        "text" => "foreground",
        "surface1" | "overlay0" => "palette 8",
        "overlay1" | "subtext0" => "palette 7",
        "accent" | "blue" => "palette 4",
        "mauve" => "palette 5",
        "green" => "palette 2",
        "yellow" => "palette 3",
        "red" => "palette 1",
        "teal" => "palette 6",
        "peach" => "palette 9",
        "surface_dim" => "background −8%",
        _ => "?",
    }
}

/// Where the token appears in Herdr's UI.
pub fn token_role(token: &str) -> &'static str {
    match token {
        "panel_bg" => "pane background",
        "sidebar_bg" => "sidebar background",
        "active_row_bg" => "selected sidebar row",
        "selection_bg" => "text selection bg",
        "text" => "primary text",
        "surface0" => "sidebar row bg",
        "surface1" => "inactive pane bg",
        "overlay0" => "dim text",
        "overlay1" => "bright text",
        "subtext0" => "secondary text",
        "accent" => "active pane border/selector",
        "blue" => "links/info",
        "mauve" | "peach" => "accent",
        "green" => "success ✓",
        "yellow" => "warning",
        "red" => "error ✗",
        "teal" => "now-playing",
        "surface_dim" => "dividers",
        _ => "",
    }
}

/* ---------------------------------------------------------------- paths */

/// The directory holding plugin assets (themes, herdr-plugin.toml).
pub fn plugin_root() -> PathBuf {
    if let Some(root) = env("HERDR_PLUGIN_ROOT") {
        return root.into();
    }
    let has_themes = |dir: &Path| dir.join("themes").is_dir();
    // The binary lives in target/release when Herdr builds the plugin.
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

/// Where plugin state (applied theme, user themes) is kept. Standalone runs
/// use the same place Herdr gives plugin commands.
pub fn state_dir() -> PathBuf {
    if let Some(state) = env("HERDR_PLUGIN_STATE_DIR") {
        return state.into();
    }
    if let Some(xdg) = env("XDG_STATE_HOME") {
        return Path::new(&xdg).join("herdr/plugins").join(PLUGIN_ID);
    }
    match env("HOME") {
        Some(home) => Path::new(&home).join(".local/state/herdr/plugins").join(PLUGIN_ID),
        None => ".state".into(),
    }
}

pub fn user_themes_dir() -> PathBuf {
    state_dir().join("themes")
}

fn user_index_file() -> PathBuf {
    state_dir().join("index.txt")
}

/// The file recording the applied theme on this machine.
pub fn applied_file() -> PathBuf {
    state_dir().join("applied")
}

/* -------------------------------------------------------------- colors */

static SLUG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").unwrap());

/// Lowercase letters, digits and single hyphens only.
pub fn is_valid_slug(slug: &str) -> bool {
    slug.len() <= 120 && SLUG_RE.is_match(slug)
}

/// Turns an arbitrary name into a slug.
pub fn slugify(name: &str) -> String {
    let mut s: String = name.to_lowercase().replace([' ', '_'], "-").chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-').collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    s.trim_matches('-').to_string()
}

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

/// Darkens #rrggbb by percent.
pub fn darken(hex: &str, percent: u32) -> String {
    if !is_hex(hex) {
        return hex.to_string();
    }
    let (r, g, b) = parse_rgb(hex);
    let f = 100u32.saturating_sub(percent);
    let d = |c: u8| (c as u32 * f / 100) as u8;
    format!("#{:02x}{:02x}{:02x}", d(r), d(g), d(b))
}

/// Whether a background reads as dark.
pub fn is_dark(hex: &str) -> bool {
    let (r, g, b) = parse_rgb(hex);
    0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b) < 128.0
}

/* ------------------------------------------------------------- palette */

/// The entries of a Ghostty palette file and any token overrides.
#[derive(Debug, Clone, Default)]
pub struct Palette {
    pub background: String,
    pub foreground: String,
    pub selection_background: String,
    pub cursor_color: String,
    pub colors: [String; 16],
    pub overrides: BTreeMap<&'static str, String>,
}

static PALETTE_ENTRY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^palette\s*=\s*(\d+)\s*=\s*(#?[A-Fa-f0-9]{6})(?:\s+#.*)?\s*$").unwrap());
static OVERRIDE_ENTRY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#\s*hpick-override:\s*([a-zA-Z0-9_]+)\s*=\s*(#?[A-Fa-f0-9]{6})(?:\s+#.*)?\s*$").unwrap());
static OVERRIDE_PREFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#\s*hpick-override:").unwrap());

const MAX_PALETTE_BYTES: usize = 1 << 20;

pub fn parse_palette_file(path: &Path) -> Result<Palette> {
    let data = fs::read(path)?;
    if data.len() > MAX_PALETTE_BYTES {
        bail!("palette exceeds {MAX_PALETTE_BYTES} bytes");
    }
    parse_palette(&String::from_utf8_lossy(&data))
}

/// Parses a Ghostty palette. Background, foreground and all 16 palette
/// entries are required.
pub fn parse_palette(content: &str) -> Result<Palette> {
    if content.len() > MAX_PALETTE_BYTES {
        bail!("palette exceeds {MAX_PALETTE_BYTES} bytes");
    }
    let mut pal = Palette::default();
    let mut colors: [Option<String>; 16] = Default::default();
    for (n, line) in content.trim_start_matches('\u{feff}').lines().enumerate() {
        let n = n + 1;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if OVERRIDE_PREFIX.is_match(line) {
            let m = OVERRIDE_ENTRY.captures(line).ok_or_else(|| anyhow!("line {n}: invalid theme token override"))?;
            let token = token_name(&m[1]).ok_or_else(|| anyhow!("line {n}: invalid theme token override"))?;
            pal.overrides.insert(token, normalize_color(&m[2])?);
            continue;
        }
        if line.starts_with('#') {
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

impl Palette {
    /// The 19 Herdr theme tokens, with overrides applied.
    pub fn tokens(&self) -> Tokens {
        let p = &self.colors;
        let sel = if self.selection_background.is_empty() { &p[8] } else { &self.selection_background };
        let bg = &self.background;
        let mut tokens: Tokens = [
            ("panel_bg", bg.clone()),
            ("sidebar_bg", darken(bg, 15)),
            ("active_row_bg", p[0].clone()),
            ("selection_bg", sel.clone()),
            ("surface0", p[0].clone()),
            ("surface1", p[8].clone()),
            ("surface_dim", darken(bg, 8)),
            ("overlay0", p[8].clone()),
            ("overlay1", p[7].clone()),
            ("text", self.foreground.clone()),
            ("subtext0", p[7].clone()),
            ("accent", p[4].clone()),
            ("mauve", p[5].clone()),
            ("green", p[2].clone()),
            ("yellow", p[3].clone()),
            ("red", p[1].clone()),
            ("blue", p[4].clone()),
            ("teal", p[6].clone()),
            ("peach", p[9].clone()),
        ]
        .into_iter()
        .collect();
        for (k, v) in &self.overrides {
            tokens.insert(k, v.clone());
        }
        tokens
    }
}

/// What the picker shows of a palette before it is applied.
#[derive(Debug, Clone, Default)]
pub struct Look {
    pub background: String,
    pub foreground: String,
    pub ansi: [String; 16],
    pub dark: bool,
    pub tokens: Tokens,
}

impl Look {
    pub fn of(pal: &Palette) -> Look {
        Look { background: pal.background.clone(), foreground: pal.foreground.clone(), ansi: pal.colors.clone(), dark: is_dark(&pal.background), tokens: pal.tokens() }
    }
}

pub fn load_look(slug: &str) -> Result<Look> {
    Ok(Look::of(&parse_palette_file(&resolve_palette(slug)?)?))
}

/// The palette file for slug: a user theme first, then a bundled one.
pub fn resolve_palette(slug: &str) -> Result<PathBuf> {
    if !is_valid_slug(slug) {
        bail!("invalid slug: {slug}");
    }
    let user = user_themes_dir().join(slug);
    match fs::symlink_metadata(&user) {
        Ok(meta) if !meta.is_file() => bail!("user theme is not a regular file: {slug}"),
        Ok(_) => {
            parse_palette_file(&user).context("invalid user theme")?;
            return Ok(user);
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let bundled = plugin_root().join("themes").join(slug);
    if bundled.is_file() {
        return Ok(bundled);
    }
    bail!("theme {slug:?} not found (browse more at https://terminalcolors.com and add one from the clipboard or your editor)")
}

/* --------------------------------------------------------------- store */

/// One row of the theme list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Item {
    pub slug: String,
    pub user: bool,
    pub applied: bool,
}

/// An index file's unique non-blank entries, split into accepted slugs and
/// rejected entries.
fn read_index(path: &Path) -> io::Result<(Vec<String>, Vec<String>)> {
    let text = fs::read_to_string(path)?;
    let (mut slugs, mut rejected) = (Vec::new(), Vec::new());
    for slug in text.lines().map(str::trim).filter(|s| !s.is_empty()) {
        if slugs.iter().chain(&rejected).any(|s| s == slug) {
            continue;
        }
        if is_valid_slug(slug) { slugs.push(slug.to_string()) } else { rejected.push(slug.to_string()) }
    }
    Ok((slugs, rejected))
}

fn read_user_index() -> Result<Vec<String>> {
    match read_index(&user_index_file()) {
        Ok((slugs, _)) => Ok(slugs),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

/// A stored user theme's file. Never follows links out of the store.
fn user_theme_file(slug: &str) -> Result<PathBuf> {
    if !is_valid_slug(slug) {
        bail!("invalid slug: {slug:?}");
    }
    let path = user_themes_dir().join(slug);
    if !fs::symlink_metadata(&path)?.is_file() {
        bail!("user theme is not a regular file: {slug}");
    }
    Ok(path)
}

/// user_theme_file restricted to themes listed in the index.
fn user_theme_path(slug: &str) -> Result<PathBuf> {
    let path = user_theme_file(slug)?;
    if !read_user_index()?.iter().any(|s| s == slug) {
        bail!("{slug:?} is not a user-added theme");
    }
    Ok(path)
}

/// Adds or removes slug in the index, keeping entries it does not know.
fn update_user_index(slug: &str, remove: bool) -> Result<()> {
    let index = user_index_file();
    let _lock = fsutil::lock(&index.with_extension("txt.lock")).context("lock theme index")?;
    let text = match fs::read_to_string(&index) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let mut lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    let found = lines.iter().any(|l| l.trim() == slug);
    if remove {
        lines.retain(|l| l.trim() != slug);
    } else if !found {
        lines.push(slug);
    }
    fsutil::atomic_write(&index, format!("{}\n", lines.join("\n")).as_bytes(), 0o644)
}

/// The recorded theme, or "" when none is usable.
pub fn applied_slug() -> Result<String> {
    let applied = match fs::read_to_string(applied_file()) {
        Ok(t) => t.trim().to_string(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(String::new()),
        Err(e) => return Err(e.into()),
    };
    Ok(if is_valid_slug(&applied) { applied } else { String::new() })
}

/// The applied theme first, then user themes, then bundled ones. Unusable
/// user entries are skipped and reported as warnings.
pub fn themes() -> Result<(Vec<Item>, Vec<String>)> {
    let mut warnings = Vec::new();
    let users = read_user_index()?;
    let bundled_index = plugin_root().join("themes/index.txt");
    let (bundled, invalid) = read_index(&bundled_index).with_context(|| bundled_index.display().to_string())?;
    if let Some(bad) = invalid.first() {
        bail!("invalid slug {bad:?} in {}", bundled_index.display());
    }
    let applied = applied_slug()?;
    let mut items: Vec<Item> = Vec::new();
    let mut add = |item: Item| {
        if !items.iter().any(|i| i.slug == item.slug) {
            items.push(item);
        }
    };
    if !applied.is_empty() {
        add(Item { user: users.contains(&applied) && user_theme_file(&applied).is_ok(), slug: applied, applied: true });
    }
    for slug in users {
        match user_theme_file(&slug) {
            Ok(_) => add(Item { slug, user: true, applied: false }),
            Err(e) => warnings.push(format!("skipping unavailable user theme {slug:?}: {e}")),
        }
    }
    for slug in bundled {
        add(Item { slug, ..Default::default() });
    }
    Ok((items, warnings))
}

/// Checks a Ghostty palette before it is named and saved.
pub fn validate_theme_body(body: &str) -> Result<()> {
    parse_palette(body).map(|_| ()).context("invalid palette")
}

/// What save_user_theme did.
#[derive(Debug, PartialEq)]
pub enum Saved {
    Saved(String),
    /// A theme with this slug exists and overwrite was not set.
    Exists(String),
}

/// Stores body as a user theme named name.
pub fn save_user_theme(name: &str, body: &str, overwrite: bool) -> Result<Saved> {
    validate_theme_body(body)?;
    let slug = slugify(name);
    if !is_valid_slug(&slug) {
        bail!("invalid theme name: {name:?}");
    }
    let dest = user_themes_dir().join(&slug);
    match fs::symlink_metadata(&dest) {
        Ok(meta) if !meta.is_file() => bail!("theme is not a regular file: {slug}"),
        Ok(_) if !overwrite => return Ok(Saved::Exists(slug)),
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    fs::create_dir_all(user_themes_dir())?;
    fsutil::atomic_write(&dest, body.as_bytes(), 0o644)?;
    update_user_index(&slug, false).with_context(|| format!("theme saved at {}, but the index update failed", dest.display()))?;
    Ok(Saved::Saved(slug))
}

/// Deletes a user theme and clears the selection marker if it pointed at
/// it. Bundled themes cannot be removed.
pub fn remove_user_theme(slug: &str) -> Result<()> {
    let dest = user_theme_path(slug)?;
    fs::remove_file(dest).context("delete theme")?;
    update_user_index(slug, true).context("theme deleted, but the index update failed")?;
    if applied_slug().unwrap_or_default() == slug {
        fs::remove_file(applied_file()).context("theme deleted, but the selection marker remains")?;
    }
    Ok(())
}

/// A user theme's palette for editing.
pub fn user_theme_body(slug: &str) -> Result<String> {
    let path = user_theme_path(slug)?;
    let body = fs::read_to_string(path)?;
    parse_palette(&body)?;
    Ok(body)
}

/// Replaces a user theme's palette with an edited body.
pub fn commit_user_theme(slug: &str, body: &str) -> Result<()> {
    let dest = user_theme_path(slug)?;
    parse_palette(body)?;
    fsutil::atomic_write(&dest, body.as_bytes(), 0o644)
}

/// Records a per-token color in a user theme body.
pub fn set_override(body: &str, token: &str, color: &str) -> Result<String> {
    let token = token_name(token).ok_or_else(|| anyhow!("unknown token: {token}"))?;
    let color = normalize_color(color.trim())?;
    let mut lines: Vec<String> = body.trim_end_matches(['\r', '\n']).lines().filter(|line| OVERRIDE_ENTRY.captures(line.trim()).is_none_or(|m| &m[1] != token)).map(String::from).collect();
    lines.push(format!("# hpick-override: {token}={color}"));
    let updated = lines.join("\n") + "\n";
    parse_palette(&updated)?;
    Ok(updated)
}

/// One Herdr token of a palette, for the token editor.
#[derive(Debug, Clone)]
pub struct TokenRow {
    pub token: &'static str,
    pub color: String,
    pub origin: &'static str,
    pub role: &'static str,
    pub overridden: bool,
}

pub fn token_rows(body: &str) -> Result<(Vec<TokenRow>, Tokens)> {
    let pal = parse_palette(body)?;
    let tokens = pal.tokens();
    let rows = TOKEN_ORDER
        .iter()
        .map(|&token| {
            let overridden = pal.overrides.contains_key(token);
            TokenRow { token, color: tokens[token].clone(), origin: if overridden { "custom override" } else { token_origin(token) }, role: token_role(token), overridden }
        })
        .collect();
    Ok((rows, tokens))
}

/// The starting text for a theme typed or pasted in an editor.
const NEW_THEME_TEMPLATE: &str = "# Paste a ghostty-format theme below, then save and close this editor.
# Required: background, foreground, and palette 0..15 as #rrggbb colors.
# Get one from https://terminalcolors.com → Download → Ghostty.
#
# background = #1e1e2e
# foreground = #cdd6f4
# cursor-color = #f5e0dc
# palette = 0=#45475a
# palette = 1=#f38ba8
# ... through ...
# palette = 15=#a6adc8
";

/// Writes the new-theme template to a temporary file for an editor. The
/// caller removes it.
pub fn new_theme_file() -> Result<PathBuf> {
    let path = fsutil::temp_path(&std::env::temp_dir(), "herdr-theme-").with_extension("txt");
    fs::write(&path, NEW_THEME_TEMPLATE)?;
    Ok(path)
}

/* ------------------------------------------------------------- editors */

/// VISUAL or EDITOR, else an installed fallback.
fn pick_editor() -> String {
    for key in ["VISUAL", "EDITOR"] {
        if let Some(v) = env(key) {
            return v;
        }
    }
    ["nvim", "nano"].iter().find(|n| proc::look_path(n).is_some()).map(|n| n.to_string()).unwrap_or_else(|| "vi".into())
}

/// Splits an editor command without evaluating shell syntax.
fn split_editor(command: &str) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '\\' && next.is_some_and(|n| (quote == Some('"') && n == '"') || (quote.is_none() && n.is_whitespace())) {
            word.push(next.unwrap());
            started = true;
            i += 2;
            continue;
        }
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => word.push(c),
            None if c == '\'' || c == '"' => quote = Some(c),
            None if c.is_whitespace() => {
                if started {
                    args.push(std::mem::take(&mut word));
                    started = false;
                }
                i += 1;
                continue;
            }
            None => word.push(c),
        }
        started = true;
        i += 1;
    }
    if quote.is_some() {
        bail!("unclosed quote in editor command");
    }
    if started {
        args.push(word);
    }
    if args.first().is_none_or(String::is_empty) {
        bail!("editor command is empty");
    }
    Ok(args)
}

/// A command that opens path in the preferred editor.
pub fn editor_command(path: &Path) -> Result<Command> {
    let editor = pick_editor();
    // A preference may be an unquoted executable path containing spaces.
    let args = match proc::look_path(&editor) {
        Some(resolved) => vec![resolved.to_string_lossy().into_owned()],
        None => split_editor(&editor)?,
    };
    let exe = proc::look_path(&args[0]).ok_or_else(|| anyhow!("editor {:?} not found", args[0]))?;
    let mut cmd = Command::new(exe);
    cmd.args(&args[1..]).arg(path);
    Ok(cmd)
}

/// The text on the clipboard.
pub fn read_clipboard() -> Result<String> {
    for (tool, args) in [("pbpaste", &[][..]), ("wl-paste", &["--no-newline"][..]), ("xclip", &["-selection", "clipboard", "-o"][..]), ("xsel", &["--clipboard", "--output"][..])] {
        if let Some(path) = proc::look_path(tool) {
            let out = Command::new(path).args(args).output()?;
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
    }
    bail!("no clipboard utility found (pbpaste, wl-paste, xclip, xsel)")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const BODY: &str = "background = #1e1e2e\nforeground = #CDD6F4\nselection-background = #585b70 # sel\n\
palette = 0=#45475a\npalette = 1=#f38ba8\npalette = 2=#a6e3a1\npalette = 3=#f9e2af\npalette = 4=#89b4fa\npalette = 5=#f5c2e7\npalette = 6=#94e2d5\npalette = 7=#bac2de\n\
palette = 8=#585b70\npalette = 9=#f38ba8\npalette = 10=#a6e3a1\npalette = 11=#f9e2af\npalette = 12=#89b4fa\npalette = 13=#f5c2e7\npalette = 14=#94e2d5\npalette = 15=#a6adc8\n";

    #[test]
    fn palette_maps_to_tokens() {
        let pal = parse_palette(BODY).unwrap();
        let t = pal.tokens();
        assert_eq!(t.len(), 19);
        assert_eq!(t["text"], "#cdd6f4");
        assert_eq!(t["accent"], "#89b4fa");
        assert_eq!(t["sidebar_bg"], darken("#1e1e2e", 15));
        assert_eq!(t["selection_bg"], "#585b70");
        assert!(is_dark(&pal.background));
    }

    #[test]
    fn palette_rejects_missing_and_bad_entries() {
        assert!(parse_palette(&BODY.replace("palette = 15=#a6adc8\n", "")).is_err());
        assert!(parse_palette(&format!("{BODY}palette = 16=#000000\n")).is_err());
        assert!(parse_palette(&format!("{BODY}# hpick-override: nope=#000000\n")).is_err());
        assert!(parse_palette(&format!("{BODY}garbage\n")).is_err());
        assert!(parse_palette(&format!("\u{feff}{BODY}# comment\n")).is_ok());
    }

    #[test]
    fn overrides_replace_tokens() {
        let body = set_override(BODY, "accent", "#ABCDEF").unwrap();
        let body = set_override(&body, "accent", "#123456").unwrap();
        assert_eq!(body.matches("hpick-override").count(), 1);
        let (rows, tokens) = token_rows(&body).unwrap();
        assert_eq!(tokens["accent"], "#123456");
        assert!(rows.iter().find(|r| r.token == "accent").unwrap().overridden);
        assert!(set_override(BODY, "nope", "#000000").is_err());
    }

    #[test]
    fn slugs() {
        assert_eq!(slugify("  My Cool_Theme!! "), "my-cool-theme");
        assert!(is_valid_slug("catppuccin-mocha"));
        assert!(!is_valid_slug("../x"));
        assert!(!is_valid_slug("a--b"));
    }

    #[test]
    fn editor_splitting() {
        assert_eq!(split_editor("code --wait").unwrap(), ["code", "--wait"]);
        assert_eq!(split_editor("'/Applications/My Editor' -n").unwrap(), ["/Applications/My Editor", "-n"]);
        assert_eq!(split_editor(r"my\ editor").unwrap(), ["my editor"]);
        assert!(split_editor("'open").is_err());
        assert!(split_editor("   ").is_err());
    }

    #[test]
    fn colors() {
        assert_eq!(darken("#ffffff", 15), "#d8d8d8");
        assert_eq!(parse_rgb("#102030"), (16, 32, 48));
        assert_eq!(parse_rgb("bad"), (0, 0, 0));
        assert!(!is_dark("#fafafa"));
    }
}
