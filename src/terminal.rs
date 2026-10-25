//! Finds the terminals of this machine's Herdr clients and recolors them.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
use std::path::Path;
use std::process::Command;

use anyhow::{Result, anyhow, bail};

/// The owning host terminal is gone, or not ours to write.
#[derive(Debug)]
pub struct Unavailable;

impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("owning host terminal unavailable")
    }
}

impl std::error::Error for Unavailable {}

pub fn is_unavailable(e: &anyhow::Error) -> bool {
    e.downcast_ref::<Unavailable>().is_some()
}

/// A process and its terminal. The PID keeps a reattach on the same TTY
/// distinct from the previous client.
#[derive(Debug, Clone, Default)]
pub struct Client {
    pub pid: i32,
    pub name: String,
    pub tty: String,
    /// A UI client launch, local or --remote.
    pub interactive: bool,
    /// A UI client rendering a server on another machine.
    pub remote: bool,
    /// This machine's endpoint for another machine's --remote client.
    pub bridge: bool,
}

impl Client {
    fn is_interactive(&self) -> bool {
        is_herdr(&self.name) && !self.tty.is_empty() && self.interactive
    }
}

pub fn is_herdr(name: &str) -> bool {
    name.rsplit('/').next() == Some("herdr")
}

/// Recognizes client launches, including the executable in args[0]. CLI
/// commands such as server stop, status, and plugin actions do not consume
/// host color replies and must never receive queries. Unknown arguments are
/// rejected rather than guessed from the binary name.
pub fn is_interactive_command(args: &[String]) -> bool {
    let Some((exe, mut args)) = args.split_first() else { return false };
    if !is_herdr(exe) {
        return false;
    }
    // Herdr rewrites this documented alias internally, without replacing argv.
    if args.len() == 3 && args[0] == "session" && args[1] == "attach" {
        let name = &args[2];
        return !name.is_empty() && name != "help" && !name.starts_with('-');
    }
    if args.first().map(String::as_str) == Some("client") {
        args = &args[1..];
    }
    let mut i = 0;
    while i < args.len() {
        let (flag, inline) = match args[i].split_once('=') {
            Some((f, v)) => (f, Some(v)),
            None => (args[i].as_str(), None),
        };
        match flag {
            "--handoff" if inline.is_some() => return false,
            "--handoff" => {}
            "--session" | "--remote" | "--remote-keybindings" => {
                let value = match inline {
                    Some(v) => v,
                    None => {
                        i += 1;
                        match args.get(i) {
                            Some(v) => v.as_str(),
                            None => return false,
                        }
                    }
                };
                if value.is_empty() || value.starts_with('-') {
                    return false;
                }
                if flag == "--remote-keybindings" && value != "local" && value != "server" {
                    return false;
                }
            }
            _ => return false,
        }
        i += 1;
    }
    true
}

fn has_remote_flag(args: &[String]) -> bool {
    args.iter().skip(1).any(|a| a == "--remote" || a.starts_with("--remote="))
}

/// The process sshd starts on the server for a client that ran herdr
/// --remote elsewhere. It has no local terminal.
pub fn is_remote_bridge(args: &[String]) -> bool {
    args.len() > 1 && is_herdr(&args[0]) && args[1] == "remote-client-bridge"
}

fn classify(p: &mut Client, args: &[String]) {
    p.interactive = is_interactive_command(args);
    p.remote = p.interactive && has_remote_flag(args);
    p.bridge = is_remote_bridge(args);
}

/// Every client attached here came from another machine, so whoever opened a
/// plugin popup cannot see this machine's theme. With local clients present
/// the invoker is unknown and false is returned.
pub fn only_remote(processes: &HashMap<i32, Client>) -> bool {
    let mut bridges = 0;
    for p in processes.values() {
        if p.is_interactive() && !p.remote {
            return false;
        }
        if p.bridge {
            bridges += 1;
        }
    }
    bridges > 0
}

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

/// The explicitly requested client. Never falls back to another process.
fn select_client(processes: &HashMap<i32, Client>, explicit: &str) -> Result<Client> {
    let pid: i32 = explicit.parse().ok().filter(|p| *p > 0).ok_or_else(|| anyhow!("invalid HERDR_CUSTOMIZE_CLIENT_PID {explicit:?}"))?;
    match processes.get(&pid) {
        Some(p) if p.is_interactive() => Ok(p.clone()),
        _ => bail!("PID {pid} is not a Herdr client"),
    }
}

/// The interactive Herdr clients on this machine, or only the explicit
/// target. Every one reads this machine's config, so they share the theme.
pub fn active_clients(explicit: &str) -> Result<Vec<Client>> {
    let processes = processes()?;
    if !explicit.is_empty() {
        return Ok(vec![select_client(&processes, explicit)?]);
    }
    let clients: Vec<Client> = processes.into_values().filter(Client::is_interactive).collect();
    if clients.is_empty() {
        return Err(Unavailable.into());
    }
    Ok(clients)
}

/// Sets colors, then requests their current values. Herdr consumes the OSC
/// replies as host-theme updates and passes them to its terminals; without
/// the readback a reattached client can keep the colors it queried before.
pub fn host_payload(colors: &str) -> String {
    if colors.is_empty() {
        return String::new();
    }
    let mut out = format!("{colors}\x1b]10;?\x1b\\\x1b]11;?\x1b\\");
    for i in 0..16 {
        out.push_str(&format!("\x1b]4;{i};?\x1b\\"));
    }
    out
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

    fn args(s: &str) -> Vec<String> {
        s.split(' ').map(String::from).collect()
    }

    #[test]
    fn interactive_commands() {
        for ok in ["herdr", "/usr/bin/herdr --session work", "herdr client --remote=host", "herdr session attach work", "herdr --remote-keybindings local --remote h"] {
            assert!(is_interactive_command(&args(ok)), "{ok}");
        }
        for no in ["herdr server stop", "herdr plugin list", "herdr --session", "herdr --remote-keybindings odd", "herdr session attach --x", "vim"] {
            assert!(!is_interactive_command(&args(no)), "{no}");
        }
    }

    #[test]
    fn only_remote_needs_a_bridge_and_no_local_client() {
        let client = |pid: i32, tty: &str, remote: bool, bridge: bool| Client { pid, name: "herdr".into(), tty: tty.into(), interactive: !bridge, remote, bridge };
        let mut p = HashMap::new();
        p.insert(1, client(1, "", false, true));
        assert!(only_remote(&p));
        p.insert(2, client(2, "ttys001", true, false));
        assert!(only_remote(&p));
        p.insert(3, client(3, "ttys002", false, false));
        assert!(!only_remote(&p));
    }

    #[test]
    fn darwin_args_skip_exe_and_padding() {
        let mut data = 2u32.to_ne_bytes().to_vec();
        data.extend_from_slice(b"/bin/herdr\0\0\0herdr\0--session\0HOME=/x\0");
        assert_eq!(darwin_args(&data).unwrap(), ["herdr", "--session"]);
    }

    #[test]
    fn payload_reads_colors_back() {
        let p = host_payload("\x1b]11;#000000\x07");
        assert!(p.starts_with("\x1b]11;#000000\x07\x1b]10;?"));
        assert!(p.ends_with("\x1b]4;15;?\x1b\\"));
        assert_eq!(host_payload(""), "");
    }
}
