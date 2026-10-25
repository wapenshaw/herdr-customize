//! Editing one value, shared by the Settings page and a plugin's settings
//! screen: the values ←/→ step through, the text a field starts from and
//! how its input is read back.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use ratatui::style::Modifier;
use ratatui::text::Span;

use super::Ctx;
use super::widgets::*;
use crate::config::{self, Value};
use crate::schema;

/// How long ←/→ must rest before the picked value is saved, so cycling
/// through values writes the config once.
pub const PICK_DELAY: Duration = Duration::from_millis(400);

/// A value picked with ←/→, saved after PICK_DELAY. file names the plugin
/// config file it belongs to; None is Herdr's config.
#[derive(Debug, Clone)]
pub struct Pending {
    pub key: String,
    pub value: Value,
    pub file: Option<String>,
    pub seq: u64,
}

/// Numbers picks on both screens, so a delayed save only ever matches the
/// pick that scheduled it.
static SEQ: AtomicU64 = AtomicU64::new(0);

/// Moves the pick for k, which is set to value in file, to the previous or
/// next listed value and schedules its save. Returns the pick on another row
/// that it replaced, for the caller to save now.
pub fn cycle(pending: &mut Option<Pending>, k: &schema::Key, value: Option<&Value>, file: Option<&str>, dir: &str, ctx: &mut Ctx) -> Option<Pending> {
    let opts: Vec<Choice> = options(k).into_iter().filter(|o| !o.custom).collect();
    if opts.is_empty() {
        return None;
    }
    let replaced = pending.take_if(|p| p.key != k.key || p.file.as_deref() != file);
    let current = pending.as_ref().map_or_else(|| effective(k, value), |p| p.value.clone());
    let i = step(selected_option(&opts, &current), opts.len(), dir);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    *pending = Some(Pending { key: k.key.clone(), value: opts[i].value.clone(), file: file.map(String::from), seq });
    ctx.debounced(seq);
    replaced
}

/// Structured types are edited as TOML values; the rest as plain text.
fn structured(k: &schema::Key) -> bool {
    [schema::ARRAY, schema::STRING_LIST, schema::TOKEN_ROWS, schema::TOKEN_TABLES].contains(&k.kind.as_str())
}

pub fn edit_text(k: &schema::Key, value: &Value) -> String {
    match value {
        Value::String(s) if !structured(k) => s.clone(),
        _ => config::encode(value).unwrap_or_else(|_| value.to_string()),
    }
}

/// Turns the text field into the value to store.
pub fn parse_value(k: &schema::Key, text: &str) -> anyhow::Result<Value> {
    Ok(match k.kind.as_str() {
        schema::INTEGER => Value::Integer(text.trim().parse().map_err(|_| anyhow::anyhow!("{} needs a whole number", k.key))?),
        schema::FLOAT => Value::Float(text.trim().parse().map_err(|_| anyhow::anyhow!("{} needs a number", k.key))?),
        _ if structured(k) => config::decode_value(text).map_err(|_| anyhow::anyhow!("{} needs a TOML value such as {}", k.key, k.default))?,
        _ => Value::String(text.into()),
    })
}

/// Moves from option i (None for none) to the previous or next one.
pub fn step(i: Option<usize>, n: usize, dir: &str) -> usize {
    match (i, dir) {
        (None, "left") => n - 1,
        (Some(i), "left") => (i + n - 1) % n,
        (None, _) => 0,
        (Some(i), _) => (i + 1) % n,
    }
}

/// One value offered in place on a setting's row.
#[derive(Debug, Clone)]
pub struct Choice {
    pub label: String,
    pub value: Value,
    /// Opens the text field for a value not listed.
    pub custom: bool,
}

/// The values offered for k: on/off for booleans, the known values
/// otherwise, and "other…" when other values are valid too.
pub fn options(k: &schema::Key) -> Vec<Choice> {
    let choice = |label: &str, value: Value| Choice { label: label.into(), value, custom: false };
    if k.kind == schema::BOOLEAN {
        return vec![choice("off", Value::Boolean(false)), choice("on", Value::Boolean(true))];
    }
    let choices = k.choices();
    if choices.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Choice> = choices.iter().map(|c| choice(if c.is_empty() { "off" } else { c }, Value::String(c.clone()))).collect();
    if k.open() {
        out.push(Choice { label: "other…".into(), value: Value::String(String::new()), custom: true });
    }
    out
}

/// The value in force for k: value when set, or the default.
pub fn effective(k: &schema::Key, value: Option<&Value>) -> Value {
    if let Some(v) = value.cloned().or_else(|| k.default_value()) {
        return v;
    }
    // Some defaults are prose, such as compact or "stable" (with a note).
    let text = &k.default;
    if let Some((_, rest)) = text.split_once('"')
        && let Some((quoted, _)) = rest.split_once('"')
    {
        return Value::String(quoted.into());
    }
    Value::String(text.clone())
}

pub fn selected_option(opts: &[Choice], value: &Value) -> Option<usize> {
    opts.iter().position(|o| !o.custom && o.value == *value)
}

/// A row's values with the one in force marked, or the one about to be
/// saved highlighted.
pub fn strip(k: &schema::Key, value: Option<&Value>, opts: &[Choice], pending: Option<&Pending>) -> Vec<Span<'static>> {
    let set = value.is_some();
    let value = effective(k, value);
    let current = selected_option(opts, &value);
    let picked = pending.filter(|p| p.key == k.key).and_then(|p| selected_option(opts, &p.value));
    let mut spans: Vec<Span<'static>> = opts
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let other = o.custom && current.is_none() && set;
            let label = if other { format_value(&value) } else { o.label.clone() };
            let chosen = Some(i) == current || other;
            match () {
                _ if Some(i) == picked => styled(TAB_ACTIVE, format!(" {label} ")),
                _ if picked.is_none() && chosen && set => styled(ACCENT.add_modifier(Modifier::BOLD), format!("[{label}]")),
                _ if picked.is_none() && chosen => heading(format!("[{label}]")),
                _ => dim(format!(" {label} ")),
            }
        })
        .collect();
    if set {
        spans.push(raw(" *"));
    }
    spans
}

pub fn format_value(value: &Value) -> String {
    match value {
        Value::String(s) if s.is_empty() => "\"\"".into(),
        Value::String(s) => s.clone(),
        _ => config::encode(value).unwrap_or_else(|_| value.to_string()),
    }
}
