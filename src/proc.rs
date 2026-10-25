//! Helper processes that outlive the popup. Herdr tears down a popup's whole
//! process group (or job object on Windows) when it closes, so a helper runs
//! in its own session.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

/// Starts path in a new session with no standard streams, without waiting.
pub fn start_detached(path: &Path, args: &[&str]) -> Result<()> {
    let command = || {
        let mut cmd = Command::new(path);
        cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        cmd
    };
    detach(command).with_context(|| format!("start {}", path.display()))
}

#[cfg(unix)]
fn detach(command: impl Fn() -> Command) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    let mut cmd = command();
    // SAFETY: setsid is async-signal-safe and runs in the child before exec,
    // so the helper has left the popup's process group when spawn returns.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn().map(drop)
}

#[cfg(windows)]
fn detach(command: impl Fn() -> Command) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::{CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, DETACHED_PROCESS};
    let flags = CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS | CREATE_NO_WINDOW;
    // Leave Herdr's job object when it allows that, so closing the popup does
    // not end the helper; otherwise start inside it.
    if command().creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB).spawn().is_ok() {
        return Ok(());
    }
    command().creation_flags(flags).spawn().map(drop)
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

#[cfg(unix)]
fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    let rc = unsafe { libc::kill(pid, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn alive(pid: i32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
    // SAFETY: the handle is checked before use and closed exactly once.
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid as u32);
        if handle.is_null() {
            return false;
        }
        let running = WaitForSingleObject(handle, 0) == WAIT_TIMEOUT;
        CloseHandle(handle);
        running
    }
}

/// Opens url in the default browser without a shell.
pub fn open_url(url: &str) -> Result<()> {
    let mut cmd = if cfg!(target_os = "macos") {
        Command::new("open")
    } else if cfg!(windows) {
        let mut cmd = Command::new("rundll32");
        cmd.arg("url.dll,FileProtocolHandler");
        cmd
    } else {
        Command::new("xdg-open")
    };
    cmd.arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().with_context(|| format!("open {url}"))?;
    Ok(())
}

/// Finds an executable on PATH. On Windows a bare name also matches each
/// PATHEXT extension, so `code` finds `code.cmd`.
pub fn look_path(name: &str) -> Option<PathBuf> {
    let candidates = |base: PathBuf| -> Vec<PathBuf> {
        let mut out = vec![base.clone()];
        if cfg!(windows) && base.extension().is_none() {
            let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
            out = exts.split(';').filter(|e| !e.is_empty()).map(|e| base.with_extension(e.trim_start_matches('.'))).collect();
        }
        out
    };
    if name.contains('/') || (cfg!(windows) && name.contains('\\')) {
        return candidates(PathBuf::from(name)).into_iter().find(|p| is_executable(p));
    }
    std::env::split_paths(&std::env::var_os("PATH")?).flat_map(|dir| candidates(if dir.as_os_str().is_empty() { PathBuf::from(".") } else { dir }.join(name))).find(|p| is_executable(p))
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

#[cfg(windows)]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}
