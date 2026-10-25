//! Finds the terminals of this machine's Herdr clients and recolors them.

use std::collections::HashMap;

use anyhow::{Result, anyhow, bail};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{emit_client, processes};
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{emit_client, processes, run_helper};

/// Largest payload or command line read from another process.
const MAX_PAYLOAD: usize = 1 << 20;

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
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    base == "herdr" || base.eq_ignore_ascii_case("herdr.exe")
}

/// Recognizes client launches, args[0] included. CLI commands (server stop,
/// status, plugin actions) never read color replies, so must never be queried;
/// unknown arguments are rejected rather than guessed from the binary name.
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

/// Splits a Windows command line as the C runtime builds argv: the program
/// ends at the first unquoted space; before a quote, 2n backslashes give n and
/// toggle quoting, 2n+1 give n and a literal quote.
#[cfg_attr(not(windows), allow(dead_code, reason = "tested on every OS, used on Windows"))]
fn split_command_line(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut args = Vec::new();
    let mut i = 0;
    let mut program = String::new();
    let mut quoted = false;
    while i < chars.len() && (quoted || !matches!(chars[i], ' ' | '\t')) {
        if chars[i] == '"' {
            quoted = !quoted;
        } else {
            program.push(chars[i]);
        }
        i += 1;
    }
    args.push(program);
    loop {
        while i < chars.len() && matches!(chars[i], ' ' | '\t') {
            i += 1;
        }
        if i >= chars.len() {
            return args;
        }
        let mut arg = String::new();
        let mut quoted = false;
        while i < chars.len() && (quoted || !matches!(chars[i], ' ' | '\t')) {
            let slashes = chars[i..].iter().take_while(|c| **c == '\\').count();
            i += slashes;
            if chars.get(i) == Some(&'"') {
                arg.extend(std::iter::repeat_n('\\', slashes / 2));
                if slashes % 2 == 1 {
                    arg.push('"');
                } else if quoted && chars.get(i + 1) == Some(&'"') {
                    arg.push('"');
                    i += 1;
                } else {
                    quoted = !quoted;
                }
                i += 1;
            } else if slashes > 0 {
                arg.extend(std::iter::repeat_n('\\', slashes));
            } else {
                arg.push(chars[i]);
                i += 1;
            }
        }
        args.push(arg);
    }
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
        assert!(is_interactive_command(&args(r"C:\Tools\HERDR.EXE --session w")));
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
    fn windows_command_lines() {
        assert_eq!(split_command_line(r#""C:\Program Files\herdr\herdr.exe" --session "my work""#), [r"C:\Program Files\herdr\herdr.exe", "--session", "my work"]);
        assert_eq!(split_command_line(r#"herdr a\\"b c" d\"e f\\g "x""y""#), ["herdr", r"a\b c", "d\"e", r"f\\g", "x\"y"]);
        assert_eq!(split_command_line("herdr"), ["herdr"]);
    }

    #[test]
    fn payload_reads_colors_back() {
        let p = host_payload("\x1b]11;#000000\x07");
        assert!(p.starts_with("\x1b]11;#000000\x07\x1b]10;?"));
        assert!(p.ends_with("\x1b]4;15;?\x1b\\"));
        assert_eq!(host_payload(""), "");
    }
}
