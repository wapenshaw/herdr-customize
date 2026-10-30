//! Drives the popup without a terminal: keys go in, background jobs run
//! inline, and the screen comes back as text. Every path the program touches
//! points into a sandbox, and herdr is a script that logs its calls.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use super::*;
use crate::config::{self, Value};

/// UI tests share the process environment, so they run one at a time.
static LOCK: Mutex<()> = Mutex::new(());

#[cfg(unix)]
const FAKE_HERDR: (&str, &str) = (
    "herdr",
    r#"#!/bin/sh
dir=$(dirname "$0")
echo "$*" >> "$dir/herdr.log"
case "$1 $2" in
  "--version ") echo "herdr 0.9.5" ;;
  "config check") exit 0 ;;
  "server reload-config") exit 0 ;;
  "plugin list") cat "$dir/plugins.json" ;;
  "plugin config-dir") echo "$dir/pc/$3" ;;
  "plugin log") echo '{"result":{"logs":[]}}' ;;
  *) echo '{"result":{}}' ;;
esac
"#,
);

#[cfg(windows)]
const FAKE_HERDR: (&str, &str) = (
    "herdr.cmd",
    r#"@echo off
set "dir=%~dp0"
>>"%dir%herdr.log" echo %*
if "%~1"=="--version" (echo herdr 0.9.5& exit /b 0)
if "%~1 %~2"=="config check" exit /b 0
if "%~1 %~2"=="server reload-config" exit /b 0
if "%~1 %~2"=="plugin list" (type "%dir%plugins.json"& exit /b 0)
if "%~1 %~2"=="plugin config-dir" (echo %dir%pc\%~3& exit /b 0)
if "%~1 %~2"=="plugin log" (echo {"result":{"logs":[]}}& exit /b 0)
echo {"result":{}}
"#,
);

const PLUGINS: &str = r#"{"result":{"plugins":[{"plugin_id":"demo","name":"Demo","version":"1.0.0","description":"A demo plugin","enabled":true,"plugin_root":"/nonexistent","source":{"kind":"local"},"actions":[{"id":"hello","title":"Say hello"}]},{"plugin_id":"remote","name":"Remote","version":"1.0.0","description":"A GitHub plugin","enabled":true,"plugin_root":"/nonexistent","source":{"kind":"github","owner":"someone","repo":"remote","requested_ref":"main","resolved_commit":"0000000000000000000000000000000000000000"},"actions":[]}]}}"#;

fn sandbox() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("herdr-customize-ui-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("state")).unwrap();
        let herdr = dir.join(FAKE_HERDR.0);
        fs::write(&herdr, FAKE_HERDR.1).unwrap();
        #[cfg(unix)]
        fs::set_permissions(&herdr, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        // SAFETY: runs once, under the driver's lock, which every test that
        // touches the environment holds.
        let set = |k: &str, v: &Path| unsafe { std::env::set_var(k, v) };
        set("HERDR_BIN_PATH", &herdr);
        set("HERDR_CONFIG_PATH", &dir.join("config.toml"));
        set("HERDR_PLUGIN_STATE_DIR", &dir.join("state"));
        set("HERDR_PLUGIN_ROOT", Path::new(env!("CARGO_MANIFEST_DIR")));
        // Applying a theme writes terminal scheme files under these.
        for var in ["HOME", "XDG_CONFIG_HOME", "APPDATA", "LOCALAPPDATA"] {
            set(var, &dir.join("home").join(var));
        }
        // Nothing reaches GitHub, an editor, or a live Herdr session.
        set("HERDR_CUSTOMIZE_GIT_BASE", &dir.join("no-such-remote"));
        set("GIT_TRACE", &dir.join("git.log"));
        set("HERDR_CUSTOMIZE_RAW_BASE", Path::new("http://127.0.0.1:9"));
        set("HERDR_CUSTOMIZE_API_BASE", Path::new("http://127.0.0.1:9"));
        set("HERDR_CUSTOMIZE_NO_TOKEN", Path::new("1"));
        set("VISUAL", Path::new("false"));
        set("EDITOR", Path::new("false"));
        unsafe { std::env::remove_var("HERDR_ENV") };
        unsafe { std::env::remove_var("HERDR_SOCKET_PATH") };
        dir
    })
}

