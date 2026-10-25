//! Unix: processes come from ps and colors are written to the client's TTY.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
use std::path::Path;
use std::process::Command;

use anyhow::{Result, anyhow, bail};

use super::{Client, MAX_PAYLOAD, Unavailable, classify, host_payload, is_herdr};

/// Every process on this machine.
pub fn processes() -> Result<HashMap<i32, Client>> {
    list_processes(&["-e"])
}

fn list_processes(selector: &[&str]) -> Result<HashMap<i32, Client>> {
    let out = Command::new("ps").args(selector).args(["-o", "pid,tty,comm"]).output().map_err(|e| anyhow!("locate terminal client: {e}"))?;
    if !out.status.success() {
        bail!("locate terminal client: ps {}", out.status);
    }
    let mut processes = HashMap::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 {
            continue;
        }
        let Ok(pid) = fields[0].parse::<i32>() else { continue };
        let tty = match fields[1] {
            "?" | "??" | "-" => "",
            t => t,
        };
        let mut p = Client { pid, tty: tty.into(), name: fields[2..].join(" "), ..Default::default() };
        // Remote bridges have no TTY, but their arguments still tell whether
        // every attached client lives on another machine.
        if is_herdr(&p.name)
            && let Ok(args) = process_args(pid)
        {
            classify(&mut p, &args);
        }
        processes.insert(pid, p);
    }
    Ok(processes)
}

/// Writes payload to client's terminal after checking it is still there.
pub fn emit_client(client: &Client, payload: &str) -> Result<()> {
    if client.tty.is_empty() {
        return Err(Unavailable.into());
    }
    // Revalidate immediately before opening the TTY; discovery may have
    // raced with a detach or with another process taking that terminal.
    let processes = list_processes(&["-p", &client.pid.to_string()]).map_err(|_| Unavailable)?;
    match processes.get(&client.pid) {
        Some(p) if p.is_interactive() && p.tty == client.tty => {}
        _ => return Err(Unavailable.into()),
    }
    if client.tty.contains("..") {
        bail!("invalid client terminal path");
    }
    let path = Path::new("/dev").join(&client.tty);
    if !std::fs::metadata(&path)?.file_type().is_char_device() {
        bail!("client terminal is not a character device");
    }
    let mut f = match OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(&path) {
        Ok(f) => f,
        // Another account's client: not ours to recolor.
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Err(Unavailable.into()),
        Err(e) => return Err(e.into()),
    };
    f.write_all(host_payload(payload).as_bytes())?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn process_args(pid: i32) -> Result<Vec<String>> {
    let data = std::fs::read(format!("/proc/{pid}/cmdline"))?;
    if data.len() > MAX_PAYLOAD {
        bail!("process arguments are too large");
    }
    let data = data.strip_suffix(&[0]).unwrap_or(&data);
    Ok(data.split(|b| *b == 0).map(|a| String::from_utf8_lossy(a).into_owned()).collect())
}

#[cfg(target_os = "macos")]
fn process_args(pid: i32) -> Result<Vec<String>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut size: libc::size_t = 0;
    // SAFETY: the first call only reports the size; the second fills a
    // buffer of exactly that size.
    unsafe {
        if libc::sysctl(mib.as_mut_ptr(), 3, std::ptr::null_mut(), &mut size, std::ptr::null_mut(), 0) != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if size > MAX_PAYLOAD {
            bail!("process arguments are too large");
        }
        let mut data = vec![0u8; size];
        if libc::sysctl(mib.as_mut_ptr(), 3, data.as_mut_ptr().cast(), &mut size, std::ptr::null_mut(), 0) != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        data.truncate(size);
        darwin_args(&data)
    }
}

/// kern.procargs2 holds argc, the executable path, alignment padding, then
/// NUL-separated argv and environment strings. Only argv identifies the
/// launch mode; spaces in a session name or executable path stay intact.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn darwin_args(data: &[u8]) -> Result<Vec<String>> {
    if data.len() < 4 {
        bail!("missing process argument count");
    }
    let argc = u32::from_ne_bytes(data[..4].try_into().unwrap()) as usize;
    let mut rest = &data[4..];
    let end = rest.iter().position(|b| *b == 0);
    let Some(end) = end.filter(|_| argc > 0 && argc <= rest.len()) else { bail!("invalid process arguments") };
    rest = &rest[end + 1..];
    let start = rest.iter().position(|b| *b != 0).unwrap_or(rest.len());
    rest = &rest[start..];
    let mut args = Vec::with_capacity(argc);
    for _ in 0..argc {
        let end = rest.iter().position(|b| *b == 0).ok_or_else(|| anyhow!("truncated process arguments"))?;
        args.push(String::from_utf8_lossy(&rest[..end]).into_owned());
        rest = &rest[end + 1..];
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn darwin_args_skip_exe_and_padding() {
        let mut data = 2u32.to_ne_bytes().to_vec();
        data.extend_from_slice(b"/bin/herdr\0\0\0herdr\0--session\0HOME=/x\0");
        assert_eq!(darwin_args(&data).unwrap(), ["herdr", "--session"]);
    }
}
