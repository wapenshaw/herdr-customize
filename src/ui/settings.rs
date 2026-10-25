//! Settings: every key in Herdr's config reference with its value, edited in
//! place. ←/→ step through a setting's listed values.

use std::collections::HashMap;
use std::fs;
use std::sync::LazyLock;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use regex::Regex;

use super::values::{self, *};
use super::widgets::*;
use super::{Ctx, Key, Msg, Page, Saved, Shared, load_doc, save};
use crate::config::{self, Doc, Value};
use crate::herdr;
use crate::schema::{self, Reference};
use crate::theme::preview::rgb;

/// Names saves of the whole file from the editor in the status line.
const RAW_WHAT: &str = "config.toml";

/// A section title, or a setting with its value (None when unset).
#[derive(Debug, Clone, Default)]
struct SettingRow {
    header: Option<String>,
    key: schema::Key,
    /// A key in config.toml that Herdr does not document.
    unknown: Option<Vec<String>>,
    value: Option<Value>,
}

impl SettingRow {
    fn path(&self) -> Vec<String> {
        self.unknown.clone().unwrap_or_else(|| self.key.path())
    }

    fn name(&self) -> String {
        self.path().join(".")
    }
}

/// A value being saved, shown until its saves report back so a row does not
/// flash its old value.
struct Assumed {
    value: Option<Value>,
    inflight: usize,
}

#[derive(PartialEq)]
enum Mode {
    Browse,
    Editing,
}

pub struct SettingsPage {
    rows: Vec<SettingRow>,
    list: List,
    load_err: Option<String>,
    issues: Vec<String>,
    mode: Mode,
    input: TextInput,
    pending: Option<Pending>,
    ask: Option<Confirm<SettingsPage>>,
    /// An edit of the whole file that failed the check, so the next edit
    /// starts from it rather than losing the work.
    raw: Option<String>,
    assumed: HashMap<String, Assumed>,
}

fn check(sh: &Shared) -> impl FnOnce() -> Msg + Send + 'static {
    let path = sh.store.path.clone();
    move || Msg::Issues(herdr::check_config(&path))
}

impl SettingsPage {
    pub fn new() -> SettingsPage {
        SettingsPage {
            rows: Vec::new(),
            list: List::new(),
            load_err: None,
            issues: Vec::new(),
            mode: Mode::Browse,
            input: TextInput::default(),
            pending: None,
            ask: None,
            raw: None,
            assumed: HashMap::new(),
        }
    }

    fn current(&self) -> Option<&SettingRow> {
        self.rows.get(self.list.selected()?).filter(|r| r.header.is_none())
    }

    fn rebuild(&mut self, sh: &Shared) {
        // Decoded once: a lookup per key would re-serialize and re-parse the
        // whole document each time.
        let table = load_doc(&sh.store, &mut self.load_err).decoded();
        let mut rows = Vec::new();
        for section in &sh.reference.sections {
            // Keybindings and themes have their own pages.
            if section.id == "keys" || section.id == "theme" {
                continue;
            }
            rows.push(SettingRow { header: Some(section.title.clone()), ..Default::default() });
            for k in &section.keys {
                rows.push(SettingRow { value: config::lookup(&table, &k.path()).cloned(), key: k.clone(), ..Default::default() });
            }
        }
        let unknown = unknown_keys(&table, &sh.reference, &[]);
        if !unknown.is_empty() {
            rows.push(SettingRow { header: Some("Not Herdr settings".into()), ..Default::default() });
            for path in unknown {
                rows.push(SettingRow { value: config::lookup(&table, &path).cloned(), unknown: Some(path), ..Default::default() });
            }
        }
        self.rows = rows;
        self.apply_assumed();
        let labels = self.rows.iter().map(|r| r.header.clone().unwrap_or_else(|| format!("{} {}", r.name(), r.key.description))).collect();
        let headers = self.rows.iter().map(|r| r.header.is_some()).collect();
        self.list.set_rows(labels, headers);
    }

