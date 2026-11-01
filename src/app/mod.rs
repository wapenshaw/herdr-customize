//! The Herdr desktop app (herdr-app). It reads its whole interface from
//! files in its home and follows edits live, so this module writes those
//! files and calls its CLI. See docs/app.md.

pub mod api;
pub mod keys;
pub mod mods;
pub mod themes;

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::config::env;
use crate::{fsutil, proc};

/// The mod that opens the popup in the app, and the folder of its data.
pub const MOD_NAME: &str = "herdr-customize";

/// The app's home: HERDR_APP_HOME, else XDG_CONFIG_HOME/herdr-app, else
/// ~/.config/herdr-app. The app ignores HOME on Windows, so this does too.
pub fn home() -> PathBuf {
    if let Some(home) = env("HERDR_APP_HOME") {
        return home.into();
    }
    if let Some(xdg) = env("XDG_CONFIG_HOME") {
        return Path::new(&xdg).join("herdr-app");
    }
    let user = if cfg!(windows) { env("USERPROFILE") } else { env("HOME") };
    user.map_or_else(|| PathBuf::from(".config"), |u| Path::new(&u).join(".config")).join("herdr-app")
}

/// herdr-customize's own records in the app's home, where the app keeps
/// each mod's data.
pub fn state_dir() -> PathBuf {
    home().join("data").join(MOD_NAME)
}

/// The herdr-app executable: HERDR_APP_BIN, PATH, then where the app's
/// installers put it.
pub fn bin() -> Option<PathBuf> {
    if let Some(bin) = env("HERDR_APP_BIN") {
        return Some(bin.into());
    }
    if let Some(found) = proc::look_path("herdr-app") {
        return Some(found);
    }
    let mut places = Vec::new();
    if cfg!(windows)
        && let Some(local) = env("LOCALAPPDATA")
    {
        places.push(Path::new(&local).join("Programs").join("herdr-app").join("herdr-app.exe"));
    }
    if let Some(user) = env(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        places.push(Path::new(&user).join(".local").join("bin").join("herdr-app"));
    }
    places.into_iter().find(|p| p.is_file())
}

/// herdr-app with args, aimed at the window that owns home().
fn command(args: &[String]) -> Result<Command> {
    let bin = bin().ok_or_else(|| anyhow!("herdr-app not found: install the Herdr app, or set HERDR_APP_BIN"))?;
    let mut cmd = Command::new(bin);
    cmd.args(args).env("HERDR_APP_HOME", home());
    Ok(cmd)
}

/// Runs herdr-app and returns what it printed, failing with its message.
fn output(args: &[String], timeout: Duration) -> Result<String> {
    let what = format!("herdr-app {}", args.first().map_or("", String::as_str));
    let out = proc::output_within(&mut command(args)?, timeout).with_context(|| what.clone())?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Reply {
        error: String,
    }
    let reply: Reply = serde_json::from_str(stdout.trim()).unwrap_or_default();
    if !reply.error.is_empty() {
        bail!("{what}: {}", reply.error);
    }
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let reason = stderr.lines().chain(stdout.lines()).map(str::trim).find(|l| !l.is_empty()).unwrap_or("failed");
        bail!("{what}: {reason}");
    }
    Ok(stdout)
}

/// Runs an app command in the open window, as a key in keys.json would.
pub fn run(name: &str, args: &[(&str, &str)]) -> Result<()> {
    let mut a = vec!["do".to_string(), name.to_string()];
    for (k, v) in args {
        a.extend([format!("--{k}"), (*v).to_string()]);
    }
    output(&a, Duration::from_secs(10)).with_context(|| format!("is the Herdr app open? ({name})")).map(drop)
}

/// One app command, as `herdr-app commands` lists them.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AppCommand {
    pub name: String,
}

/// Every app command. It needs no open window.
pub fn commands() -> Result<Vec<AppCommand>> {
    let text = output(&["commands".into()], Duration::from_secs(10))?;
    serde_json::from_str(&text).context("herdr-app commands")
}

/// Loads a mod once in the app's checker and reports what is wrong with it.
pub fn check_mod(dir: &Path) -> Result<()> {
    output(&["check".into(), dir.to_string_lossy().into_owned()], Duration::from_secs(30)).map(drop)
}

/// A short, stable fingerprint of a file's bytes (FNV-1a), to tell a file
/// herdr-customize wrote from one changed since.
pub fn digest(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3));
    format!("{hash:016x}")
}

/// Reads one of herdr-customize's records; a missing file is empty.
fn read_record<T: DeserializeOwned + Default>(name: &str) -> Result<T> {
    let path = state_dir().join(name);
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).with_context(|| path.display().to_string()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e).with_context(|| path.display().to_string()),
    }
}

