//! Applying a theme: [theme.custom] in config.toml, the selection marker,
//! the host terminals' colors, and a reload.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use toml_edit::{Item, Table};

use super::{Palette, TOKENS, Tokens, applied_file, parse_palette_file, resolve_palette, state_dir};
use crate::config::{self, Doc, SaveResult, Store, env};
use crate::{fsutil, herdr, terminal};

/// Writes [theme.custom] to this machine's config, records the selection,
/// recolors the client terminals and reloads. Sync and reload are best
/// effort: the config is the truth, and a manual reload repairs either.
pub fn apply_theme(slug: &str) -> Result<()> {
    let cfg = config::path();
    let (pal, palette_path) = load_palette(slug)?;
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

/// The palette for slug, bundled or user-added, with the file it came from.
fn load_palette(slug: &str) -> Result<(Palette, PathBuf)> {
    let path = resolve_palette(slug).with_context(|| format!("cannot resolve theme {slug:?}"))?;
    let pal = parse_palette_file(&path).with_context(|| format!("invalid palette for {slug:?}"))?;
    Ok((pal, path))
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
    if let Item::Table(t) = custom {
        t.set_implicit(false);
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
/// set) and refreshes the user's Ghostty fragment if it exists. Fails with
/// terminal::Unavailable when no client was found.
fn sync_terminal_colors(pal: &Palette, palette_path: &Path, client_pid: &str) -> Result<()> {
    let mut errors: Vec<String> = Vec::new();
    // Ghostty only loads this fragment if the user includes it, so an
    // existing file is the opt-in. Never create it.
    if let Some(home) = config::home() {
        for fragment in [home.join(".config").join("ghostty").join("herdr-theme"), home.join("AppData").join("Roaming").join("ghostty").join("herdr-theme")] {
            if fragment.exists()
                && let Err(e) = fs::read(palette_path).map_err(anyhow::Error::from).and_then(|data| fsutil::atomic_write(&fragment, &data, 0o644))
            {
                errors.push(format!("Ghostty theme: {e:#}"));
            }
        }
    }
    let clients = terminal::active_clients(client_pid)?;
    let payload = osc_payload(pal);
    let mut synced = 0;
    for client in &clients {
        match terminal::emit_client(client, &payload) {
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
    let (pal, palette_path) = load_palette(&slug)?;
    // A config edited independently, or a failed marker write, must never
    // resurrect stale terminal colors from the previous selection.
    let cfg = config::path();
    let doc = Doc::parse(&fs::read_to_string(&cfg).context("read config before sync")?)?.decoded();
    let custom = config::lookup(&doc, &["theme".into(), "custom".into()]).and_then(|v| v.as_table());
    for (key, value) in pal.tokens() {
        let current = custom.and_then(|c| c.get(key)).and_then(|v| v.as_str()).unwrap_or_default();
        if !current.eq_ignore_ascii_case(&value) {
            bail!("saved theme {slug:?} does not match {}; run apply {slug} again before syncing", cfg.display());
        }
    }
    sync_terminal_colors(&pal, &palette_path, client_pid)
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

    #[test]
    fn payload_sets_every_color() {
        let p = osc_payload(&parse_palette(BODY).unwrap());
        assert!(p.starts_with("\x1b]10;#cdd6f4\x07\x1b]11;#1e1e2e\x07"));
        assert!(p.ends_with("\x1b]4;15;#a6adc8\x07"));
    }
}