    fn apply_assumed(&mut self) {
        for r in &mut self.rows {
            if let Some(a) = self.assumed.get(&r.name()).filter(|_| r.header.is_none()) {
                r.value = a.value.clone();
            }
        }
    }

    /// Saves one key, or removes it when value is None. The row shows the
    /// new value at once.
    fn write(&mut self, what: String, path: Vec<String>, value: Option<Value>, ctx: &mut Ctx) {
        let name = path.join(".");
        let a = self.assumed.entry(name.clone()).or_insert(Assumed { value: None, inflight: 0 });
        a.value = value.clone();
        a.inflight += 1;
        self.apply_assumed();
        ctx.save(what, name, move |doc| match &value {
            Some(v) => doc.set(&path, v),
            None => doc.unset(&path).map(|_| ()),
        });
    }

    /// Booleans toggle, enums move to the next value and everything else
    /// gets a text field.
    fn edit(&mut self, r: SettingRow, ctx: &mut Ctx) {
        let k = &r.key;
        if k.kind == schema::BOOLEAN {
            let current = matches!(effective(k, r.value.as_ref()), Value::Boolean(true));
            return self.write(k.key.clone(), k.path(), Some(Value::Boolean(!current)), ctx);
        }
        if !options(k).is_empty() && !k.open() {
            return self.cycle(&r, "right", ctx);
        }
        self.edit_text(&r);
    }

    fn edit_text(&mut self, r: &SettingRow) {
        let text = r.value.clone().or_else(|| r.key.default_value()).map(|v| edit_text(&r.key, &v)).unwrap_or_default();
        self.input = TextInput::new(format!("{} = ", r.key.key), &text);
        self.mode = Mode::Editing;
    }

    fn edit_key(&mut self, key: &Key, ctx: &mut Ctx) {
        match key.name.as_str() {
            "esc" => self.mode = Mode::Browse,
            "enter" => {
                let Some(r) = self.current().cloned() else {
                    self.mode = Mode::Browse;
                    return;
                };
                match parse_value(&r.key, &self.input.value()) {
                    Ok(value) => {
                        self.mode = Mode::Browse;
                        self.write(r.key.key.clone(), r.key.path(), Some(value), ctx);
                    }
                    Err(e) => ctx.error(e),
                }
            }
            _ => {
                self.input.handle(key);
            }
        }
    }

    /// Moves r to the previous or next listed value, wrapping around. The
    /// save waits for PICK_DELAY so stepping through several values writes
    /// once.
    fn cycle(&mut self, r: &SettingRow, dir: &str, ctx: &mut Ctx) {
        let replaced = values::cycle(&mut self.pending, &r.key, r.value.as_ref(), None, dir, ctx);
        self.save_pick(replaced, ctx);
    }

    /// Saves the pending value now.
    fn flush(&mut self, ctx: &mut Ctx) {
        let pick = self.pending.take();
        self.save_pick(pick, ctx);
    }

    fn save_pick(&mut self, pick: Option<Pending>, ctx: &mut Ctx) {
        if let Some(p) = pick {
            let path = config::split_key(&p.key);
            self.write(p.key, path, Some(p.value), ctx);
        }
    }

    /// Opens config.toml in the user's editor through a scratch copy, so the
    /// edit is checked like any other before it replaces the file.
    fn edit_raw(&mut self, ctx: &mut Ctx) {
        let data = match &self.raw {
            Some(raw) => raw.clone(),
            None => match ctx.sh.store.load() {
                Ok(doc) => doc.text(),
                Err(e) => match fs::read_to_string(&ctx.sh.store.path) {
                    Ok(text) => text,
                    Err(_) => return ctx.error(e),
                },
            },
        };
        let path = crate::fsutil::temp_path(&std::env::temp_dir(), "herdr-config-").with_extension("toml");
        if let Err(e) = fs::write(&path, data) {
            return ctx.error(e);
        }
        ctx.edit_file(path.clone(), move |text| {
            let _ = fs::remove_file(&path);
            Msg::RawEdited(text)
        });
    }