/// Holds the driver's lock with the sandbox in place, for tests elsewhere
/// that write files and so take write locks in the state folder.
pub(crate) fn isolated() -> MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    sandbox();
    guard
}

struct Driver {
    app: App,
    _guard: MutexGuard<'static, ()>,
}

impl Driver {
    /// Opens tab with every optional page turned on.
    fn new(tab: &str, config: &str) -> Driver {
        Driver::with_pages(tab, config, "themes\nplugins\n")
    }

    /// Opens tab with the optional pages listed in pages turned on.
    fn with_pages(tab: &str, config: &str, pages: &str) -> Driver {
        let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let dir = sandbox();
        fs::write(dir.join("state/pages"), pages).unwrap();
        fs::write(dir.join("config.toml"), config).unwrap();
        fs::write(dir.join("plugins.json"), PLUGINS).unwrap();
        fs::write(dir.join("herdr.log"), "").unwrap();
        fs::write(dir.join("git.log"), "").unwrap();
        let _ = fs::remove_dir_all(dir.join("pc"));
        let mut d = Driver { app: App::new(tab), _guard: guard };
        d.drain();
        d
    }

    /// Runs queued work until nothing is left. Timers wait for fire().
    fn drain(&mut self) {
        loop {
            self.app.settle();
            assert!(self.app.execs.is_empty(), "a test ran a program on the terminal");
            let jobs = std::mem::take(&mut self.app.jobs);
            let writes = std::mem::take(&mut self.app.writes);
            if jobs.is_empty() && writes.is_empty() && self.app.queue.is_empty() {
                return;
            }
            for write in writes {
                self.app.written(write());
            }
            // In parallel like the app, delivered in order so runs repeat.
            let running: Vec<_> = jobs.into_iter().map(std::thread::spawn).collect();
            for job in running {
                self.app.dispatch(job.join().unwrap());
            }
        }
    }

    /// Delivers every pending timer now.
    fn fire(&mut self) {
        for (_, msg) in std::mem::take(&mut self.app.timers) {
            self.app.dispatch(msg);
        }
        self.drain();
    }

    fn press(&mut self, keys: &[&str]) {
        for k in keys {
            self.app.key(&Key::named(k));
            self.drain();
        }
    }

    fn typ(&mut self, text: &str) {
        for c in text.chars() {
            self.press(&[&c.to_string()]);
        }
    }

    fn screen(&mut self) -> String {
        self.app.view(140, 40).iter().map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>()).collect::<Vec<_>>().join("\n")
    }

    fn config(&self) -> String {
        fs::read_to_string(sandbox().join("config.toml")).unwrap()
    }

    fn herdr_calls(&self, prefix: &str) -> usize {
        fs::read_to_string(sandbox().join("herdr.log")).unwrap().lines().filter(|l| l.starts_with(prefix)).count()
    }

    /// Update checks, counted from git's own trace.
    fn git_checks(&self) -> usize {
        fs::read_to_string(sandbox().join("git.log")).unwrap().lines().filter(|l| l.contains("ls-remote")).count()
    }
}

/// A boolean Herdr setting outside the keys and theme sections.
fn boolean_key(sh: &Shared) -> schema::Key {
    sh.reference.sections.iter().filter(|s| s.id != "keys" && s.id != "theme").flat_map(|s| &s.keys).find(|k| k.kind == schema::BOOLEAN && k.default_value().is_some()).cloned().unwrap()
}

/// An enum setting with at least three values.
fn enum_key(sh: &Shared) -> schema::Key {
    sh.reference.sections.iter().filter(|s| s.id != "keys" && s.id != "theme").flat_map(|s| &s.keys).find(|k| !k.open() && k.choices().len() >= 3).cloned().unwrap()
}

