//! Running other programs with a deadline, and finding them on PATH.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use anyhow::Result;

/// Runs cmd like Command::output, but gives up once timeout passes, killing
/// it if it still runs. A descendant can hold the pipes open after cmd exits;
/// the output is then what was written by the time it exited.
pub fn output_within(cmd: &mut Command, timeout: Duration) -> Result<Output> {
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let read = |mut pipe: Option<Box<dyn Read + Send>>| {
        let shared = Arc::new(Mutex::new(Vec::new()));
        let (done, finished) = mpsc::channel();
        let into = Arc::clone(&shared);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            while let Some(n) = pipe.as_mut().and_then(|p| p.read(&mut chunk).ok()).filter(|n| *n > 0) {
                if let Ok(mut all) = into.lock() {
                    all.extend_from_slice(&chunk[..n]);
                }
            }
            let _ = done.send(());
        });
        (shared, finished)
    };
    let (out, out_done) = read(child.stdout.take().map(|p| Box::new(p) as _));
    let (err, err_done) = read(child.stderr.take().map(|p| Box::new(p) as _));
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(TimedOut(timeout).into());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let flushed = Instant::now() + Duration::from_millis(500);
    for done in [&out_done, &err_done] {
        let _ = done.recv_timeout(flushed.saturating_duration_since(Instant::now()));
    }
    let take = |buf: Arc<Mutex<Vec<u8>>>| buf.lock().map_or_else(|_| Vec::new(), |mut all| std::mem::take(&mut *all));
    Ok(Output { status, stdout: take(out), stderr: take(err) })
}

/// The error output_within gives when its timeout passes.
#[derive(Debug)]
pub struct TimedOut(pub Duration);

impl std::fmt::Display for TimedOut {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "gave up after {:?}", self.0)
    }
}

impl std::error::Error for TimedOut {}

/// Finds an executable on PATH. On Windows a bare name also matches each
/// PATHEXT extension, so `code` finds `code.cmd`.
pub fn look_path(name: &str) -> Option<PathBuf> {
    if name.contains('/') || (cfg!(windows) && name.contains('\\')) {
        return candidates(PathBuf::from(name)).into_iter().find(|p| is_executable(p));
    }
    std::env::split_paths(&std::env::var_os("PATH")?).flat_map(|dir| candidates(if dir.as_os_str().is_empty() { PathBuf::from(".") } else { dir }.join(name))).find(|p| is_executable(p))
}

#[cfg(unix)]
fn candidates(base: PathBuf) -> Vec<PathBuf> {
    vec![base]
}

/// A name without an extension stands for each PATHEXT extension instead.
#[cfg(windows)]
fn candidates(base: PathBuf) -> Vec<PathBuf> {
    if base.extension().is_some() {
        return vec![base];
    }
    let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    exts.split(';').filter(|e| !e.is_empty()).map(|e| base.with_extension(e.trim_start_matches('.'))).collect()
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(windows)]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    // A program that never answers must not hold up its caller.
    #[test]
    fn output_within_kills_a_hung_program() {
        let mut cmd = if cfg!(windows) { Command::new("ping") } else { Command::new("sleep") };
        if cfg!(windows) {
            cmd.args(["-n", "30", "127.0.0.1"]);
        } else {
            cmd.arg("30");
        }
        let start = Instant::now();
        let err = output_within(&mut cmd, Duration::from_millis(200)).unwrap_err();
        assert!(err.is::<TimedOut>(), "{err:#}");
        assert!(start.elapsed() < Duration::from_secs(5));
        let mut cmd = if cfg!(windows) { Command::new("cmd") } else { Command::new("sh") };
        cmd.args(if cfg!(windows) { ["/C", "echo hi"] } else { ["-c", "echo hi"] });
        let out = output_within(&mut cmd, Duration::from_secs(10)).unwrap();
        assert!(out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "hi");
    }

    // A descendant that keeps the output pipe open after its parent exits
    // must not hold up the caller, or turn a program that succeeded into a failure.
    #[test]
    fn output_within_stops_waiting_on_inherited_pipes() {
        let mut cmd = if cfg!(windows) { Command::new("cmd") } else { Command::new("sh") };
        cmd.args(if cfg!(windows) { ["/C", "echo hi & start /b ping -n 4 127.0.0.1"] } else { ["-c", "echo hi; sleep 3 &"] });
        let start = Instant::now();
        let out = output_within(&mut cmd, Duration::from_secs(10)).unwrap();
        assert!(out.status.success() && String::from_utf8_lossy(&out.stdout).trim_start().starts_with("hi"));
        assert!(start.elapsed() < Duration::from_millis(2000), "{:?}", start.elapsed());
    }
}
