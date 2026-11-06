//! herdr-core's config.toml. The core reads it each time it needs a setting
//! and treats a broken file as no file, so every value is checked here as the
//! core checks it before it is written.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use toml_edit::{Array, DocumentMut, Item, value};

use crate::fsutil;

/// What a setting holds, and so how it is checked and written.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Shell,
    /// A number above zero, or from zero when the flag is set.
    Number(bool),
    Toggle,
    DetachKey,
    Size,
}

struct Setting {
    key: &'static str,
    title: &'static str,
    about: &'static str,
    /// What to type, with an example.
    hint: &'static str,
    default: &'static str,
    kind: Kind,
}

/// The core's settings, less `socket`: moving it leaves every client looking
/// for the core in the old place.
const SETTINGS: [Setting; 8] = [
    Setting {
        key: "shell",
        title: "Shell",
        about: "The program new terminals start when none is given. Running terminals keep theirs.",
        hint: r"A full path, or a name on PATH: C:\Program Files\Git\bin\bash.exe, /opt/homebrew/bin/fish, nu.",
        default: "this machine's shell",
        kind: Kind::Shell,
    },
    Setting {
        key: "history_mb",
        title: "Scrollback",
        about: "Megabytes of history each live terminal keeps.",
        hint: "Megabytes, like 20 for long builds. 0 keeps none.",
        default: "5",
        kind: Kind::Number(true),
    },
    Setting {
        key: "notes",
        title: "Agent notes",
        about: "Notes to parent agents, and <hc> tags on messages between agents. Off, hc types nothing on its own.",
        hint: "",
        default: "on",
        kind: Kind::Toggle,
    },
    Setting {
        key: "notes_quiet",
        title: "Note delay",
        about: "Seconds without a person typing before a note goes in.",
        hint: "Seconds, like 10 if notes land while you type. 0 sends them at once.",
        default: "3",
        kind: Kind::Number(true),
    },
    Setting {
        key: "wait_timeout",
        title: "Wait check-in",
        about: "Seconds an hc wait runs before it checks in.",
        hint: "Seconds above zero, like 600 for agents on long tasks.",
        default: "180",
        kind: Kind::Number(false),
    },
    Setting {
        key: "machine_timeout",
        title: "Machine timeout",
        about: "Seconds another machine gets to answer before its line is dialed again.",
        hint: "Seconds above zero, like 60 over a slow or far link.",
        default: "30",
        kind: Kind::Number(false),
    },
    Setting {
        key: "detach_key",
        title: "Detach key",
        about: "The ctrl chord that leaves hc attach.",
        hint: r"ctrl+ and a letter or one of @ [ \ ] ^ _, like ctrl+] or ctrl+q.",
        default: "ctrl+\\",
        kind: Kind::DetachKey,
    },
    Setting {
        key: "headless",
        title: "Unwatched size",
        about: "COLSxROWS of a terminal nobody is looking at.",
        hint: "Columns x rows, like 160x50. Agents running unwatched see this size.",
        default: "200x120",
        kind: Kind::Size,
    },
];