// A fresh install shows only Settings and Keys, and says where the rest are.
#[test]
fn optional_pages_start_hidden() {
    let mut d = Driver::with_pages("settings", "", "");
    let tabs = d.screen().lines().next().unwrap().to_string();
    assert!(tabs.contains("1 Settings") && tabs.contains("2 Keys") && !tabs.contains("3 Themes") && tabs.contains("Themes and Plugins can be turned on in Settings"), "{tabs}");
    d.press(&["tab", "tab"]);
    assert_eq!(PAGES[d.app.active], "settings");
    d.press(&["3"]);
    assert_eq!(PAGES[d.app.active], "settings");
}

// Turning a page on in Settings must show it now and in the next popup.
#[test]
fn optional_pages_turn_on_and_off() {
    let mut d = Driver::with_pages("settings", "", "");
    assert!(d.screen().contains("Themes page"), "{}", d.screen());
    d.press(&["right"]);
    assert_eq!(fs::read_to_string(sandbox().join("state/pages")).unwrap(), "themes\n");
    assert!(d.screen().lines().next().unwrap().contains("3 Themes"), "{}", d.screen());
    assert!(d.screen().contains("Themes page on"), "{}", d.screen());
    assert!(App::new("settings").visible[PAGES.iter().position(|p| *p == "themes").unwrap()]);
    d.press(&["x"]);
    assert_eq!(fs::read_to_string(sandbox().join("state/pages")).unwrap(), "");
    assert!(!d.screen().lines().next().unwrap().contains("3 Themes"), "{}", d.screen());
}

// Bindings to herdr-customize.themes and the --remote laptop setup open a
// page by name, so a hidden one still opens for that popup.
#[test]
fn a_hidden_page_opens_by_name() {
    let mut d = Driver::with_pages("themes", "", "");
    assert_eq!(PAGES[d.app.active], "themes");
    let tabs = d.screen().lines().next().unwrap().to_string();
    assert!(tabs.contains("3 Themes") && tabs.contains("Plugins can be turned on in Settings"), "{tabs}");
    assert_eq!(fs::read_to_string(sandbox().join("state/pages")).unwrap(), "");
}

#[test]
fn only_tab_switches_pages() {
    let mut d = Driver::new("themes", "");
    assert!(d.screen().contains("⌕ type to search"));
    d.press(&["tab"]);
    assert_eq!(PAGES[d.app.active], "plugins");
    d.press(&["right", "left"]);
    assert_eq!(PAGES[d.app.active], "plugins");
    d.press(&["tab"]);
    assert_eq!(PAGES[d.app.active], "settings");
    d.press(&["shift+tab", "2"]);
    assert_eq!(PAGES[d.app.active], "keys");
    // The page's own keys must not push tab off a narrow popup.
    assert!(d.screen().lines().last().unwrap().starts_with("tab next page · "));
    d.press(&["esc"]);
    assert!(d.app.quit);
}

#[test]
fn themes_search_and_apply() {
    let mut d = Driver::new("themes", "");
    d.typ("dracula");
    let screen = d.screen();
    assert!(screen.contains("⌕ dracula"), "{screen}");
    assert!(screen.contains("› ") && screen.contains("dracula"));
    // q is typed into the search rather than closing the popup.
    d.typ("q");
    assert!(!d.app.quit);
    d.press(&["backspace", "enter"]);
    assert!(d.app.quit && d.app.after.is_some(), "enter applies the selected theme after closing");
}

#[test]
fn themes_filters_by_kind() {
    let mut d = Driver::new("themes", "");
    d.press(&["right"]);
    let screen = d.screen();
    assert!(screen.contains("Dark") && !screen.contains("LIGHT"), "{screen}");
}

#[test]
fn settings_toggle_shows_new_value_and_saves() {
    let mut d = Driver::new("settings", "");
    let k = boolean_key(&d.app.sh);
    let default = matches!(k.default_value(), Some(Value::Boolean(true)));
    let checks = d.herdr_calls("config check");
    d.typ("/");
    d.typ(&k.key);
    d.press(&["enter", "enter"]);
    assert!(d.config().contains(&format!("= {}", !default)), "{}", d.config());
    assert_eq!(d.herdr_calls("server reload-config"), 1);
    assert_eq!(d.herdr_calls("config check"), checks + 1, "the issue list comes from the save's own check");
    assert!(d.screen().contains("saved and reloaded"));
}

