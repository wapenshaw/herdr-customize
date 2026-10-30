//! Applying a theme: [theme.custom] in config.toml, the selection marker,
//! the host terminals' colors, and a reload.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use toml_edit::{Item, Table};

use super::{Palette, TOKENS, Tokens, applied_file, resolve_palette, state_dir};
use crate::config::{self, Doc, SaveResult, Store, env};
use crate::{fsutil, herdr, schema, terminal};

const STARTUP_WAIT: Duration = Duration::from_secs(10);

/// Writes [theme.custom] to this machine's config, records the selection,
/// recolors the client terminals and reloads. Sync and reload are best
/// effort: the config is the truth, and a manual reload repairs either.
pub fn apply_theme(slug: &str) -> Result<()> {
    let cfg = config::path();
    let (pal, palette_path) = resolve_palette(slug).with_context(|| format!("cannot load theme {slug:?}"))?;
    fs::create_dir_all(state_dir()).context("create state directory")?;
    let _lock = fsutil::lock(&applied_file().with_extension("lock")).context("lock theme selection")?;
    let saved = write_custom_block(&cfg, &pal.tokens()).context("failed to write the custom theme block")?;
    if !saved.check_skipped.is_empty() {
        eprintln!("Warning: herdr config check unavailable: {}", saved.check_skipped);
    }
    // Record the selection with the config so a later sync or failed reload
    // can never pair the new UI theme with the previous terminal palette.
    fsutil::atomic_write(&applied_file(), format!("{slug}\n").as_bytes(), 0o644).with_context(|| format!("theme written to {}, but the selection marker failed", cfg.display()))?;
    if let Err(e) = sync_terminal_colors(&pal, &palette_path, &env("HERDR_CUSTOMIZE_CLIENT_PID").unwrap_or_default())
        && !terminal::is_unavailable(&e)
    {
        eprintln!("Warning: terminal colors not fully synced: {e:#}");
    }
    if let Err(e) = herdr::reload() {
        eprintln!("Theme {slug:?} saved to {}, but reload failed: {e:#}\nUse Herdr's reload config action (default prefix+shift+r).", cfg.display());
        return Ok(());
    }
    println!("Applied theme {slug:?}.");
    Ok(())
}

/// Removes the palette from [theme.custom], forgets the selection, hands the
/// client terminals back their own colors and reloads, so Herdr shows its
/// built-in theme. Scheme files and the auto_switch layers are kept.
pub fn reset_theme() -> Result<()> {
    let cfg = config::path();
    fs::create_dir_all(state_dir()).context("create state directory")?;
    let _lock = fsutil::lock(&applied_file().with_extension("lock")).context("lock theme selection")?;
    let saved = Store { path: cfg.clone(), no_reload: true }.update(drop_custom_tokens).context("failed to remove the custom theme block")?;
    if !saved.check_skipped.is_empty() {
        eprintln!("Warning: herdr config check unavailable: {}", saved.check_skipped);
    }
    match fs::remove_file(applied_file()) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e).with_context(|| format!("theme removed from {}, but the selection marker remains", cfg.display())),
        _ => {}
    }
    // OSC 104, 110 and 111 reset the palette, foreground and background to
    // the terminal's own.
    if let Err(e) = emit("\x1b]104\x07\x1b]110\x07\x1b]111\x07", &env("HERDR_CUSTOMIZE_CLIENT_PID").unwrap_or_default(), Vec::new())
        && !terminal::is_unavailable(&e)
    {
        eprintln!("Warning: terminal colors not fully reset: {e:#}");
    }
    if let Err(e) = herdr::reload() {
        eprintln!("Theme removed from {}, but reload failed: {e:#}\nUse Herdr's reload config action (default prefix+shift+r).", cfg.display());
        return Ok(());
    }
    println!("Back to Herdr's built-in theme.");
    Ok(())
}

