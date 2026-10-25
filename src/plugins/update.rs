//! Whether a GitHub plugin has moved on since it was installed.

use std::collections::HashMap;
use std::process::Command;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
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
    let wanted = candidates(git_ref);
    let mut cmd = Command::new("git");
    cmd.args(["ls-remote", &format!("{}/{slug}", git_base()), "HEAD"])
        .args(&wanted)
        // Never prompt for credentials, and give up on a stalled connection.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_HTTP_LOW_SPEED_LIMIT", "1000")
        .env("GIT_HTTP_LOW_SPEED_TIME", "5");
    let out = proc::output_within(&mut cmd, Duration::from_secs(20)).with_context(|| format!("git ls-remote {slug}"))?;
    if !out.status.success() {
        bail!("git ls-remote {slug}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let refs: HashMap<&str, &str> = text.lines().filter_map(|l| l.split_once(char::is_whitespace)).map(|(sha, r)| (r.trim(), sha)).collect();
    for key in &wanted {
        if let Some(sha) = refs.get(key.as_str()) {
            return Ok(sha.to_string());
        }
    }
    match refs.get("HEAD") {
        Some(sha) if git_ref == "HEAD" => Ok(sha.to_string()),
        _ => Err(anyhow!("{slug} has no ref {git_ref:?}")),
    }
}

/// The full refs git_ref can mean, best first: a peeled tag, the tag, then a
/// branch. A ref naming its namespace means only that one.
fn candidates(git_ref: &str) -> Vec<String> {
    if git_ref.starts_with("refs/heads/") {
        return vec![git_ref.into()];
    }
    let name = git_ref.strip_prefix("refs/tags/").unwrap_or(git_ref);
    let mut out = vec![format!("refs/tags/{name}^{{}}"), format!("refs/tags/{name}")];
    if name == git_ref {
        out.push(format!("refs/heads/{name}"));
    }
    out
}

/// The version in the plugin manifest at commit.
fn remote_version(p: &Plugin, commit: &str) -> Result<String> {
    let mut url = format!("{}/{}/{commit}", raw_base(), p.slug());
    if !p.source.subdir.is_empty() {
        url += &format!("/{}", p.source.subdir);
    }
    let body = ureq::get(&format!("{url}/herdr-plugin.toml")).config().timeout_global(Some(Duration::from_secs(5))).build().call()?.body_mut().read_to_string()?;
    #[derive(Deserialize)]
    struct Manifest {
        #[serde(default)]
        version: String,
    }
    Ok(toml::from_str::<Manifest>(&body)?.version)
}

/// The GitHub plugins in list, grouped by the repo and ref they follow so
/// each group needs one ls-remote.
pub fn check_groups(list: &[Plugin]) -> Vec<Vec<Plugin>> {
    let mut groups: Vec<Vec<Plugin>> = Vec::new();
    for p in list.iter().filter(|p| p.github()) {
        let same = |q: &Plugin| q.slug().eq_ignore_ascii_case(&p.slug()) && q.git_ref() == p.git_ref();
        match groups.iter_mut().find(|g| same(&g[0])) {
            Some(g) => g.push(p.clone()),
            None => groups.push(vec![p.clone()]),
        }
    }
    groups
}

/// Compares each plugin in a group from check_groups with their shared
/// remote ref, keyed by plugin id.
pub fn check_group(group: &[Plugin]) -> Vec<(String, Status)> {
    let mut remote = None;
    let mut check = |p: &Plugin| {
        // An exact-commit pin cannot drift, and ls-remote cannot resolve one.
        if p.pinned() {
            return Status { state: State::Current, remote: p.source.commit.clone(), ..Default::default() };
        }
        match remote.get_or_insert_with(|| resolve_remote(&p.slug(), p.git_ref()).map_err(|e| format!("{e:#}"))) {
            Err(e) => Status { state: State::Failed, error: e.clone(), ..Default::default() },
            Ok(r) if r.eq_ignore_ascii_case(&p.source.commit) => Status { state: State::Current, remote: r.clone(), ..Default::default() },
            Ok(r) => Status { state: State::Behind, version: remote_version(p, r).unwrap_or_default(), remote: r.clone(), ..Default::default() },
        }
    };
    group.iter().map(|p| (p.id.clone(), check(p))).collect()
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

    // A branch and a tag can share a name; an explicit namespace must pick one.
    #[test]
    fn explicit_namespaces_are_kept() {
        assert_eq!(candidates("refs/heads/fix/x"), ["refs/heads/fix/x"]);
        assert_eq!(candidates("refs/tags/v1"), ["refs/tags/v1^{}", "refs/tags/v1"]);
        assert_eq!(candidates("main"), ["refs/tags/main^{}", "refs/tags/main", "refs/heads/main"]);
    }

    // Two installs of one repo and ref must share a check, and no other.
    #[test]
    fn groups_share_a_repo_and_ref() {
        let gh = |id: &str, repo: &str, r: &str| {
            let mut p = plugin(r, "abc");
            (p.id, p.source.kind, p.source.owner, p.source.repo) = (id.into(), "github".into(), "o".into(), repo.into());
            p
        };
        let list = [gh("a", "r", "main"), gh("b", "R", "main"), gh("c", "r", "v1"), plugin("main", "abc")];
        let ids: Vec<Vec<String>> = check_groups(&list).iter().map(|g| g.iter().map(|p| p.id.clone()).collect()).collect();
        assert_eq!(ids, [vec!["a", "b"], vec!["c"]]);
    }

    #[test]
    fn full_shas_resolve_without_git() {
        let sha = "a".repeat(40);
        assert_eq!(resolve_remote("o/r", &sha).unwrap(), sha);
    }
}