#[test]
fn settings_cycling_writes_once() {
    let mut d = Driver::new("settings", "");
    let k = enum_key(&d.app.sh);
    d.typ("/");
    d.typ(&k.key);
    d.press(&["enter", "right", "right"]);
    assert_eq!(d.herdr_calls("server reload-config"), 0, "nothing is saved while cycling");
    d.fire();
    assert_eq!(d.herdr_calls("server reload-config"), 1);
    let doc = Doc::parse(&d.config()).unwrap();
    let saved = config::lookup(&doc.decoded(), &k.path()).cloned().unwrap();
    let choices = k.choices();
    let current = choices.iter().position(|c| Some(Value::String(c.clone())) == k.default_value()).map_or(-1, |i| i as isize);
    let expected = &choices[((current + 2).rem_euclid(choices.len() as isize)) as usize];
    assert_eq!(saved, Value::String(expected.clone()));
}

#[test]
fn settings_lists_unknown_keys() {
    let mut d = Driver::new("settings", "mystery = 1\n");
    d.typ("/");
    d.typ("mystery");
    let screen = d.screen();
    assert!(screen.contains("not a Herdr setting"), "{screen}");
    d.press(&["enter", "x", "y"]);
    assert!(!d.config().contains("mystery"));
}

#[test]
fn keys_adds_a_custom_command() {
    let mut d = Driver::new("keys", "# my config\n");
    d.press(&["n"]);
    assert!(d.screen().contains("New custom command"));
    d.press(&["ctrl+right"]); // popup → pane
    d.typ("prefix+g");
    d.press(&["tab"]);
    d.typ("lazygit");
    d.press(&["enter"]);
    let config = d.config();
    assert!(config.starts_with("# my config"), "{config}");
    assert!(config.contains("[[keys.command]]") && config.contains("type = \"pane\"") && config.contains("command = \"lazygit\""), "{config}");
    assert!(d.screen().contains("prefix+g"));
}

#[test]
fn bind_adds_the_first_binding_once() {
    let d = Driver::new("keys", "# my config\n");
    let store = config::Store::default_store();
    let checks = d.herdr_calls("config check");
    assert_eq!(crate::bind::bind(&store, None).unwrap(), "Bound prefix+t to the customize popup.");
    let config = d.config();
    assert!(config.starts_with("# my config") && config.contains("key = \"prefix+t\"") && config.contains("command = \"herdr-customize.open\""), "{config}");
    assert_eq!((d.herdr_calls("config check") - checks, d.herdr_calls("server reload-config")), (1, 1));
    // Running it again must not touch the file or reload Herdr.
    assert_eq!(crate::bind::bind(&store, None).unwrap(), "The customize popup already opens with prefix+t.");
    assert_eq!(d.config(), config);
    assert_eq!(d.herdr_calls("server reload-config"), 1);
}

#[test]
fn keys_marks_conflicts() {
    let mut d = Driver::new("keys", "[[keys.command]]\nkey = \"prefix+g\"\ntype = \"shell\"\ncommand = \"a\"\n\n[[keys.command]]\nkey = \"prefix+g\"\ntype = \"shell\"\ncommand = \"b\"\n");
    let screen = d.screen();
    assert!(screen.contains("bindings share a key"), "{screen}");
    assert!(screen.contains("⚠ conflict"));
}

#[test]
fn keys_delete_asks_first() {
    let mut d = Driver::new("keys", "[[keys.command]]\nkey = \"prefix+g\"\ntype = \"pane\"\ncommand = \"lazygit\"\n");
    d.typ("/lazygit");
    d.press(&["enter", "d"]);
    assert!(d.screen().contains("Delete command 1 (lazygit)? [y/N]"));
    d.press(&["n"]);
    assert!(d.screen().contains("cancelled") && d.config().contains("lazygit"));
    d.press(&["d", "y"]);
    assert!(!d.config().contains("lazygit"), "{}", d.config());
}