/// Herdr's own theme, which a reset hands back to.
pub struct Builtin {
    /// theme.name, or Herdr's default when it is unset.
    pub name: String,
    /// [theme.custom] holds no palette, so Herdr shows this theme.
    pub in_use: bool,
    /// The auto_switch layers a reset keeps.
    pub layers: bool,
}

pub fn builtin() -> Result<Builtin> {
    let doc = read_config()?;
    let name = config::lookup(&doc, &["theme".into(), "name".into()])
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| schema::bundled().lookup("theme.name")?.default_value()?.as_str().map(String::from))
        .unwrap_or_else(|| "built-in".into());
    let custom = config::lookup(&doc, &["theme".into(), "custom".into()]).and_then(|v| v.as_table());
    Ok(Builtin { name, in_use: !custom.is_some_and(|c| TOKENS.iter().all(|(k, ..)| c.contains_key(*k))), layers: custom_layers(&doc).iter().any(Option::is_some) })
}

/// Replaces the base tokens of [theme.custom], keeping the auto_switch
/// [theme.custom.light] and [theme.custom.dark] layers. It never reloads, so
/// the reload can follow the terminal sync.
pub(crate) fn write_custom_block(cfg: &Path, tokens: &Tokens) -> Result<SaveResult> {
    Store { path: cfg.to_path_buf(), no_reload: true }.update(|doc| replace_custom_tokens(doc, tokens))
}

const LAYERS: [&str; 2] = ["light", "dark"];

fn replace_custom_tokens(doc: &mut Doc, tokens: &Tokens) -> Result<()> {
    let layers = custom_layers(&doc.decoded());
    let root = doc.raw().as_table_mut();
    if !root.contains_key("theme") {
        let mut t = Table::new();
        t.set_implicit(true);
        root.insert("theme", Item::Table(t));
    }
    let theme = root["theme"].as_table_like_mut().ok_or_else(|| anyhow!("theme must be a TOML table"))?;
    let custom = theme.entry("custom").or_insert(Item::Table(Table::new()));
    // A table that only held layers was implicit; it now needs its header.
    // A comment above it described the previous colors, so it goes too.
    if let Item::Table(t) = custom {
        t.set_implicit(false);
        let prefix = t.decor().prefix().and_then(|p| p.as_str()).unwrap_or_default();
        if prefix.contains('#') {
            let blank = prefix.lines().take_while(|l| l.trim().is_empty()).count();
            t.decor_mut().set_prefix("\n".repeat(blank));
        }
    }
    let custom = custom.as_table_like_mut().ok_or_else(|| anyhow!("theme.custom must be a TOML table"))?;
    let stale: Vec<String> = custom.iter().map(|(k, _)| k.to_string()).filter(|k| !LAYERS.contains(&k.as_str())).collect();
    for key in stale {
        custom.remove(&key);
    }
    for (token, ..) in TOKENS {
        let mut value = toml_edit::Value::from(tokens[token].as_str());
        value.decor_mut().clear();
        custom.insert(token, Item::Value(value));
    }
    if custom_layers(&doc.decoded()) != layers {
        bail!("could not preserve [theme.custom.light] / [theme.custom.dark]");
    }
    Ok(())
}

/// Removes every base token of [theme.custom] when it holds a palette, and the
/// table itself unless auto_switch layers remain in it. A partial table is a
/// tweak on the built-in theme, such as Herdr Radar's block, and stays.
fn drop_custom_tokens(doc: &mut Doc) -> Result<()> {
    let layers = custom_layers(&doc.decoded());
    let root = doc.raw().as_table_mut();
    let Some(theme) = root.get_mut("theme").and_then(Item::as_table_like_mut) else {
        return Ok(());
    };
    let Some(custom) = theme.get_mut("custom") else {
        return Ok(());
    };
    let table = custom.as_table_like_mut().ok_or_else(|| anyhow!("theme.custom must be a TOML table"))?;
    if !TOKENS.iter().all(|(k, ..)| table.contains_key(k)) {
        return Ok(());
    }
    let base: Vec<String> = table.iter().map(|(k, _)| k.to_string()).filter(|k| !LAYERS.contains(&k.as_str())).collect();
    for key in base {
        table.remove(&key);
    }
    if table.is_empty() {
        theme.remove("custom");
    } else if let Item::Table(t) = custom {
        // Only layers are left, which need no [theme.custom] header.
        t.set_implicit(true);
    }
    if custom_layers(&doc.decoded()) != layers {
        bail!("could not preserve [theme.custom.light] / [theme.custom.dark]");
    }
    Ok(())
}

