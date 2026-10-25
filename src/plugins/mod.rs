//! Installed Herdr plugins through the herdr plugin CLI, update checks
//! against GitHub, and marketplace search.

mod market;
mod update;

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::Deserialize;

pub use market::{MARKET_TOPIC, Repo, Results, search};
pub use update::{State, Status, check_all, have_git, resolve_remote};

use crate::{config, herdr, proc};

/// One installed or linked plugin.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Plugin {
    #[serde(rename = "plugin_id")]
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    #[serde(rename = "plugin_root")]
    pub root: String,
    pub source: Source,
    pub actions: Vec<Action>,
}

/// Where a plugin came from.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Source {
    /// "github" for installs and something else for linked checkouts.
    pub kind: String,
    pub owner: String,
    pub repo: String,
    pub subdir: String,
    /// The branch, tag or commit named at install time.
    pub requested_ref: String,
    #[serde(rename = "ref")]
    pub legacy_ref: String,
    #[serde(rename = "resolved_commit")]
    pub commit: String,
}

/// A plugin action that a keybinding or the Plugins page can run.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Action {
    pub id: String,
    pub title: String,
    pub platforms: Vec<String>,
}

impl Plugin {
    /// Installed from GitHub, rather than linked from a checkout.
    pub fn github(&self) -> bool {
        self.source.kind == "github" && !self.source.owner.is_empty() && !self.source.repo.is_empty()
    }

    /// owner/repo for GitHub installs.
    pub fn slug(&self) -> String {
        format!("{}/{}", self.source.owner, self.source.repo)
    }

    /// What herdr plugin install takes: owner/repo[/subdir].
    pub fn spec(&self) -> String {
        if self.source.subdir.is_empty() { self.slug() } else { format!("{}/{}", self.slug(), self.source.subdir) }
    }

    /// The requested install ref, or "" for the default branch.
    pub fn git_ref(&self) -> &str {
        if self.source.requested_ref.is_empty() { &self.source.legacy_ref } else { &self.source.requested_ref }
    }

    /// The plugin's GitHub page, at the installed commit for subdir plugins.
    pub fn repo_url(&self) -> String {
        let mut url = format!("https://github.com/{}", self.slug());
        if !self.source.subdir.is_empty() && !self.source.commit.is_empty() {
            url += &format!("/tree/{}/{}", self.source.commit, self.source.subdir);
        }
        url
    }
}

impl Action {
    /// The plugin.action name keybindings use.
    pub fn qualified(&self, plugin: &str) -> String {
        format!("{plugin}.{}", self.id)
    }

    /// Whether the action runs on this OS.
    pub fn available(&self) -> bool {
        let os = if cfg!(target_os = "macos") { "macos" } else { std::env::consts::OS };
        self.platforms.is_empty() || self.platforms.iter().any(|p| p == os)
    }
}

/// The installed plugins.
pub fn list() -> Result<Vec<Plugin>> {
    parse_list(&herdr::run(&["plugin", "list", "--json"]).context("herdr plugin list")?)
}

/// Decodes herdr plugin list --json output.
pub fn parse_list(data: &str) -> Result<Vec<Plugin>> {
    #[derive(Deserialize)]
    struct Payload {
        result: Inner,
    }
    #[derive(Deserialize)]
    struct Inner {
        #[serde(default)]
        plugins: Vec<Plugin>,
    }
    Ok(serde_json::from_str::<Payload>(data).context("unexpected herdr plugin list output")?.result.plugins)
}

pub fn set_enabled(id: &str, enabled: bool) -> Result<()> {
    herdr::run(&["plugin", if enabled { "enable" } else { "disable" }, id]).map(|_| ())
}

/// Uninstalls a GitHub plugin or unlinks a local checkout.
pub fn remove(p: &Plugin) -> Result<()> {
    herdr::run(&["plugin", if p.github() { "uninstall" } else { "unlink" }, &p.id]).map(|_| ())
}

/// One recorded plugin command run.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Log {
    pub command: Vec<String>,
    pub event: String,
    #[serde(rename = "action_id")]
    pub action: String,
    pub status: String,
    pub exit_code: Option<i32>,
    #[serde(rename = "started_unix_ms")]
    pub started: i64,
    #[serde(rename = "finished_unix_ms")]
    pub finished: i64,
    pub stdout: String,
    pub stderr: String,
}