/// The file, where the core looks for it: XDG_CONFIG_HOME, else %APPDATA% on
/// Windows and ~/.config elsewhere, macOS included.
pub fn path() -> PathBuf {
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = var("XDG_CONFIG_HOME").unwrap_or_else(|| {
        if cfg!(windows) {
            var("APPDATA").unwrap_or_else(|| var("USERPROFILE").unwrap_or_else(|| r"C:\".into()).join(r"AppData\Roaming"))
        } else {
            var("HOME").unwrap_or_else(|| "/".into()).join(".config")
        }
    });
    base.join("herdr-core").join("config.toml")
}

fn read(path: &Path) -> Result<DocumentMut> {
    match std::fs::read_to_string(path) {
        Ok(text) => text.parse().with_context(|| format!("{} is not valid TOML, so the core uses its defaults", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
    }
}

/// A value as the screen shows and sends it: a shell as its JSON list.
fn text(item: &Item) -> Option<String> {
    let v = item.as_value()?;
    Some(match v {
        toml_edit::Value::String(s) => s.value().clone(),
        toml_edit::Value::Array(a) => serde_json::to_string(&a.iter().map(|x| x.as_str().unwrap_or_default()).collect::<Vec<_>>()).ok()?,
        other => other.to_string().trim().to_string(),
    })
}

/// Every setting with its value, the shells found here, and why the file
/// cannot be read, if it cannot.
pub fn describe() -> Value {
    let path = path();
    let (doc, error) = match read(&path) {
        Ok(doc) => (doc, None),
        Err(e) => (DocumentMut::new(), Some(format!("{e:#}"))),
    };
    let settings: Vec<Value> =
        SETTINGS.iter().map(|s| json!({ "key": s.key, "title": s.title, "about": s.about, "hint": s.hint, "default": s.default, "value": doc.get(s.key).and_then(text) })).collect();
    let shells: Vec<Value> = shells().into_iter().map(|(name, argv)| json!({ "name": name, "argv": argv })).collect();
    json!({ "path": path, "error": error, "settings": settings, "shells": shells })
}

/// Checks text as the core would read it, and returns it as TOML.
fn parse(s: &Setting, text: &str) -> Result<Item> {
    let text = text.trim();
    let number = |zero_ok: bool| -> Result<Item> {
        let n: f64 = text.parse().ok().filter(|n: &f64| n.is_finite()).with_context(|| format!("{} must be a number", s.title))?;
        if n < 0.0 || (n == 0.0 && !zero_ok) {
            bail!("{} must be {}", s.title, if zero_ok { "zero or more" } else { "more than zero" });
        }
        Ok(if n.fract() == 0.0 && n < 1e15 { value(n as i64) } else { value(n) })
    };
    Ok(match s.kind {
        Kind::Shell => {
            let argv: Vec<String> = serde_json::from_str(text).context("the shell must be a list of strings")?;
            if argv.first().is_none_or(|exe| exe.trim().is_empty()) {
                bail!("the shell needs a program");
            }
            value(argv.into_iter().collect::<Array>())
        }
        Kind::Number(zero_ok) => number(zero_ok)?,
        Kind::Toggle => match text {
            "true" | "on" => value(true),
            "false" | "off" => value(false),
            _ => bail!("{} is on or off", s.title),
        },
        Kind::DetachKey => {
            let mut key = text.strip_prefix("ctrl+").unwrap_or_default().chars();
            let ok = matches!((key.next(), key.next()), (Some(c), None) if c.is_ascii_alphabetic() || "@[\\]^_".contains(c));
            if !ok {
                bail!("the detach key is ctrl+ and a letter or one of @ [ \\ ] ^ _, like ctrl+]");
            }
            value(text.to_ascii_lowercase())
        }
        Kind::Size => {
            let ok = text.split_once('x').and_then(|(c, r)| Some((c.trim().parse::<u16>().ok()?, r.trim().parse::<u16>().ok()?))).is_some_and(|(c, r)| c > 0 && r > 0);
            if !ok {
                bail!("the size is COLSxROWS, like 200x120");
            }
            value(text.replace(' ', ""))
        }
    })
}

/// Sets key to text, or back to the core's default when text is None.
pub fn set(key: &str, text: Option<&str>) -> Result<()> {
    let s = SETTINGS.iter().find(|s| s.key == key).with_context(|| format!("no core setting {key:?}"))?;
    let item = text.map(|t| parse(s, t)).transpose()?;
    let path = path();
    let mut doc = read(&path)?;
    match item {
        Some(item) => doc[key] = item,
        None => {
            doc.remove(key);
        }
    }
    std::fs::create_dir_all(path.parent().context("config path")?)?;
    fsutil::write_backed_up(&path, doc.to_string().as_bytes(), 0o644)
}

/// Shells to offer, by name, as the argv the core would start. PowerShell is
/// not among them: on Windows it is the default, which adds the hook that lets
/// the core follow the shell's folder, so the screen offers it as the default.
fn shells() -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    if cfg!(windows) {
        let on_path = |exe: &str| std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(exe)).find(|f| f.is_file()));
        let git_bash = on_path("git.exe")
            .and_then(|git| Some(git.parent()?.parent()?.join(r"bin\bash.exe")))
            .filter(|b| b.is_file())
            .or_else(|| Some(PathBuf::from(r"C:\Program Files\Git\bin\bash.exe")).filter(|b| b.is_file()));
        if let Some(bash) = git_bash {
            out.push(("Git Bash".into(), vec![bash.display().to_string(), "--login".into(), "-i".into()]));
        }
        for (name, exe) in [("Nushell", "nu.exe"), ("WSL", "wsl.exe")] {
            if let Some(f) = on_path(exe) {
                out.push((name.into(), vec![f.display().to_string()]));
            }
        }
    } else {
        let listed = std::fs::read_to_string("/etc/shells").unwrap_or_default();
        for line in listed.lines().map(str::trim).filter(|l| l.starts_with('/')) {
            let name = line.rsplit('/').next().unwrap_or(line).to_string();
            if Path::new(line).is_file() && !out.iter().any(|(n, _): &(String, Vec<String>)| *n == name) {
                // The core starts its own default shell as a login shell on macOS.
                let argv = if cfg!(target_os = "macos") { vec![line.to_string(), "-l".into()] } else { vec![line.to_string()] };
                out.push((name, argv));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(key: &str, text: &str) -> Result<Item> {
        parse(SETTINGS.iter().find(|s| s.key == key).unwrap(), text)
    }

    // A value the core refuses makes it drop the whole file for its
    // defaults, so nothing it would refuse is written.
    #[test]
    fn values_are_checked_as_the_core_reads_them() {
        assert_eq!(check("history_mb", "0").unwrap().to_string().trim(), "0");
        assert_eq!(check("notes_quiet", "1.5").unwrap().to_string().trim(), "1.5");
        assert_eq!(check("detach_key", "ctrl+]").unwrap().as_str(), Some("ctrl+]"));
        assert_eq!(check("headless", "160 x 50").unwrap().as_str(), Some("160x50"));
        assert_eq!(check("notes", "off").unwrap().as_bool(), Some(false));
        for (key, bad) in [
            ("wait_timeout", "0"),
            ("history_mb", "-1"),
            ("history_mb", "lots"),
            ("detach_key", "alt+x"),
            ("detach_key", "ctrl+1"),
            ("headless", "0x10"),
            ("shell", "[]"),
            ("shell", "nu"),
            ("notes", "maybe"),
        ] {
            assert!(check(key, bad).is_err(), "{key} = {bad}");
        }
    }

    // Comments and settings the screen does not show stay as they were.
    #[test]
    fn edits_keep_the_rest_of_the_file() {
        let _guard = crate::ui::tests::isolated();
        let path = path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "# mine\nsocket = \"/tmp/x.sock\"\nhistory_mb = 5\n").unwrap();
        set("shell", Some(r#"["C:\\Program Files\\Git\\bin\\bash.exe", "--login", "-i"]"#)).unwrap();
        set("history_mb", None).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# mine\nsocket = \"/tmp/x.sock\"\n"), "{text}");
        assert!(!text.contains("history_mb"));
        let shell = describe()["settings"][0]["value"].as_str().unwrap().to_string();
        assert_eq!(serde_json::from_str::<Vec<String>>(&shell).unwrap()[2], "-i");
        assert!(set("socket", Some("/elsewhere")).is_err());
        std::fs::write(&path, "shell = [").unwrap();
        assert!(describe()["error"].as_str().unwrap().contains("defaults"));
        assert!(set("notes", Some("off")).is_err(), "a broken file is left for the person to fix");
    }
}
