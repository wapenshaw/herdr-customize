//! Herdr key strings: parsing, conflicts between bindings, and Herdr's
//! documented rules for where a key may be used.

use std::collections::HashMap;
use std::fmt;

use anyhow::{Result, anyhow, bail};

use crate::config::Value;

/// One parsed key such as prefix+shift+t.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Combo {
    pub prefix: bool,
    /// In canonical order: ctrl, alt, shift, cmd.
    pub mods: Vec<&'static str>,
    pub key: String,
}

const MOD_ORDER: [&str; 4] = ["ctrl", "alt", "shift", "cmd"];

fn modifier(name: &str) -> Option<&'static str> {
    Some(match name {
        "ctrl" | "control" => "ctrl",
        "alt" | "option" | "opt" | "meta" => "alt",
        "shift" => "shift",
        "cmd" | "command" | "super" | "win" => "cmd",
        _ => return None,
    })
}

fn key_alias(name: &str) -> Option<&'static str> {
    Some(match name {
        "escape" => "esc",
        "return" => "enter",
        "spacebar" | " " => "space",
        "-" => "minus",
        "," => "comma",
        "." => "period",
        "`" => "backtick",
        "&" => "ampersand",
        "=" => "equal",
        "/" => "slash",
        ";" => "semicolon",
        "'" => "quote",
        "[" => "leftbracket",
        "]" => "rightbracket",
        "\\" => "backslash",
        "pgup" => "pageup",
        "pgdown" => "pagedown",
        "del" => "delete",
        "ins" => "insert",
        "arrowup" => "up",
        "arrowdown" => "down",
        "arrowleft" => "left",
        "arrowright" => "right",
        _ => return None,
    })
}

const NAMED_KEYS: &[&str] = &[
    "enter",
    "tab",
    "esc",
    "space",
    "backspace",
    "delete",
    "insert",
    "home",
    "end",
    "pageup",
    "pagedown",
    "up",
    "down",
    "left",
    "right",
    "minus",
    "plus",
    "comma",
    "period",
    "backtick",
    "ampersand",
    "equal",
    "slash",
    "semicolon",
    "quote",
    "leftbracket",
    "rightbracket",
    "backslash",
    "question",
    "exclamation",
    "at",
    "hash",
    "dollar",
    "percent",
    "caret",
    "asterisk",
    "underscore",
    "pipe",
    "tilde",
    "colon",
    "lessthan",
    "greaterthan",
];

/// Reads one key string. Ranges such as prefix+1..9 must be expanded with
/// expand first.
pub fn parse(s: &str) -> Result<Combo> {
    let s = s.trim();
    if s.is_empty() {
        bail!("empty key");
    }
    let mut parts: Vec<&str> = s.split('+').collect();
    // A trailing "+" names the plus key itself, as in ctrl++.
    if s == "+" {
        parts = vec!["plus"];
    } else if let Some(head) = s.strip_suffix("++") {
        parts = head.split('+').chain(["plus"]).collect();
    }
    let mut c = Combo::default();
    let last = parts.len() - 1;
    for (i, part) in parts.iter().enumerate() {
        let lower = part.trim().to_lowercase();
        if i < last {
            if lower == "prefix" && i == 0 {
                c.prefix = true;
                continue;
            }
            let m = modifier(&lower).ok_or_else(|| anyhow!("unknown modifier {part:?} in {s:?}"))?;
            if !c.mods.contains(&m) {
                c.mods.push(m);
            }
            continue;
        }
        let mut key = canonical_key(part.trim()).map_err(|e| anyhow!("{e} in {s:?}"))?;
        // An uppercase letter is the shifted letter.
        let mut chars = key.chars();
        if let (Some(ch), None) = (chars.next(), chars.next())
            && ch.is_uppercase()
        {
            key = ch.to_lowercase().collect();
            if !c.mods.contains(&"shift") {
                c.mods.push("shift");
            }
        }
        c.key = key;
    }
    c.mods.sort_by_key(|m| MOD_ORDER.iter().position(|o| o == m));
    Ok(c)
}

fn canonical_key(key: &str) -> Result<String> {
    if key.is_empty() {
        bail!("missing key");
    }
    if key.chars().count() == 1 {
        return Ok(key_alias(key).map(String::from).unwrap_or_else(|| key.to_string()));
    }
    let lower = key.to_lowercase();
    if let Some(alias) = key_alias(&lower) {
        return Ok(alias.into());
    }
    if NAMED_KEYS.contains(&lower.as_str()) {
        return Ok(lower);
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|d| d.parse::<u32>().ok().filter(|n| n.to_string() == d))
        && (1..=24).contains(&n)
    {
        return Ok(lower);
    }
    bail!("unknown key {key:?}")
}

impl fmt::Display for Combo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.prefix {
            f.write_str("prefix+")?;
        }
        for m in &self.mods {
            write!(f, "{m}+")?;
        }
        f.write_str(&self.key)
    }
}

impl Combo {
    /// A printable key with no prefix or modifier, which would intercept
    /// typing in panes.
    pub fn plain(&self) -> bool {
        !self.prefix && self.mods.is_empty() && (self.key.chars().count() == 1 || ["space", "minus", "comma", "period", "slash"].contains(&self.key.as_str()))
    }
}

/// Turns a binding with a 1..9 range into one key per digit.
pub fn expand(s: &str) -> Vec<String> {
    match s.find("1..9") {
        Some(i) => ('1'..='9').map(|d| format!("{}{d}{}", &s[..i], &s[i + 4..])).collect(),
        None => vec![s.to_string()],
    }
}