impl Log {
    /// What ran the command: an event or an action.
    pub fn trigger(&self) -> String {
        if !self.event.is_empty() {
            self.event.clone()
        } else if !self.action.is_empty() {
            self.action.clone()
        } else {
            self.command.join(" ")
        }
    }

    pub fn started_at(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(self.started.max(0) as u64)
    }
}

/// The most recent command logs for a plugin, newest first.
pub fn logs(id: &str, limit: usize) -> Result<Vec<Log>> {
    #[derive(Deserialize)]
    struct Payload {
        result: Inner,
    }
    #[derive(Deserialize)]
    struct Inner {
        #[serde(default)]
        logs: Vec<Log>,
    }
    let out = herdr::run(&["plugin", "log", "list", "--plugin", id, "--limit", &limit.to_string()])?;
    let mut logs = serde_json::from_str::<Payload>(&out).context("herdr plugin log list")?.result.logs;
    logs.reverse();
    Ok(logs)
}

/// Herdr's record of installed plugins, beside its socket or, failing that,
/// beside config.toml.
pub fn registry_path() -> PathBuf {
    if let Some(socket) = config::env("HERDR_SOCKET_PATH").filter(|_| !cfg!(windows)) {
        let path = PathBuf::from(socket).with_file_name("plugins.json");
        if path.exists() {
            return path;
        }
    }
    config::path().with_file_name("plugins.json")
}

/// A herdr plugin config-dir answer for id.
pub fn config_dir(id: &str) -> Result<PathBuf> {
    let dir = herdr::run(&["plugin", "config-dir", id])?.trim().to_string();
    if dir.is_empty() {
        anyhow::bail!("herdr gave no config directory for {id}");
    }
    Ok(dir.into())
}

/// Runs action once this popup has closed. Herdr refuses to open UI from an
/// action while a popup is showing, so a detached copy of this program waits
/// for the popup process to exit and invokes it then.
pub fn invoke_after_close(action: &str) -> Result<()> {
    proc::start_detached(&std::env::current_exe()?, &["invoke-after-close", &std::process::id().to_string(), action])
}

/// The detached half of invoke_after_close.
pub fn run_invoke_after_close(pid: i32, action: &str) -> Result<()> {
    proc::wait_exit(pid, Duration::from_secs(30));
    let mut last = None;
    // Herdr takes a moment to return to the workspace view.
    for _ in 0..15 {
        match herdr::run(&["plugin", "action", "invoke", action]) {
            Ok(_) => return Ok(()),
            Err(e) => {
                let message = e.to_string();
                if !message.contains("ui_busy") && !message.contains("popup already open") {
                    return Err(e);
                }
                last = Some(e);
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("herdr stayed busy")))
}

/// A command that runs herdr with args on the terminal, for installs and
/// updates whose trust prompt needs it, then waits for enter so the outcome
/// stays readable.
pub fn interactive(args: &[String]) -> Result<Command> {
    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.arg("herdr-run").args(args);
    Ok(cmd)
}

/// The body of interactive's command. Returns the exit code.
pub fn run_interactive(args: &[String]) -> i32 {
    println!("\n$ herdr {}\n", args.join(" "));
    let code = match herdr::command(args).status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            println!("{e}");
            1
        }
    };
    if code == 0 {
        print!("\n✓ done");
    } else {
        print!("\n✗ cancelled or failed (exit {code})");
    }
    print!(" · press enter to return ");
    let _ = std::io::stdout().flush();
    let _ = std::io::stdin().lock().read_line(&mut String::new());
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plugin_list() {
        let list = parse_list(r#"{"result":{"plugins":[{"plugin_id":"a","name":"A","enabled":true,"plugin_root":"/x","source":{"kind":"github","owner":"o","repo":"r","subdir":"s","requested_ref":"","ref":"v1","resolved_commit":"abc"},"actions":[{"id":"go","title":"Go","platforms":["plan9"]}]}]}}"#).unwrap();
        let p = &list[0];
        assert!(p.github());
        assert_eq!(p.spec(), "o/r/s");
        assert_eq!(p.git_ref(), "v1");
        assert_eq!(p.repo_url(), "https://github.com/o/r/tree/abc/s");
        assert_eq!(p.actions[0].qualified(&p.id), "a.go");
        assert!(!p.actions[0].available());
    }
}
