//! Windows: processes come from a Toolhelp snapshot, and colors are written
//! by a short-lived helper that attaches to exactly one client's console.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessCommandLineInformation};
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_INVALID_HANDLE, ERROR_INVALID_PARAMETER, ERROR_NO_MORE_FILES, HANDLE, INVALID_HANDLE_VALUE, UNICODE_STRING};
use windows_sys::Win32::System::Console::{AttachConsole, ENABLE_VIRTUAL_TERMINAL_PROCESSING, FreeConsole, GetConsoleMode, SetConsoleMode};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS};
use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

use super::{Client, MAX_PAYLOAD, Unavailable, classify, host_payload, is_herdr, split_command_line};

/// The helper's exit code for a client that is gone or has no console.
const UNAVAILABLE_EXIT: i32 = 3;

/// Closes a handle when dropped.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful open and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// Every process on this machine.
pub fn processes() -> Result<HashMap<i32, Client>> {
    // SAFETY: the snapshot handle is checked and owned; entry is sized as the
    // API requires before the first call.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error().into());
        }
        let _snapshot = Owned(snapshot);
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snapshot, &mut entry) == 0 {
            return Err(io::Error::last_os_error().into());
        }
        let mut processes = HashMap::new();
        loop {
            let pid = entry.th32ProcessID as i32;
            let len = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
            let mut p = Client { pid, name: String::from_utf16_lossy(&entry.szExeFile[..len]), tty: "console".into(), ..Default::default() };
            if is_herdr(&p.name)
                && let Ok(args) = process_args(pid)
            {
                classify(&mut p, &args);
            }
            processes.insert(pid, p);
            if Process32NextW(snapshot, &mut entry) == 0 {
                let err = io::Error::last_os_error();
                if err.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                    break;
                }
                return Err(err.into());
            }
        }
        Ok(processes)
    }
}

/// Reads a process's command line from the OS rather than guessing its role
/// from the executable name.
fn process_args(pid: i32) -> Result<Vec<String>> {
    // SAFETY: the buffer is 8-byte aligned and as large as the kernel asked
    // for; the string it describes is bounds-checked against the buffer
    // before it is read.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32);
        if handle.is_null() {
            return Err(io::Error::last_os_error().into());
        }
        let _handle = Owned(handle);
        let mut size = 0u32;
        NtQueryInformationProcess(handle, ProcessCommandLineInformation, std::ptr::null_mut(), 0, &mut size);
        if (size as usize) < size_of::<UNICODE_STRING>() || size as usize > MAX_PAYLOAD {
            bail!("invalid process command line size");
        }
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        let status = NtQueryInformationProcess(handle, ProcessCommandLineInformation, buffer.as_mut_ptr().cast(), size, &mut size);
        if status < 0 {
            bail!("query process command line: NTSTATUS {status:#x}");
        }
        let text = &*(buffer.as_ptr() as *const UNICODE_STRING);
        let total = buffer.len() * 8;
        let offset = (text.Buffer as usize).wrapping_sub(buffer.as_ptr() as usize);
        if !text.Length.is_multiple_of(2) || offset > total || text.Length as usize > total - offset {
            bail!("invalid process command line buffer");
        }
        let wide = std::slice::from_raw_parts(text.Buffer, text.Length as usize / 2);
        Ok(split_command_line(&String::from_utf16_lossy(wide)))
    }
}

/// Writes payload to client's console through the terminal-sync helper. A
/// process can attach to only one console, and the popup already has its
/// own, so the write happens in a child that attaches to the client's.
pub fn emit_client(client: &Client, payload: &str) -> Result<()> {
    let exe = std::env::current_exe()?;
    let mut child = Command::new(exe).args(["terminal-sync", &client.pid.to_string()]).creation_flags(CREATE_NO_WINDOW).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(host_payload(payload).as_bytes())?;
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            bail!("sync terminal: helper timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    match status.code() {
        Some(0) => Ok(()),
        Some(UNAVAILABLE_EXIT) => Err(Unavailable.into()),
        _ => {
            let mut stderr = String::new();
            if let Some(mut err) = child.stderr.take() {
                let _ = err.read_to_string(&mut stderr);
            }
            bail!("sync terminal: {status}: {}", stderr.trim())
        }
    }
}

/// The terminal-sync helper: attaches to exactly the requested client's
/// console and writes stdin to it. It never falls back to another console.
/// Returns the process exit code.
pub fn run_helper(pid_text: &str) -> i32 {
    match helper(pid_text) {
        Ok(()) => 0,
        Err(e) if super::is_unavailable(&e) => UNAVAILABLE_EXIT,
        Err(e) => {
            eprintln!("{e:#}");
            1
        }
    }
}

fn helper(pid_text: &str) -> Result<()> {
    let pid: u32 = pid_text.parse().ok().filter(|p| *p > 0).ok_or_else(|| anyhow!("invalid client PID {pid_text:?}"))?;
    let mut data = Vec::new();
    io::stdin().take(MAX_PAYLOAD as u64 + 1).read_to_end(&mut data)?;
    if data.len() > MAX_PAYLOAD {
        bail!("terminal payload is too large");
    }
    let processes = processes()?;
    // SAFETY: console attach/detach calls take no pointers; the CONOUT$
    // handle stays owned by the File for the duration of the mode calls.
    unsafe {
        if FreeConsole() == 0 {
            bail!("detach helper console: {}", io::Error::last_os_error());
        }
        if !processes.get(&(pid as i32)).is_some_and(Client::is_interactive) {
            return Err(Unavailable.into());
        }
        if AttachConsole(pid) == 0 {
            let err = io::Error::last_os_error();
            if matches!(err.raw_os_error(), Some(c) if c == ERROR_INVALID_HANDLE as i32 || c == ERROR_INVALID_PARAMETER as i32) {
                return Err(Unavailable.into());
            }
            bail!("attach client console: {err}");
        }
        let result = (|| -> Result<()> {
            let mut out = OpenOptions::new().read(true).write(true).open("CONOUT$")?;
            let handle = out.as_raw_handle() as HANDLE;
            let mut mode = 0;
            if GetConsoleMode(handle, &mut mode) == 0 {
                return Err(io::Error::last_os_error().into());
            }
            if SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) == 0 {
                return Err(io::Error::last_os_error().into());
            }
            let written = out.write_all(&data);
            SetConsoleMode(handle, mode);
            Ok(written?)
        })();
        FreeConsole();
        result
    }
}