/// The user's auto_switch overrides, which the picker leaves alone.
fn custom_layers(doc: &toml::Table) -> Vec<Option<toml::Value>> {
    LAYERS.iter().map(|l| config::lookup(doc, &["theme".into(), "custom".into(), l.to_string()]).cloned()).collect()
}

/// The OSC 4/10/11 sequences that set a palette.
fn osc_payload(pal: &Palette) -> String {
    let mut out = format!("\x1b]10;{}\x07\x1b]11;{}\x07", pal.foreground, pal.background);
    for (i, color) in pal.colors.iter().enumerate() {
        out.push_str(&format!("\x1b]4;{i};{color}\x07"));
    }
    out
}

/// Recolors this machine's Herdr client terminals (only client_pid's, when
/// set) and refreshes the scheme files terminals load on start. Fails with
/// terminal::Unavailable when no client was found.
fn sync_terminal_colors(pal: &Palette, palette_path: &Path, client_pid: &str) -> Result<()> {
    emit(&osc_payload(pal), client_pid, super::schemes::refresh(pal, palette_path))
}

/// Sends payload to this machine's client terminals (only client_pid's, when
/// set), failing with errors and its own, or with terminal::Unavailable when
/// no client was reached.
fn emit(payload: &str, client_pid: &str, mut errors: Vec<String>) -> Result<()> {
    let clients = terminal::active_clients(client_pid)?;
    let mut synced = 0;
    for client in &clients {
        match terminal::emit_client(client, payload) {
            Ok(()) => synced += 1,
            Err(e) if terminal::is_unavailable(&e) => {}
            Err(e) => errors.push(format!("client {}: {e:#}", client.pid)),
        }
    }
    if !errors.is_empty() {
        bail!("{}", errors.join("; "));
    }
    if synced == 0 {
        return Err(terminal::Unavailable.into());
    }
    Ok(())
}

/// Re-sends the saved selection's colors, e.g. after opening a new terminal
/// window. Without a saved selection nothing is written.
pub fn sync_applied_theme(client_pid: &str) -> Result<()> {
    let slug = super::applied_slug().context("read applied theme")?;
    if slug.is_empty() {
        return Ok(());
    }
    let (pal, palette_path) = resolve_palette(&slug).with_context(|| format!("cannot load theme {slug:?}"))?;
    // A config edited independently, or a failed marker write, must never
    // resurrect stale terminal colors from the previous selection.
    if !config_matches(&pal.tokens())? {
        bail!("saved theme {slug:?} does not match {}; run apply {slug} again before syncing", config::path().display());
    }
    sync_terminal_colors(&pal, &palette_path, client_pid)
}

/// Whether [theme.custom] holds these tokens.
pub(super) fn config_matches(tokens: &Tokens) -> Result<bool> {
    let doc = read_config()?;
    let custom = config::lookup(&doc, &["theme".into(), "custom".into()]).and_then(|v| v.as_table());
    Ok(tokens.iter().all(|(key, value)| custom.and_then(|c| c.get(*key)).and_then(|v| v.as_str()).is_some_and(|c| c.eq_ignore_ascii_case(value))))
}