    fn raw_edited(&mut self, text: &mut anyhow::Result<String>, ctx: &mut Ctx) {
        let data = match text {
            Ok(data) => std::mem::take(data),
            Err(e) => return ctx.error(e),
        };
        // Kept until the save succeeds, so a rejected edit can be resumed.
        self.raw = Some(data.clone());
        let doc = match Doc::parse(&data) {
            Ok(doc) => doc,
            Err(e) => return ctx.error(format!("not saved, press E to fix: {e:#}")),
        };
        let store = ctx.sh.store.clone();
        ctx.spawn(move || {
            let mut saved = save(&store, RAW_WHAT.into(), None, |d| {
                *d = doc;
                Ok(())
            });
            saved.err = saved.err.map(|e| e.context("press E to keep editing"));
            Msg::Saved(saved)
        });
    }

    /// The value column: set values stand out, defaults are dim.
    fn display(&self, r: &SettingRow) -> Vec<Span<'static>> {
        let Some(value) = &r.value else {
            return vec![dim(r.key.default.clone())];
        };
        if r.unknown.is_some() {
            return vec![warn(format_value(value)), dim("  not a Herdr setting")];
        }
        let mut spans = Vec::new();
        if let Value::String(s) = value
            && HEX_COLOR.is_match(s)
        {
            spans.push(styled(Style::new().fg(rgb(s)), "██ "));
        }
        spans.extend([accent(format_value(value)), raw(" *")]);
        spans
    }

    fn detail(&self, sh: &Shared, width: usize) -> Vec<Line<'static>> {
        if self.mode == Mode::Editing {
            return vec![self.input.view()];
        }
        let Some(r) = self.current() else {
            if self.mode == Mode::Browse && self.list.query().is_empty() {
                return vec![line(dim("Keybindings are on the Keys page and themes on the Themes page."))];
            }
            return Vec::new();
        };
        if r.unknown.is_some() {
            return vec![line(warn(format!("{} is not in Herdr's config reference {}; it may be a typo or belong to an older release.", r.name(), sh.reference.version)))];
        }
        let k = &r.key;
        let mut about = k.kind.clone();
        if !k.values.is_empty() {
            about += &format!(": {}", k.values.join(", "));
        }
        let mut lines: Vec<Line<'static>> = wrap(&k.description, width).into_iter().map(Line::raw).collect();
        lines.push(line(dim(format!("{about} · default {}", k.default))));
        let lower = k.description.to_lowercase();
        if lower.contains("server startup") || lower.contains("restart") {
            lines.push(line(warn("Takes effect when the Herdr server restarts.")));
        }
        lines
    }
}

/// Leaf keys that no reference key covers. Keybindings are left to the Keys
/// page and plugin tables are left alone.
fn unknown_keys(table: &toml::Table, reference: &Reference, prefix: &[String]) -> Vec<Vec<String>> {
    let mut names: Vec<&String> = table.keys().collect();
    names.sort();
    let mut out = Vec::new();
    for name in names {
        let mut path = prefix.to_vec();
        path.push(name.clone());
        let dotted = path.join(".");
        if dotted == "keys" || reference.lookup(&dotted).is_some() {
            continue;
        }
        match &table[name] {
            Value::Table(sub) if reference.covers_prefix(&dotted) => out.extend(unknown_keys(sub, reference, &path)),
            _ => out.push(path),
        }
    }
    out
}

static HEX_COLOR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#[0-9a-fA-F]{6}$").unwrap());