#[test]
fn themes_delete_asks_first() {
    let mut d = Driver::new("themes", "");
    let body = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("themes/dracula-default")).unwrap();
    crate::theme::save_user_theme("mine", &body, true).unwrap();
    let file = sandbox().join("state/themes/mine");
    d.press(&["ctrl+r"]);
    d.typ("mine");
    d.press(&["ctrl+d"]);
    assert!(d.screen().contains("Delete \"mine\"? [y/N]"));
    d.press(&["n"]);
    assert!(d.screen().contains("cancelled") && file.exists());
    d.press(&["ctrl+d", "y"]);
    assert!(!file.exists());
}

#[test]
fn plugins_unlink_asks_first() {
    let mut d = Driver::new("plugins", "");
    d.press(&["x"]);
    assert!(d.screen().contains("unlink Demo (demo)? [y/N]"));
    d.press(&["n"]);
    assert!(d.screen().contains("cancelled"));
    assert_eq!(d.herdr_calls("plugin unlink"), 0);
    d.press(&["x", "y"]);
    assert_eq!(d.herdr_calls("plugin unlink demo"), 1);
}

#[test]
fn plugins_panel_binds_an_action() {
    let mut d = Driver::new("plugins", "");
    let screen = d.screen();
    assert!(screen.contains("Demo") && screen.contains("Say hello"), "{screen}");
    d.press(&["enter", "b"]);
    assert_eq!(PAGES[d.app.active], "keys");
    let screen = d.screen();
    assert!(screen.contains("New custom command") && screen.contains("demo.hello"), "{screen}");
}

#[test]
fn plugin_settings_are_typed() {
    let mut d = Driver::new("plugins", "");
    let pc = sandbox().join("pc/demo");
    fs::create_dir_all(&pc).unwrap();
    fs::write(pc.join("settings.env"), "# Poll often\n#FAST=false\nNAME=bob\n").unwrap();
    d.press(&["s"]);
    let screen = d.screen();
    assert!(screen.contains("settings.env") && screen.contains("FAST") && screen.contains("[off]"), "{screen}");
    d.press(&["enter"]);
    assert_eq!(fs::read_to_string(pc.join("settings.env")).unwrap(), "# Poll often\n#FAST=false\nFAST=true\nNAME=bob\n");
    assert!(d.screen().contains("saved FAST"));
}

#[test]
fn closing_saves_a_waiting_pick() {
    let mut d = Driver::new("settings", "");
    let k = enum_key(&d.app.sh);
    d.typ("/");
    d.typ(&k.key);
    d.press(&["enter", "right", "q"]);
    assert!(d.app.quit);
    d.app.shutdown();
    d.drain();
    let doc = Doc::parse(&d.config()).unwrap();
    assert!(config::lookup(&doc.decoded(), &k.path()).is_some(), "{}", d.config());
}

// A save that fails on the way out must keep the popup open with its error,
// or the pick is lost without a word.
#[test]
fn closing_keeps_a_failed_save_on_screen() {
    let mut d = Driver::new("settings", "");
    let k = enum_key(&d.app.sh);
    d.typ("/");
    d.typ(&k.key);
    d.press(&["enter", "right", "q"]);
    let cfg = sandbox().join("config.toml");
    fs::remove_file(&cfg).unwrap();
    fs::create_dir(&cfg).unwrap();
    d.app.shutdown();
    d.drain();
    fs::remove_dir(&cfg).unwrap();
    assert!(!d.app.ready_to_close(0));
    assert!(!d.app.quit && d.app.status_err, "{}", d.app.status);
}

// A pick started while the last writes finish would wait on a delay the close
// never waits for.
#[test]
fn closing_ignores_new_input() {
    let mut d = Driver::new("settings", "");
    let k = enum_key(&d.app.sh);
    d.typ("/");
    d.typ(&k.key);
    d.press(&["enter", "right", "q"]);
    d.app.shutdown();
    d.press(&["right"]);
    assert!(!d.app.timers.iter().any(|(_, m)| matches!(m, Msg::PendingSave { .. })));
}