/// This machine's config, empty when there is none yet.
fn read_config() -> Result<toml::Table> {
    let cfg = config::path();
    match fs::read_to_string(&cfg) {
        Ok(text) => Ok(Doc::parse(&text)?.decoded()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(toml::Table::new()),
        Err(e) => Err(e).with_context(|| format!("read {}", cfg.display())),
    }
}

/// Syncs each Herdr client on this machine that shows up within STARTUP_WAIT,
/// once. The window that started the server may appear before or after this
/// starts, and other windows may already be open, so all of them are watched.
pub fn sync_at_startup() -> Result<()> {
    if super::applied_slug()?.is_empty() {
        return Ok(());
    }
    let deadline = Instant::now() + STARTUP_WAIT;
    let mut synced = HashSet::new();
    let mut errors = Vec::new();
    while Instant::now() < deadline {
        for client in terminal::active_clients("").unwrap_or_default() {
            if synced.insert(client.pid)
                && let Err(e) = sync_applied_theme(&client.pid.to_string())
                && !terminal::is_unavailable(&e)
            {
                errors.push(format!("{e:#}"));
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
    if !errors.is_empty() {
        bail!("{}", errors.join("; "));
    }
    Ok(())
}

/// Whether this runs inside Herdr while only herdr --remote clients are
/// attached. Whoever opened it is then on another machine whose theme comes
/// from its own config, so changing this machine's would not affect them.
pub fn only_remote_clients() -> bool {
    env("HERDR_ENV").is_some() && terminal::processes().is_ok_and(|p| terminal::only_remote(&p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::parse_palette;
    use crate::theme::tests::BODY;

    fn tokens() -> Tokens {
        parse_palette(BODY).unwrap().tokens()
    }

    #[test]
    fn writes_a_new_block_and_keeps_everything_else() {
        let mut doc = Doc::parse("# my config\n[ui]\nsidebar_width = 30 # wide\n").unwrap();
        replace_custom_tokens(&mut doc, &tokens()).unwrap();
        let text = doc.text();
        assert!(text.starts_with("# my config\n[ui]\nsidebar_width = 30 # wide\n"), "{text}");
        assert!(text.contains("[theme.custom]\npanel_bg = \"#1e1e2e\"\nsidebar_bg = "), "{text}");
    }

    #[test]
    fn replaces_tokens_and_keeps_layers() {
        let mut doc = Doc::parse("[theme]\nauto_switch = true\n\n[theme.custom]\naccent = \"#000000\"\nstray = 1\n\n[theme.custom.dark]\naccent = \"#ffffff\"\n").unwrap();
        replace_custom_tokens(&mut doc, &tokens()).unwrap();
        replace_custom_tokens(&mut doc, &tokens()).unwrap();
        let decoded = doc.decoded();
        let custom = config::lookup(&decoded, &["theme".into(), "custom".into()]).unwrap().as_table().unwrap();
        assert_eq!(custom["accent"].as_str(), Some("#89b4fa"));
        assert!(!custom.contains_key("stray"));
        assert_eq!(custom["dark"]["accent"].as_str(), Some("#ffffff"));
        assert_eq!(doc.text().matches("[theme.custom]").count(), 1);
    }

    #[test]
    fn layers_only_gain_a_header() {
        let mut doc = Doc::parse("[theme.custom.light]\ntext = \"#000000\"\n").unwrap();
        replace_custom_tokens(&mut doc, &tokens()).unwrap();
        let decoded = doc.decoded();
        assert_eq!(config::lookup(&decoded, &["theme".into(), "custom".into(), "light".into(), "text".into()]).and_then(|v| v.as_str()), Some("#000000"));
        assert_eq!(config::lookup(&decoded, &["theme".into(), "custom".into(), "text".into()]).and_then(|v| v.as_str()), Some("#cdd6f4"));
    }

    #[test]
    fn inline_theme_tables() {
        let mut doc = Doc::parse("theme = { name = \"x\", custom = { accent = \"#000000\" } }\n").unwrap();
        replace_custom_tokens(&mut doc, &tokens()).unwrap();
        let decoded = doc.decoded();
        assert_eq!(config::lookup(&decoded, &["theme".into(), "name".into()]).and_then(|v| v.as_str()), Some("x"));
        assert_eq!(config::lookup(&decoded, &["theme".into(), "custom".into(), "accent".into()]).and_then(|v| v.as_str()), Some("#89b4fa"));
    }

    // A comment left by an earlier picker named colors that are no longer there.
    #[test]
    fn drops_the_comment_above_the_replaced_block() {
        let mut doc = Doc::parse("[theme]\nname = \"x\"\n\n# Ayu Dark, as applied by Theme Picker.\n[theme.custom]\naccent = \"#000000\"\n").unwrap();
        replace_custom_tokens(&mut doc, &tokens()).unwrap();
        let text = doc.text();
        assert!(!text.contains("Ayu") && text.contains("name = \"x\"\n\n[theme.custom]\n"), "{text}");
    }

    #[test]
    fn payload_sets_every_color() {
        let p = osc_payload(&parse_palette(BODY).unwrap());
        assert!(p.starts_with("\x1b]10;#cdd6f4\x07\x1b]11;#1e1e2e\x07"));
        assert!(p.ends_with("\x1b]4;15;#a6adc8\x07"));
    }

    // A server start with no theme picked must neither wait nor touch any
    // terminal.
    #[test]
    fn startup_sync_without_a_theme_returns_at_once() {
        let _env = crate::ui::tests::isolated();
        let _ = fs::remove_file(applied_file());
        let started = Instant::now();
        sync_at_startup().unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    // A reset hands the colors back to Herdr but keeps the user's auto_switch
    // layers, and leaves no empty [theme.custom] behind.
    #[test]
    fn reset_drops_tokens_and_keeps_layers() {
        let tokens = parse_palette(BODY).unwrap().tokens();
        let mut doc = Doc::parse("[theme]\nname = \"vesper\"\n\n[theme.custom.dark]\naccent = \"#ffffff\"\n").unwrap();
        replace_custom_tokens(&mut doc, &tokens).unwrap();
        doc.raw()["theme"]["custom"]["stray"] = toml_edit::value(1);
        drop_custom_tokens(&mut doc).unwrap();
        let decoded = doc.decoded();
        let custom = config::lookup(&decoded, &["theme".into(), "custom".into()]).unwrap().as_table().unwrap();
        assert_eq!(custom.keys().collect::<Vec<_>>(), ["dark"]);
        assert_eq!(custom["dark"]["accent"].as_str(), Some("#ffffff"));
        let mut doc = Doc::parse("[ui]\nsidebar_width = 30\n").unwrap();
        replace_custom_tokens(&mut doc, &tokens).unwrap();
        drop_custom_tokens(&mut doc).unwrap();
        assert!(!doc.text().contains("custom"), "{}", doc.text());
        let mut doc = Doc::parse("[ui]\nsidebar_width = 30\n").unwrap();
        drop_custom_tokens(&mut doc).unwrap();
        assert_eq!(doc.text(), "[ui]\nsidebar_width = 30\n");
    }

    // Herdr Radar keeps one token in its own marked [theme.custom] block; a
    // reset must leave it, markers and all, and count the built-in as in use.
    #[test]
    fn reset_keeps_a_partial_table() {
        let text = "[ui]\nsidebar_width = 30\n\n# >>> herdr-radar theme block\n[theme.custom]\nactive_row_bg = \"#414868\"\n# <<< herdr-radar theme block\n";
        let mut doc = Doc::parse(text).unwrap();
        drop_custom_tokens(&mut doc).unwrap();
        assert_eq!(doc.text(), text);
        let _guard = crate::ui::tests::isolated();
        fs::write(config::path(), text).unwrap();
        assert!(builtin().unwrap().in_use);
        write_custom_block(&config::path(), &parse_palette(BODY).unwrap().tokens()).unwrap();
        assert!(!builtin().unwrap().in_use);
    }
}
