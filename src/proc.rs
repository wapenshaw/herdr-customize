//! Starting other programs: helpers that outlive the popup, the browser and
//! the user's editor. Herdr tears down a popup's whole process group (or job
//! object on Windows) when it closes, so a helper runs in its own session.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};

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
    std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

#[cfg(windows)]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

/// VISUAL or EDITOR, else an installed fallback.
fn pick_editor() -> String {
    for key in ["VISUAL", "EDITOR"] {
        if let Some(v) = crate::config::env(key) {
            return v;
        }
    }
    let fallback = if cfg!(windows) { "notepad.exe" } else { "vi" };
    ["nvim", "nano"].iter().find(|n| look_path(n).is_some()).map_or(fallback, |n| n).to_string()
}

/// Splits an editor command without evaluating shell syntax.
fn split_editor(command: &str) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '\\'
            && let Some(n) = next
            && ((quote == Some('"') && n == '"') || (quote.is_none() && n.is_whitespace()))
        {
            word.push(n);
            started = true;
            i += 2;
            continue;
        }
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => word.push(c),
            None if c == '\'' || c == '"' => quote = Some(c),
            None if c.is_whitespace() => {
                if started {
                    args.push(std::mem::take(&mut word));
                    started = false;
                }
                i += 1;
                continue;
            }
            None => word.push(c),
        }
        started = true;
        i += 1;
    }
    if quote.is_some() {
        bail!("unclosed quote in editor command");
    }
    if started {
        args.push(word);
    }
    if args.first().is_none_or(String::is_empty) {
        bail!("editor command is empty");
    }
    Ok(args)
}

/// A command that opens path in the preferred editor.
pub fn editor_command(path: &Path) -> Result<Command> {
    let editor = pick_editor();
    // A preference may be an unquoted executable path containing spaces.
    let args = match look_path(&editor) {
        Some(resolved) => vec![resolved.to_string_lossy().into_owned()],
        None => split_editor(&editor)?,
    };
    let exe = look_path(&args[0]).ok_or_else(|| anyhow!("editor {:?} not found", args[0]))?;
    let is_script = exe.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));
    if cfg!(windows) && is_script {
        // Editors such as VS Code install a .cmd shim rather than an .exe.
        // PowerShell's literal quoting keeps cmd.exe from expanding paths.
        let quote = |s: &str| format!("'{}'", s.replace('\'', "''"));
        let mut line = format!("& {}", quote(&exe.to_string_lossy()));
        for arg in args[1..].iter().map(String::as_str).chain([path.to_string_lossy().as_ref()]) {
            line.push(' ');
            line.push_str(&quote(arg));
        }
        let mut cmd = Command::new(powershell());
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &line]);
        return Ok(cmd);
    }
    let mut cmd = Command::new(exe);
    cmd.args(&args[1..]).arg(path);
    Ok(cmd)
}

/// PowerShell 7 when installed, else Windows PowerShell.
pub fn powershell() -> &'static str {
    if look_path("pwsh.exe").is_some() { "pwsh.exe" } else { "powershell.exe" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_splitting() {
        assert_eq!(split_editor("code --wait").unwrap(), ["code", "--wait"]);
        assert_eq!(split_editor("'/Applications/My Editor' -n").unwrap(), ["/Applications/My Editor", "-n"]);
        assert_eq!(split_editor(r"my\ editor").unwrap(), ["my editor"]);
        assert_eq!(split_editor(r#"ed "a \"b\"""#).unwrap(), ["ed", r#"a "b""#]);
        assert!(split_editor("'open").is_err());
        assert!(split_editor("   ").is_err());
    }
}