// A save stuck in Herdr must not leave the popup open with no way out.
#[test]
fn ctrl_c_closes_without_waiting() {
    let mut d = Driver::new("settings", "");
    let k = enum_key(&d.app.sh);
    d.typ("/");
    d.typ(&k.key);
    d.press(&["enter", "right", "q"]);
    d.app.after = Some(Box::new(|| Ok(())));
    d.app.shutdown();
    assert!(!d.app.writes.is_empty());
    assert!(d.screen().contains("ctrl+c closes without waiting"), "{}", d.screen());
    d.app.key(&Key::named("x"));
    assert!(!d.app.abandon && d.app.after.is_some());
    d.app.key(&Key::named("ctrl+c"));
    assert!(d.app.abandon && d.app.after.is_none());
}

// A save that succeeds after one that failed must not hide the failure.
#[test]
fn closing_keeps_the_first_failure() {
    let mut d = Driver::new("settings", "");
    d.app.shutdown();
    d.app.written(Msg::Saved(Saved { what: "first".into(), err: Some(anyhow::anyhow!("disk full")), ..Default::default() }));
    let changed = config::SaveResult { changed: true, ..Default::default() };
    d.app.written(Msg::Saved(Saved { what: "second".into(), result: changed, ..Default::default() }));
    assert!(!d.app.ready_to_close(0));
    assert!(d.app.status_err && d.app.status.contains("disk full"), "{}", d.app.status);
}

// The pick's delayed save would otherwise put back the value just reset, or
// set one the reset meant to leave unset.
#[test]
fn reset_drops_a_waiting_pick() {
    let k = enum_key(&Driver::new("settings", "").app.sh);
    let mut set = Doc::default();
    set.set(&k.path(), &Value::String(k.choices()[0].clone())).unwrap();
    for start in [set.text(), String::new()] {
        let mut d = Driver::new("settings", &start);
        d.typ("/");
        d.typ(&k.key);
        d.press(&["enter", "right", "x"]);
        d.fire();
        let doc = Doc::parse(&d.config()).unwrap();
        assert!(config::lookup(&doc.decoded(), &k.path()).is_none(), "{}", d.config());
    }
}

#[test]
fn plugin_settings_reset_drops_a_waiting_pick() {
    for start in ["#FAST=false\nFAST=true\n", "#FAST=false\n"] {
        let mut d = Driver::new("plugins", "");
        let pc = sandbox().join("pc/demo");
        fs::create_dir_all(&pc).unwrap();
        fs::write(pc.join("a.env"), start).unwrap();
        d.press(&["s", "right", "x"]);
        d.fire();
        assert_eq!(fs::read_to_string(pc.join("a.env")).unwrap(), "#FAST=false\n");
    }
}

// A slow write must not let the one queued after it land first.
#[test]
fn writes_finish_in_order() {
    let (tx, rx) = mpsc::channel();
    let (writer, done) = start_writer(tx);
    let slow: Job = Box::new(|| {
        std::thread::sleep(Duration::from_millis(100));
        Msg::PendingSave { seq: 1 }
    });
    writer.send(slow).unwrap();
    writer.send(Box::new(|| Msg::PendingSave { seq: 2 })).unwrap();
    drop(writer);
    done.join().unwrap();
    let seqs: Vec<u64> = rx.try_iter().filter_map(|(m, _)| if let Msg::PendingSave { seq } = m { Some(seq) } else { None }).collect();
    assert_eq!(seqs, [1, 2]);
}

