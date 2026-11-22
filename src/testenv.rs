//! The sandbox every test that touches files or programs runs in: the app's
//! home sits in a temporary folder, herdr-app is a script that logs its
//! calls, and nothing reaches GitHub, an editor or the real app.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

/// Tests share the process environment, so sandboxed ones run one at a time.
static LOCK: Mutex<()> = Mutex::new(());

#[cfg(unix)]
const FAKE_APP: (&str, &str) = (
    "herdr-app",
    r#"#!/bin/sh
dir=$(dirname "$0")
echo "$*" >> "$dir/herdr-app.log"
case "$1" in
  commands) cat "$dir/commands.json" ;;
  --version) echo 'herdr-app 9.9.9' ;;
  *) echo '{"ok":true}' ;;
esac
"#,
);

#[cfg(windows)]
const FAKE_APP: (&str, &str) = (
    "herdr-app.cmd",
    r#"@echo off
set "dir=%~dp0"
>>"%dir%herdr-app.log" echo %*
if "%~1"=="commands" (type "%dir%commands.json"& exit /b 0)
if "%~1"=="--version" (echo herdr-app 9.9.9& exit /b 0)
echo {"ok":true}
"#,
);

/// What the fake herdr-app lists for `herdr-app commands`.
const COMMANDS: &str = r#"[{"name":"tab.new","args":[],"about":"Open a tab"},{"name":"app.theme","args":[{"name":"name","required":true}],"about":"Set the theme"},{"name":"space.select","args":[{"name":"index","required":true}],"about":"Show a space"},{"name":"mod.open","args":[{"name":"name","required":true}],"about":"Open a mod"}]"#;

static SANDBOX: LazyLock<PathBuf> = LazyLock::new(|| {
    {
        let dir = std::env::temp_dir().join(format!("herdr-customize-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // SAFETY: runs once, under LOCK, which every test that touches the
        // environment holds.
        let set = |k: &str, v: &Path| unsafe { std::env::set_var(k, v) };
        let app = dir.join(FAKE_APP.0);
        fs::write(&app, FAKE_APP.1).unwrap();
        #[cfg(unix)]
        fs::set_permissions(&app, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        fs::write(dir.join("commands.json"), COMMANDS).unwrap();
        set("HERDR_APP_BIN", &app);
        set("HERDR_CORE_BIN", &dir.join("no-hc"));
        // The app's home and herdr-core's config then sit under these.
        for var in ["HOME", "USERPROFILE", "XDG_CONFIG_HOME", "APPDATA", "LOCALAPPDATA"] {
            set(var, &dir.join("home").join(var));
        }
        set("HERDR_CUSTOMIZE_GIT_BASE", &dir.join("no-such-remote"));
        set("GIT_TRACE", &dir.join("git.log"));
        set("HERDR_CUSTOMIZE_RAW_BASE", Path::new("http://127.0.0.1:9"));
        set("HERDR_CUSTOMIZE_API_BASE", Path::new("http://127.0.0.1:9"));
        set("HERDR_CUSTOMIZE_NO_TOKEN", Path::new("1"));
        set("VISUAL", Path::new("false"));
        set("EDITOR", Path::new("false"));
        // SAFETY: as above.
        unsafe { std::env::remove_var("HERDR_APP_HOME") };
        dir
    }
});

/// Holds the lock with the sandbox in place.
pub fn isolated() -> MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    LazyLock::force(&SANDBOX);
    guard
}

/// Empties the app's home, leaving the keys.json a started app has (here
/// with no keys), so edits never fall back to fetching the shipped ones.
pub fn fresh_home() {
    let home = crate::app::home();
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    fs::write(home.join("keys.json"), "{}\n").unwrap();
}
