//! Wraps the herdr CLI: config checks, reloads and plugin commands. Every
//! call honors HERDR_BIN_PATH, which Herdr sets for plugins.

use std::path::Path;
use std::process::{Command, Output};
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use regex::Regex;

use crate::proc::TimedOut;

/// The herdr executable to run.
pub fn bin() -> String {
    std::env::var("HERDR_BIN_PATH").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "herdr".into())
}

/// A herdr invocation that has not run yet.
pub fn command<I, S>(args: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut cmd = Command::new(bin());
    cmd.args(args);
    cmd
}

/// Runs herdr and returns stdout. A failure carries the CLI's own message:
/// herdr prints JSON errors for socket commands and text otherwise.
pub fn run(args: &[&str]) -> Result<String> {
    let out = command(args).output()?;
    let stdout = String::from_utf8_lossy_owned(out.stdout);
    if !out.status.success() {
        let msg = error_message(&format!("{}{}", String::from_utf8_lossy(&out.stderr), stdout));
        if msg.is_empty() {
            bail!("herdr {}: {}", args.join(" "), out.status);
        }
        bail!("{msg}");
    }
    Ok(stdout)
}

/// The first useful line of herdr output, unwrapping
/// {"error":{"message":...}} payloads.
pub fn error_message(output: &str) -> String {
    let Some(line) = output.lines().map(str::trim).find(|l| !l.is_empty()) else { return String::new() };
    let json = serde_json::from_str::<serde_json::Value>(line).ok().filter(|_| line.starts_with('{'));
    match json.as_ref().and_then(|v| v["error"]["message"].as_str()).filter(|m| !m.is_empty()) {
        Some(m) => m.to_string(),
        None => line.to_string(),
    }
}

/// How long a config check or reload may take; a save waits on both, and
/// closing the popup waits on the save.
const CONFIG_DEADLINE: Duration = Duration::from_secs(10);

/// Runs 'herdr server reload-config'. A machine that only connects with
/// herdr --remote has no local server; the user reloads instead.
pub fn reload() -> Result<()> {
    let out = crate::proc::output_within(&mut command(["server", "reload-config"]), CONFIG_DEADLINE)?;
    if !out.status.success() {
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        bail!("{}: {}", out.status, text.trim());
    }
    Ok(())
}

static VERSION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d+\.\d+\.\d+)\b").unwrap());

/// The installed herdr version, such as "0.9.5".
pub fn version() -> Result<String> {
    let out = command(["--version"]).output()?;
    let text = String::from_utf8_lossy(&out.stdout);
    VERSION_RE.captures(&text).map(|c| c[1].to_string()).ok_or_else(|| anyhow!("unrecognized herdr version output: {:?}", text.trim()))
}

/// The outcome of 'herdr config check'.
#[derive(Debug, Default, Clone)]
pub struct CheckResult {
    pub issues: Vec<String>,
    /// Why the check did not run, for example on a machine without herdr or
    /// with a herdr too old to have the command. Empty when it ran.
    pub skipped: String,
}

/// Validates the config at path with herdr's own parser. Fails when herdr
/// started but never answered, which leaves the config unchecked.
pub fn check_config(path: &Path) -> Result<CheckResult> {
    read_check(crate::proc::output_within(command(["config", "check"]).env("HERDR_CONFIG_PATH", path), CONFIG_DEADLINE))
}

fn read_check(out: Result<Output>) -> Result<CheckResult> {
    let out = match out {
        Ok(out) => out,
        Err(e) if e.is::<TimedOut>() => return Err(e.context("herdr config check")),
        Err(e) => return Ok(CheckResult { skipped: format!("{e:#}"), ..Default::default() }),
    };
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let text = text.trim();
    if out.status.success() {
        return Ok(CheckResult::default());
    }
    if out.status.code() == Some(1) && text.starts_with("config: issues found") {
        let issues = text.lines().skip(1).map(|l| l.trim_end_matches('\r')).filter(|l| !l.trim().is_empty()).map(String::from).collect();
        return Ok(CheckResult { issues, ..Default::default() });
    }
    let skipped = if text.is_empty() { out.status.to_string() } else { error_message(text) };
    Ok(CheckResult { skipped, ..Default::default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_message_unwraps_json() {
        assert_eq!(error_message("\n{\"error\":{\"message\":\"ui_busy\"}}\n"), "ui_busy");
        assert_eq!(error_message("  plain failure\nmore"), "plain failure");
        assert_eq!(error_message(""), "");
    }

    // A check that never answered must stop the save; only a herdr that
    // cannot run the check lets it go ahead unchecked.
    #[test]
    fn a_check_that_times_out_fails() {
        assert!(read_check(Err(TimedOut(Duration::ZERO).into())).is_err());
        let missing = read_check(Err(anyhow!("program not found"))).unwrap();
        assert_eq!(missing.skipped, "program not found");
    }
}