fn write_record(name: &str, value: &impl serde::Serialize) -> Result<()> {
    fs::create_dir_all(state_dir())?;
    fsutil::atomic_write(&state_dir().join(name), (serde_json::to_string_pretty(value)? + "\n").as_bytes(), 0o644)
}

/// Reads a JSON object file; a missing file is an empty object.
fn read_object(path: &Path) -> Result<serde_json::Map<String, serde_json::Value>> {
    match fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(serde_json::Map::new()),
        Ok(text) => match serde_json::from_str(text.trim_start_matches('\u{feff}'))? {
            serde_json::Value::Object(map) => Ok(map),
            _ => bail!("{} is not a JSON object", path.display()),
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(serde_json::Map::new()),
        Err(e) => Err(e).with_context(|| path.display().to_string()),
    }
}

/// The mods and layouts the app ships and replaces on every start: "agents",
/// "layout:main".
pub fn shipped() -> BTreeSet<String> {
    read_object(&home().join("installed.json")).map(|m| m.into_iter().map(|(k, _)| k).collect()).unwrap_or_default()
}

/// The key install binds when none is given: free in the app and in shells.
pub const DEFAULT_KEY: &str = if cfg!(target_os = "macos") { "cmd-shift-m" } else { "ctrl-shift-m" };

/// What install did with the key.
#[derive(Debug, PartialEq, Eq)]
pub enum Bound {
    Added(String),
    /// Some key already opened the popup.
    Already(String),
}

/// Writes the mod that opens this binary's popup in the app, checks it, and
/// binds key (or DEFAULT_KEY) to it unless a key already opens it.
pub fn install_popup(key: Option<&str>) -> Result<Bound> {
    let exe = std::env::current_exe().context("find this program")?;
    let dir = home().join("mods").join(MOD_NAME);
    fs::create_dir_all(&dir)?;
    let bin = serde_json::to_string(&exe.to_string_lossy())?;
    let js = include_str!("../../app/mods/herdr-customize/main.js");
    fsutil::atomic_write(&dir.join("main.js"), js.replace("\"@BIN@\"", &bin).as_bytes(), 0o644)?;
    fsutil::atomic_write(&dir.join("mod.json"), include_str!("../../app/mods/herdr-customize/mod.json").as_bytes(), 0o644)?;
    check_mod(&dir)?;
    let action = format!("mod.open:{MOD_NAME}");
    let bindings = keys::load()?;
    if let Some(b) = bindings.iter().find(|b| b.section == "global" && b.action == action) {
        return Ok(Bound::Already(b.key.clone()));
    }
    let key = keys::normalize(key.unwrap_or(DEFAULT_KEY));
    if let Some(e) = keys::problems("global", &key).0.first() {
        bail!("{e}");
    }
    if let Some(b) = bindings.iter().find(|b| b.section == "global" && keys::normalize(&b.key) == key) {
        bail!("{} already runs {}; pick another key: herdr-customize app install <key>", b.key, b.action);
    }
    keys::set("global", None, &key, &action)?;
    Ok(Bound::Added(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The mod lands where the app loads it, aimed at this binary, and the
    // key is added once and never over another binding.
    #[test]
    fn install_popup_binds_a_free_key() {
        let _guard = crate::ui::tests::isolated();
        let _ = fs::remove_dir_all(home());
        fs::create_dir_all(home()).unwrap();
        fs::write(keys::path(), format!(r#"{{"global":{{"{DEFAULT_KEY}":"tab.new"}}}}"#)).unwrap();
        assert!(install_popup(None).unwrap_err().to_string().contains("already runs tab.new"));
        assert_eq!(install_popup(Some("alt-shift-c")).unwrap(), Bound::Added("alt-shift-c".into()));
        assert_eq!(install_popup(Some("f9")).unwrap(), Bound::Already("alt-shift-c".into()));
        let js = fs::read_to_string(home().join("mods").join(MOD_NAME).join("main.js")).unwrap();
        let exe = std::env::current_exe().unwrap();
        assert!(js.contains(&serde_json::to_string(&exe.to_string_lossy()).unwrap()) && !js.contains("@BIN@"));
        let _ = fs::remove_dir_all(home());
    }

    // Manifests compare these across runs and versions, so the value is fixed.
    #[test]
    fn digest_is_stable() {
        assert_eq!(digest(b""), "cbf29ce484222325");
        assert_eq!(digest(b"a"), "af63dc4c8601ec8c");
    }
}