// A pick waiting in one file must not be lost when a pick starts in another.
#[test]
fn plugin_settings_picks_in_two_files_write_both() {
    let mut d = Driver::new("plugins", "");
    let pc = sandbox().join("pc/demo");
    fs::create_dir_all(&pc).unwrap();
    fs::write(pc.join("a.env"), "# Poll often\n#FAST=false\n").unwrap();
    fs::write(pc.join("b.env"), "# Poll rarely\n#FAST=false\n").unwrap();
    d.press(&["s", "right", "down", "right"]);
    d.fire();
    assert_eq!(fs::read_to_string(pc.join("a.env")).unwrap(), "# Poll often\n#FAST=false\nFAST=true\n");
    assert_eq!(fs::read_to_string(pc.join("b.env")).unwrap(), "# Poll rarely\n#FAST=false\nFAST=true\n");
}

// A save reloads the list; the cursor must stay on the same file's row even
// when another file has a row with the same label.
#[test]
fn plugin_settings_cursor_stays_on_the_saved_file() {
    let mut d = Driver::new("plugins", "");
    let pc = sandbox().join("pc/demo");
    fs::create_dir_all(&pc).unwrap();
    fs::write(pc.join("a.env"), "# Poll often\n#FAST=false\n").unwrap();
    fs::write(pc.join("b.env"), "# Poll often\n#FAST=false\n").unwrap();
    d.press(&["s", "down", "enter", "enter"]);
    assert_eq!(fs::read_to_string(pc.join("a.env")).unwrap(), "# Poll often\n#FAST=false\n");
    assert_ne!(fs::read_to_string(pc.join("b.env")).unwrap(), "# Poll often\n#FAST=false\n");
}

#[test]
fn key_sheet_opens_and_closes() {
    let mut d = Driver::new("plugins", "");
    d.press(&["?"]);
    assert!(d.screen().contains("Keys · Plugins"));
    d.press(&["x"]);
    assert!(!d.screen().contains("Keys · Plugins"));
}

// A theme apply must pass Herdr's config check like any other config edit,
// and must leave the reload until after the terminal sync.
#[test]
fn theme_block_is_checked_and_not_reloaded() {
    let d = Driver::new("themes", "[theme.custom.dark]\naccent = \"#ffffff\"\n");
    let checks = d.herdr_calls("config check");
    let pal = crate::theme::parse_palette_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join("themes/dracula-default")).unwrap();
    let saved = crate::theme::write_custom_block(&sandbox().join("config.toml"), &pal.tokens()).unwrap();
    assert!(saved.changed);
    assert_eq!(d.herdr_calls("config check"), checks + 1);
    assert_eq!(d.herdr_calls("server reload-config"), 0);
    let config = d.config();
    assert!(config.contains("[theme.custom]\npanel_bg = ") && config.contains("[theme.custom.dark]\naccent = \"#ffffff\""), "{config}");
}

// Pages rebuild from the config a save wrote instead of reading it again.
#[test]
fn saves_rebuild_from_what_they_wrote() {
    let mut d = Driver::new("settings", "");
    let config: toml::Table = toml::from_str("zz_probe = 1\n[[keys.command]]\nkey = \"f9\"\ntype = \"shell\"\ncommand = \"zz-probe\"\n").unwrap();
    d.app.dispatch(Msg::Saved(Saved { what: "probe".into(), config: Some(config), ..Default::default() }));
    d.typ("/");
    d.typ("zz_probe");
    assert!(d.screen().contains("not a Herdr setting"), "{}", d.screen());
    d.press(&["esc", "tab"]);
    d.typ("/");
    d.typ("zz-probe");
    assert!(d.screen().contains("f9"), "{}", d.screen());
}

// Themes and the process scan cost the popup's start, so they wait until
// the Themes page is shown.
#[test]
fn themes_load_when_their_page_opens() {
    {
        let _env = isolated();
        let jobs = |tab| App::new(tab).jobs.len();
        assert_eq!(jobs("themes"), jobs("settings") + 2);
    }
    let mut d = Driver::new("keys", "");
    assert!(!d.screen().contains("dracula"));
    d.press(&["tab"]);
    assert_eq!(PAGES[d.app.active], "themes");
    assert!(d.screen().to_lowercase().contains("dracula"), "{}", d.screen());
}