impl Page for SettingsPage {
    fn title(&self) -> &'static str {
        "Settings"
    }

    fn init(&mut self, ctx: &mut Ctx) {
        self.rebuild(ctx.sh);
        ctx.spawn(check(ctx.sh));
    }

    fn update(&mut self, msg: &mut Msg, ctx: &mut Ctx) {
        match msg {
            Msg::Reference(_) => self.rebuild(ctx.sh),
            Msg::Saved(Saved { key, result, err, .. }) => {
                match key {
                    Some(key) => match self.assumed.get_mut(key) {
                        Some(a) if a.inflight > 1 => a.inflight -= 1,
                        _ => {
                            self.assumed.remove(key);
                        }
                    },
                    None if err.is_none() => self.raw = None,
                    None => {}
                }
                self.rebuild(ctx.sh);
                if err.is_none() && result.changed {
                    ctx.spawn(check(ctx.sh));
                }
            }
            Msg::Issues(result) => self.issues = std::mem::take(&mut result.issues),
            Msg::PendingSave { seq } => {
                if self.pending.as_ref().is_some_and(|p| p.seq == *seq) {
                    self.flush(ctx);
                }
            }
            Msg::RawEdited(text) => self.raw_edited(text, ctx),
            _ => {}
        }
    }

    fn key(&mut self, key: &Key, ctx: &mut Ctx) {
        if Confirm::handle(self.ask.take(), self, key, ctx) {
            return;
        }
        if self.mode == Mode::Editing {
            return self.edit_key(key, ctx);
        }
        if self.list.handle(key) {
            // Moving away saves a value still waiting on its delay.
            return self.flush(ctx);
        }
        if key.is("E") {
            return self.edit_raw(ctx);
        }
        let Some(r) = self.current().cloned() else { return };
        match key.name.as_str() {
            "left" | "right" if r.unknown.is_none() => self.cycle(&r, &key.name, ctx),
            // Space steps through the listed values of a setting that also
            // takes others; Enter types one.
            " " if r.unknown.is_none() && r.key.open() => self.cycle(&r, "right", ctx),
            "enter" | " " if r.unknown.is_none() => self.edit(r, ctx),
            "x" => {
                if r.value.is_none() {
                    return ctx.status(format!("{} already uses the default", r.name()));
                }
                let (path, name) = (r.path(), r.name());
                if r.unknown.is_some() {
                    self.ask = Some(Confirm::new(format!("Remove {name} from config.toml?"), move |p: &mut SettingsPage, ctx| p.write(name, path, None, ctx)));
                } else {
                    self.write(format!("{name} default"), path, None, ctx);
                }
            }
            _ => {}
        }
    }

    fn view(&mut self, sh: &Shared, width: usize, height: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        if let Some(e) = &self.load_err {
            lines.push(line(error(format!("config.toml: {e}"))));
        }
        if let Some(first) = self.issues.first() {
            lines.push(line(warn(format!("herdr config check: {} issue(s): {first}", self.issues.len()))));
        }
        let detail = self.detail(sh, width);
        let list_height = height.saturating_sub(lines.len() + detail.len() + 1).max(3);
        let key_width = (width / 3).clamp(24, 40);
        let rows = self.list.window(list_height);
        let body = self.list.render(rows, |i, selected| {
            let r = &self.rows[i];
            if let Some(h) = &r.header {
                return line(heading(h.clone()));
            }
            let mut spans = vec![raw(pad(&r.name(), key_width)), raw(" ")];
            let opts = if r.unknown.is_none() { options(&r.key) } else { Vec::new() };
            if opts.is_empty() {
                spans.extend(self.display(r));
            } else {
                spans.extend(strip(&r.key, r.value.as_ref(), &opts, self.pending.as_ref()));
            }
            row(Line::from(spans), selected)
        });
        lines.extend(fit(body, width, list_height));
        lines.push(Line::default());
        lines.extend(detail);
        lines
    }

    fn help(&self) -> String {
        if let Some(help) = Confirm::help(&self.ask) {
            return help;
        }
        if self.mode == Mode::Editing {
            return "↵ save · esc cancel".into();
        }
        match self.current() {
            None => "/ search · E edit config.toml",
            Some(r) if r.unknown.is_some() => "x remove · / search · E edit config.toml",
            Some(r) if r.key.open() => "←/→ pick · ↵ type a value · x reset · / search · E edit config.toml",
            Some(r) if !options(&r.key).is_empty() => "←/→ pick · x reset · / search · E edit config.toml",
            Some(_) => "↵ change · x reset · / search · E edit config.toml",
        }
        .into()
    }

    fn capturing(&self) -> bool {
        self.mode != Mode::Browse || self.ask.is_some() || self.list.filtering
    }

    fn back(&mut self) -> bool {
        self.list.clear_filter()
    }
}
