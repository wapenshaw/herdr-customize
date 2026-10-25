//! Helper processes that outlive the popup. Herdr tears down a popup's whole
//! process group when it closes, so a helper runs in its own session.

use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

/// Starts path in a new session with no standard streams, without waiting.
pub fn start_detached(path: &std::path::Path, args: &[&str]) -> Result<()> {
    let mut cmd = Command::new(path);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // SAFETY: setsid is async-signal-safe and runs in the child before exec,
    // so the helper has left the popup's process group when spawn returns.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn().with_context(|| format!("start {}", path.display()))?;
    Ok(())
}

/// Waits until pid has exited or timeout passes, and reports whether it exited.
pub fn wait_exit(pid: i32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while alive(pid) {
        if Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    true
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    let rc = unsafe { libc::kill(pid, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Opens url in the default browser without a shell.
pub fn open_url(url: &str) -> Result<()> {
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    Command::new(opener).arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().with_context(|| format!("open {url}"))?;
    Ok(())
}

/// Finds an executable on PATH.
pub fn look_path(name: &str) -> Option<std::path::PathBuf> {
    if name.contains('/') {
        let p = std::path::PathBuf::from(name);
        return is_executable(&p).then_some(p);
    }
    std::env::var_os("PATH")?.to_str()?.split(':').map(|dir| std::path::Path::new(if dir.is_empty() { "." } else { dir }).join(name)).find(|p| is_executable(p))
}

fn is_executable(p: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}