/// Reads a binding value: a string, an array of strings, or nothing.
pub fn values(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
        Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from).collect(),
        _ => Vec::new(),
    }
}

/// A navigate-mode action, which Herdr checks separately from the rest.
pub fn navigate(action: &str) -> bool {
    action.starts_with("navigate_")
}

/// Herdr's documented rules that value breaks for action (the name under
/// [keys], such as "zoom").
pub fn problems(action: &str, value: &str) -> Vec<String> {
    let mut out = Vec::new();
    for key in expand(value) {
        let c = match parse(&key) {
            Ok(c) => c,
            Err(e) => {
                out.push(e.to_string());
                continue;
            }
        };
        if action == "prefix" {
            if c.prefix {
                out.push("the prefix key cannot itself use prefix+".into());
            }
        } else if navigate(action) {
            if c.prefix {
                out.push("navigate-mode keys cannot use prefix+".into());
            }
            if c.mods.is_empty() && ["esc", "enter", "tab", "left", "right", "1", "2", "3", "4", "5", "6", "7", "8", "9"].contains(&c.key.as_str()) {
                out.push(format!("navigate mode reserves {:?}", c.key));
            }
            if c.key == "tab" && c.mods == ["shift"] {
                out.push("navigate mode reserves shift+tab".into());
            }
        } else if c.plain() {
            out.push(format!("{key:?} without prefix+ or a modifier intercepts typing in panes"));
        }
    }
    out
}

/// One thing that owns keys.
#[derive(Debug, Clone, Default)]
pub struct Binding {
    pub owner: String,
    pub keys: Vec<String>,
    /// Navigate bindings only apply in navigate mode.
    pub navigate: bool,
}

fn mode_key(navigate: bool, c: &Combo) -> String {
    if navigate { format!("navigate:{c}") } else { c.to_string() }
}

fn parsed(b: &Binding) -> impl Iterator<Item = String> + '_ {
    b.keys.iter().flat_map(|v| expand(v)).filter_map(|k| parse(&k).ok()).map(|c| mode_key(b.navigate, &c))
}

/// Maps each key used by more than one binding in the same mode to the
/// owners that use it. Navigate-mode keys are prefixed with "navigate:".
pub fn conflicts(bindings: &[Binding]) -> HashMap<String, Vec<String>> {
    let mut owners: HashMap<String, Vec<String>> = HashMap::new();
    for b in bindings {
        for id in parsed(b) {
            let list = owners.entry(id).or_default();
            if !list.contains(&b.owner) {
                list.push(b.owner.clone());
            }
        }
    }
    owners.retain(|_, list| list.len() > 1);
    owners
}

/// The other owners that share a key with b.
pub fn clashes(conflicts: &HashMap<String, Vec<String>>, b: &Binding) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in parsed(b) {
        for other in conflicts.get(&id).into_iter().flatten() {
            if *other != b.owner && !out.contains(other) {
                out.push(other.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_canonicalizes() {
        for (input, want) in [
            ("prefix+shift+t", "prefix+shift+t"),
            ("Prefix+T", "prefix+shift+t"),
            ("shift+ctrl+K", "ctrl+shift+k"),
            ("control+option+x", "ctrl+alt+x"),
            ("prefix+-", "prefix+minus"),
            ("prefix+minus", "prefix+minus"),
            ("escape", "esc"),
            ("F12", "f12"),
            ("ctrl++", "ctrl+plus"),
            ("super+k", "cmd+k"),
            ("prefix+tab", "prefix+tab"),
        ] {
            assert_eq!(parse(input).unwrap().to_string(), want, "{input}");
        }
        for bad in ["", "prefix+zz", "hyper+x", "f25", "prefix+"] {
            assert!(parse(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn expand_ranges() {
        let got = expand("prefix+alt+1..9");
        assert_eq!(got.len(), 9);
        assert_eq!(got[0], "prefix+alt+1");
        assert_eq!(got[8], "prefix+alt+9");
    }

    #[test]
    fn problem_rules() {
        let p = problems("zoom", "z");
        assert!(p.len() == 1 && p[0].contains("intercepts typing"), "{p:?}");
        assert!(problems("zoom", "prefix+z").is_empty());
        assert!(problems("navigate_pane_down", "j").is_empty());
        for bad in ["prefix+j", "enter", "shift+tab", "3"] {
            assert!(!problems("navigate_pane_down", bad).is_empty(), "{bad}");
        }
        assert!(!problems("prefix", "prefix+b").is_empty());
        assert!(!problems("zoom", "prefix+zz").is_empty());
    }

    #[test]
    fn conflicts_and_clashes() {
        let b = |owner: &str, key: &str, navigate: bool| Binding { owner: owner.into(), keys: vec![key.into()], navigate };
        let bindings = [
            b("keys.previous_tab", "prefix+p", false),
            b("command: plugin manager", "prefix+P", false),
            b("command: other", "prefix+p", false),
            b("keys.switch_tab", "prefix+1..9", false),
            b("command: one", "prefix+1", false),
            b("keys.navigate_pane_down", "j", true),
            b("command: j", "prefix+j", false),
        ];
        let c = conflicts(&bindings);
        assert_eq!(c["prefix+p"], ["keys.previous_tab", "command: other"]);
        assert_eq!(clashes(&c, &bindings[3]), ["command: one"]);
        assert!(clashes(&c, &bindings[1]).is_empty());
        assert!(clashes(&c, &bindings[5]).is_empty());
    }
}