/// Opens the plugins page on seven GitHub plugins, two of them one repo and ref.
fn seven_github_plugins() -> Driver {
    let mut d = Driver::new("themes", "");
    let gh = |id: &str, repo: &str| {
        format!(
            r#"{{"plugin_id":"{id}","name":"{id}","version":"1","enabled":true,"plugin_root":"/x","source":{{"kind":"github","owner":"o","repo":"{repo}","requested_ref":"main","resolved_commit":"0"}},"actions":[]}}"#
        )
    };
    let list = [gh("a", "one"), gh("b", "one"), gh("c", "two"), gh("d", "three"), gh("e", "four"), gh("f", "five"), gh("g", "six")];
    fs::write(sandbox().join("plugins.json"), format!(r#"{{"result":{{"plugins":[{}]}}}}"#, list.join(","))).unwrap();
    d.app.dispatch(Msg::PluginList(crate::plugins::list()));
    d.press(&["tab"]);
    d
}

// A reload while checks still run must not start more than the cap allows.
#[test]
fn plugin_checks_stay_capped_across_reloads() {
    if !crate::plugins::have_git() {
        return;
    }
    let mut d = seven_github_plugins();
    // The jobs are dropped unrun, so the first reload's checks never finish.
    let mut reload = || {
        d.app.dispatch(Msg::PluginList(crate::plugins::list()));
        d.app.settle();
        std::mem::take(&mut d.app.jobs).len()
    };
    let (first, second) = (reload(), reload());
    assert_eq!(first - second, 4);
}

// Installs that follow one repo and ref must share one update check.
#[test]
fn plugin_checks_share_a_repo_and_ref() {
    let mut d = seven_github_plugins();
    assert_eq!(PAGES[d.app.active], "plugins");
    assert_eq!(d.git_checks(), if crate::plugins::have_git() { 6 } else { 0 });
    if crate::plugins::have_git() {
        assert!(d.screen().contains("7 not"), "{}", d.screen());
    }
}

// Opening the popup on another page must not start update checks.
#[test]
fn plugin_checks_wait_for_the_page() {
    let mut d = Driver::new("themes", "");
    assert_eq!(d.git_checks(), 0);
    assert_eq!(d.herdr_calls("plugin config-dir"), 0);
    d.press(&["tab"]);
    assert_eq!(PAGES[d.app.active], "plugins");
    assert_eq!(d.git_checks(), usize::from(crate::plugins::have_git()));
    assert_eq!(d.herdr_calls("plugin config-dir"), 1);
}

// With no [theme.custom] colors Herdr shows its own theme; the list names it,
// marks it in use, and picking it resets.
#[test]
fn themes_offer_herdr_default() {
    let mut d = Driver::new("themes", "[theme]\nname = \"vesper\"\n");
    let screen = d.screen();
    assert!(screen.contains("IN USE") && screen.contains("● Herdr default (vesper)"), "{screen}");
    d.typ("default");
    d.press(&["enter"]);
    assert!(d.app.quit && d.app.after.is_some(), "enter resets after closing");
}

// A recorded selection whose colors are gone from the config is not in use,
// so the ● follows what Herdr actually shows.
#[test]
fn themes_mark_only_what_the_config_holds() {
    let mut d = Driver::new("themes", "");
    fs::write(sandbox().join("state/applied"), "dracula-default\n").unwrap();
    d.press(&["ctrl+r"]);
    let screen = d.screen();
    assert!(screen.contains("● Herdr default (catppuccin)") && !screen.contains("● dracula-default"), "{screen}");
    let pal = crate::theme::parse_palette_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join("themes/dracula-default")).unwrap();
    crate::theme::write_custom_block(&sandbox().join("config.toml"), &pal.tokens()).unwrap();
    d.press(&["ctrl+r"]);
    let mut screen = d.screen();
    d.press(&["home"]);
    screen += &d.screen();
    let _ = fs::remove_file(sandbox().join("state/applied"));
    assert!(screen.contains("● dracula-default") && screen.contains("↩ Herdr default (catppuccin)"), "{screen}");
}
