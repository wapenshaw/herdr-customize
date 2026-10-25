//! Whether a GitHub plugin has moved on since it was installed.

use std::collections::HashMap;
use std::process::Command;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use regex::Regex;
use serde::Deserialize;

use super::Plugin;
use crate::config::env;
use crate::proc;

/// Where git ls-remote finds owner/repo; replaced in tests.
fn git_base() -> String {
    env("HERDR_CUSTOMIZE_GIT_BASE").unwrap_or_else(|| "https://github.com".into())
}

/// Serves files at a commit: base/owner/repo/sha/path.
fn raw_base() -> String {
    env("HERDR_CUSTOMIZE_RAW_BASE").unwrap_or_else(|| "https://raw.githubusercontent.com".into())
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Unchecked,
    Current,
    Behind,
    Failed,
}

/// Whether a GitHub plugin has an update.
#[derive(Debug, Clone, Default)]
pub struct Status {
    pub state: State,
    /// The commit an update would move to.
    pub remote: String,
    /// The remote manifest's version, when it could be read.
    pub version: String,
    pub error: String,
}

static SHA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[0-9a-fA-F]{7,40}$").unwrap());

impl Plugin {
    /// Installed at an exact commit, which never moves by itself.
    pub fn pinned(&self) -> bool {
        let r = self.git_ref();
        SHA.is_match(r) && self.source.commit.to_lowercase().starts_with(&r.to_lowercase())
    }
}

/// The commit ref points at on GitHub; "" means the default branch.
/// Annotated tags resolve to the commit they tag.
pub fn resolve_remote(slug: &str, git_ref: &str) -> Result<String> {
    let git_ref = if git_ref.is_empty() { "HEAD" } else { git_ref };
    if git_ref.len() == 40 && SHA.is_match(git_ref) {
        return Ok(git_ref.into());
    }
    let name = git_ref.trim_start_matches("refs/tags/").trim_start_matches("refs/heads/");
    let (peeled, tag, head) = (format!("refs/tags/{name}^{{}}"), format!("refs/tags/{name}"), format!("refs/heads/{name}"));
    let out = Command::new("git")
        .args(["ls-remote", &format!("{}/{slug}", git_base()), "HEAD", &tag, &peeled, &head])
        // Never prompt for credentials, and give up on a stalled connection.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_HTTP_LOW_SPEED_LIMIT", "1000")
        .env("GIT_HTTP_LOW_SPEED_TIME", "5")
        .output()?;
    if !out.status.success() {
        bail!("git ls-remote {slug}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let refs: HashMap<&str, &str> = text.lines().filter_map(|l| l.split_once(char::is_whitespace)).map(|(sha, r)| (r.trim(), sha)).collect();
    for key in [&peeled, &tag, &head] {
        if let Some(sha) = refs.get(key.as_str()) {
            return Ok(sha.to_string());
        }
    }
    match refs.get("HEAD") {
        Some(sha) if name == "HEAD" => Ok(sha.to_string()),
        _ => Err(anyhow!("{slug} has no ref {name:?}")),
    }
}

/// The version in the plugin manifest at commit.
fn remote_version(p: &Plugin, commit: &str) -> Result<String> {
    let mut url = format!("{}/{}/{commit}", raw_base(), p.slug());
    if !p.source.subdir.is_empty() {
        url += &format!("/{}", p.source.subdir);
    }
    let body = ureq::get(&format!("{url}/herdr-plugin.toml")).timeout(Duration::from_secs(5)).call()?.into_string()?;
    #[derive(Deserialize)]
    struct Manifest {
        #[serde(default)]
        version: String,
    }
    Ok(toml::from_str::<Manifest>(&body)?.version)
}

/// Compares an installed GitHub plugin with its remote ref.
pub fn check(p: &Plugin) -> Status {
    if !p.github() {
        return Status::default();
    }
    // An exact-commit pin cannot drift, and ls-remote cannot resolve one.
    if p.pinned() {
        return Status { state: State::Current, remote: p.source.commit.clone(), ..Default::default() };
    }
    match resolve_remote(&p.slug(), p.git_ref()) {
        Err(e) => Status { state: State::Failed, error: e.to_string(), ..Default::default() },
        Ok(remote) if remote.eq_ignore_ascii_case(&p.source.commit) => Status { state: State::Current, remote, ..Default::default() },
        Ok(remote) => Status { state: State::Behind, version: remote_version(p, &remote).unwrap_or_default(), remote, ..Default::default() },
    }
}

/// Checks every GitHub plugin in parallel, keyed by plugin id.
pub fn check_all(list: &[Plugin]) -> HashMap<String, Status> {
    std::thread::scope(|s| {
        let handles: Vec<_> = list.iter().filter(|p| p.github()).map(|p| (p.id.clone(), s.spawn(|| check(p)))).collect();
        handles.into_iter().map(|(id, h)| (id, h.join().unwrap_or_default())).collect()
    })
}

/// Whether update checks can run.
pub fn have_git() -> bool {
    proc::look_path("git").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin(requested: &str, commit: &str) -> Plugin {
        let mut p = Plugin::default();
        p.source.requested_ref = requested.into();
        p.source.commit = commit.into();
        p
    }

    #[test]
    fn pins() {
        assert!(plugin("abc1234", "abc1234def").pinned());
        assert!(!plugin("main", "abc1234def").pinned());
        assert!(!plugin("abc1234", "fff").pinned());
    }

    #[test]
    fn full_shas_resolve_without_git() {
        let sha = "a".repeat(40);
        assert_eq!(resolve_remote("o/r", &sha).unwrap(), sha);
    }
}
