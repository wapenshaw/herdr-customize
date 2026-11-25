//! Whether a newer herdr-customize is out: GitHub's latest release, asked at
//! most once a day. Updating stays a step the person takes (docs/install.md).

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{market, read_record, write_record};

const RECORD: &str = "latest.json";
const REPO: &str = "wapenshaw/herdr-customize";
const DAY: u64 = 24 * 60 * 60;

/// The last answer, kept even when a later ask fails.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Seen {
    checked: u64,
    latest: String,
}

/// The running version, and the newer one with the command that updates this
/// checkout when there is one. refresh asks GitHub even within the day.
pub fn describe(refresh: bool) -> Value {
    let current = env!("CARGO_PKG_VERSION");
    let mut seen: Seen = read_record(RECORD).unwrap_or_default();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    if refresh || now.saturating_sub(seen.checked) >= DAY {
        // A failed ask counts too, so an offline machine isn't asked on every open.
        seen.checked = now;
        if let Ok(latest) = fetch() {
            seen.latest = latest;
        }
        let _ = write_record(RECORD, &seen);
    }
    let latest = newer(&seen.latest, current).then_some(seen.latest);
    let command = latest.as_ref().and_then(|_| std::env::current_exe().ok()).and_then(|exe| update_command(&exe));
    json!({"version": current, "latest": latest, "command": command})
}

fn fetch() -> Result<String> {
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
    }
    let mut resp = ureq::get(&format!("{}/repos/{REPO}/releases/latest", market::api_base()))
        .header("Accept", "application/vnd.github+json")
        .config()
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .call()?;
    let release: Release = serde_json::from_str(&resp.body_mut().read_to_string()?)?;
    if version(&release.tag_name).is_none() {
        bail!("unexpected release tag {:?}", release.tag_name);
    }
    Ok(release.tag_name.trim_start_matches('v').to_string())
}

/// x.y.z, with or without a leading v; anything else (a pre-release) is none.
fn version(text: &str) -> Option<[u64; 3]> {
    let mut parts = text.trim_start_matches('v').split('.').map(|p| p.parse().ok());
    let v = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(v)
}

fn newer(latest: &str, current: &str) -> bool {
    version(latest).zip(version(current)).is_some_and(|(l, c)| l > c)
}

/// The update steps for a binary in a checkout's target/release, as one line
/// for the platform's shell; none for a binary that came another way.
fn update_command(exe: &Path) -> Option<String> {
    let root = exe.parent()?.parent()?.parent()?;
    if !root.join(".git").exists() || !root.join("scripts").join("fetch-or-build.sh").is_file() {
        return None;
    }
    let root = root.to_string_lossy();
    Some(if cfg!(windows) {
        format!("cd \"{root}\"; git pull --ff-only; powershell -NoProfile -ExecutionPolicy Bypass -File scripts\\fetch-or-build.ps1; target\\release\\herdr-customize.exe app install")
    } else {
        let root = root.replace('\'', "'\\''");
        format!("cd '{root}' && git pull --ff-only && sh scripts/fetch-or-build.sh && target/release/herdr-customize app install")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn only_a_higher_release_is_newer() {
        assert!(newer("1.2.0", "1.1.9") && newer("v2.0.0", "1.10.3") && newer("1.10.0", "1.9.0"));
        assert!(!newer("1.1.1", "1.1.1") && !newer("1.0.9", "1.1.0"));
        assert!(!newer("1.2.0-rc.1", "1.1.0") && !newer("", "1.1.0") && !newer("1.2", "1.1.0"));
    }

    // A newer release is told within the day without asking again, and a
    // failed ask keeps the last answer rather than hiding it.
    #[test]
    fn the_last_answer_holds_while_github_is_unreachable() {
        let _guard = crate::testenv::isolated();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        write_record(RECORD, &Seen { checked: now, latest: "99.0.0".into() }).unwrap();
        assert_eq!(describe(false)["latest"], "99.0.0");
        write_record(RECORD, &Seen { checked: 0, latest: "99.0.0".into() }).unwrap();
        assert_eq!(describe(false)["latest"], "99.0.0");
        let seen: Seen = read_record(RECORD).unwrap();
        assert!(seen.checked >= now, "a failed ask waits a day too");
        write_record(RECORD, &Seen { checked: now, latest: env!("CARGO_PKG_VERSION").into() }).unwrap();
        assert_eq!(describe(false)["latest"], Value::Null);
        fs::remove_file(super::super::state_dir().join(RECORD)).unwrap();
    }

    #[test]
    fn the_command_updates_only_a_checkout() {
        let dir = std::env::temp_dir().join(format!("hc-latest-{}", std::process::id()));
        let exe = dir.join("target/release/herdr-customize");
        fs::create_dir_all(dir.join("scripts")).unwrap();
        assert_eq!(update_command(&exe), None);
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(dir.join("scripts/fetch-or-build.sh"), "").unwrap();
        let command = update_command(&exe).unwrap();
        assert!(command.contains(&*dir.to_string_lossy()) && command.contains("git pull --ff-only") && command.ends_with("app install"));
        fs::remove_dir_all(dir).unwrap();
    }
}
