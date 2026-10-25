//! The Plugins page screen for one plugin's own settings: every file in its
//! Herdr config directory, typed by its config-schema.json or by inference.

use std::sync::Mutex;

use ratatui::text::Line;

use super::settings::{PICK_DELAY, Pending, SettingRow, edit_text, effective, format_value, options, parse_value, selected_option, step, strip};
use super::widgets::*;
use super::{Ctx, Key, Msg};
use crate::config::Value;
use crate::pluginconfig::{self, Config, Entry, File, Format, SCHEMA_FILE};
use crate::plugins::{self, Plugin};
use crate::{schema, theme};

/// Keeps writes to plugin config files in order.
static WRITES: Mutex<()> = Mutex::new(());

pub struct PluginConfig {
    pub plugin: Plugin,
    cfg: Option<Config>,
    err: Option<String>,
    /// (file, entry) per row; entry None is the file's heading.
    rows: Vec<(usize, Option<usize>)>,
    pub list: List,
    pub editing: bool,
    input: TextInput,
    /// A value picked with ←/→ in pending_file, saved after PICK_DELAY like
    /// on the Settings page.
    pending: Option<Pending>,
    pending_file: String,
    /// Saves still running; the files are read again once the last one is
    /// done, and until then rows show the values being saved.
    saving: usize,
}

pub fn load(pl: &Plugin) -> impl FnOnce() -> Msg + Send + 'static {
    let (id, root) = (pl.id.clone(), pl.root.clone());
    move || {
        let cfg = plugins::config_dir(&id).and_then(|dir| pluginconfig::load(&dir, root.as_ref()));
        Msg::PluginConfig { id, cfg: cfg.map_err(|e| format!("{e:#}")) }
    }
}

fn as_row(e: &Entry) -> SettingRow {
    SettingRow { key: e.key.clone(), value: e.value.clone(), ..Default::default() }
}

impl PluginConfig {
    pub fn new(plugin: Plugin) -> PluginConfig {
        PluginConfig { plugin, cfg: None, err: None, rows: Vec::new(), list: List::new(), editing: false, input: TextInput::default(), pending: None, pending_file: String::new(), saving: 0 }
    }

    fn loaded(&mut self, cfg: &Result<Config, String>) {
        (self.cfg, self.err) = match cfg {
            Ok(c) => (Some(c.clone()), None),
            Err(e) => (None, Some(e.clone())),
        };
        let (mut rows, mut labels, mut headers) = (Vec::new(), Vec::new(), Vec::new());
        for (i, f) in self.cfg.iter().flat_map(|c| c.files.iter()).enumerate() {
            rows.push((i, None));
            labels.push(f.name.clone());
            headers.push(true);
            for (j, e) in f.entries.iter().enumerate() {
                rows.push((i, Some(j)));
                labels.push(format!("{} {}", e.key.key, e.key.description));
                headers.push(false);
            }
        }
        self.rows = rows;
        self.list.set_rows(labels, headers);
    }

    /// The file under the cursor and, on a setting, the setting.
    fn current(&self) -> Option<(&File, Option<&Entry>)> {
        let (fi, ei) = *self.rows.get(self.list.selected()?)?;
        let f = self.cfg.as_ref()?.files.get(fi)?;
        Some((f, ei.and_then(|e| f.entries.get(e))))
    }

    pub fn help(&self) -> &'static str {
        if self.editing {
            return "↵ save · esc cancel";
        }
        match self.current() {
            None => "r reload · esc back",
            Some((_, None)) => "E edit file · / search · r reload · esc back",
            Some((_, Some(e))) if e.read_only => "E edit file · / search · r reload · esc back",
            Some((_, Some(e))) if e.key.open() => "←/→ pick · ↵ type a value · x reset · E edit file · esc back",
            Some((_, Some(e))) if !options(&e.key).is_empty() => "←/→ pick · x reset · E edit file · / search · esc back",
            Some(_) => "↵ change · x reset · E edit file · / search · esc back",
        }
    }

    /// Takes esc so a picked value is saved before going back.
    pub fn claims(&self, key: &Key) -> bool {
        !self.editing && !self.list.filtering && key.is("esc")
    }

    pub fn update(&mut self, msg: &mut Msg, ctx: &mut Ctx) {
        match msg {
            Msg::PluginConfig { id, cfg } if *id == self.plugin.id => self.loaded(cfg),
            Msg::PluginConfigSaved { id, what, err } if *id == self.plugin.id => {
                self.saving = self.saving.saturating_sub(1);
                if self.saving == 0 {
                    ctx.spawn(load(&self.plugin));
                }
                match err {
                    Some(e) => ctx.error(format!("{what}: {e}")),
                    None => ctx.status(format!("saved {what}; {} sees it the next time it reads its config", self.plugin.name)),
                }
            }
            Msg::PluginConfigReload(id) if *id == self.plugin.id => ctx.spawn(load(&self.plugin)),
            Msg::PluginConfigPick(seq) if self.pending.as_ref().is_some_and(|p| p.seq == *seq) => {
                self.flush(ctx);
            }
            _ => {}
        }
    }

    pub fn key(&mut self, key: &Key, ctx: &mut Ctx) {
        if self.editing {
            return self.edit_key(key, ctx);
        }
        if self.list.handle(key) {
            return self.flush(ctx);
        }
        if key.is("r") {
            self.flush(ctx);
            return ctx.spawn(load(&self.plugin));
        }
        let Some((f, e)) = self.current() else { return };
        let (f, e) = (f.clone(), e.cloned());
        if key.is("E") {
            self.flush(ctx);
            return self.edit_file(&f, ctx);
        }
        let Some(e) = e else {
            if key.is("enter") {
                self.edit_file(&f, ctx);
            }
            return;
        };
        if e.read_only {
            return ctx.status(format!("{} is a list of tables; press E to edit the file", e.key.key));
        }
        match key.name.as_str() {
            "left" | "right" => self.cycle(&f, &e, &key.name, ctx),
            "enter" | " " => {
                if e.key.kind == schema::BOOLEAN {
                    let current = matches!(effective(&as_row(&e)), Value::Boolean(true));
                    return self.save(&f, &e.key.key, Value::Boolean(!current), ctx);
                }
                if !options(&e.key).is_empty() && (!e.key.open() || key.is(" ")) {
                    return self.cycle(&f, &e, "right", ctx);
                }
                let text = e.value.clone().or_else(|| e.key.default_value()).map(|v| edit_text(&e.key, &v)).unwrap_or_default();
                self.input = TextInput::new(format!("{} = ", e.key.key), &text);
                self.editing = true;
            }
            "x" => {
                if !e.set() {
                    return ctx.status(format!("{} already uses the default", e.key.key));
                }
                self.assume(&f.name, &e.key.key, None);
                let key = e.key.key.clone();
                self.run(format!("{key} default"), move || f.unset(&key), ctx);
            }
            _ => {}
        }
    }

    fn edit_key(&mut self, key: &Key, ctx: &mut Ctx) {
        match key.name.as_str() {
            "esc" => self.editing = false,
            "enter" => {
                self.editing = false;
                let Some((f, Some(e))) = self.current() else { return };
                let (f, e) = (f.clone(), e.clone());
                match parse_value(&e.key, &self.input.value()) {
                    Ok(value) => self.save(&f, &e.key.key, value, ctx),
                    Err(err) => {
                        self.editing = true;
                        ctx.error(err);
                    }
                }
            }
            _ => {
                self.input.handle(key);
            }
        }
    }

    /// Shows value on the row until the files are read again.
    fn assume(&mut self, file: &str, key: &str, value: Option<Value>) {
        let entry = self.cfg.iter_mut().flat_map(|c| c.files.iter_mut()).filter(|f| f.name == file).flat_map(|f| f.entries.iter_mut()).find(|e| e.key.key == key);
        if let Some(e) = entry {
            e.value = value;
        }
    }

    /// Writes key, showing the new value at once.
    fn save(&mut self, f: &File, key: &str, value: Value, ctx: &mut Ctx) {
        self.assume(&f.name, key, Some(value.clone()));
        let (f, key2) = (f.clone(), key.to_string());
        self.run(key.to_string(), move || f.set(&key2, &value), ctx);
    }

    fn run(&mut self, what: String, write: impl FnOnce() -> anyhow::Result<()> + Send + 'static, ctx: &mut Ctx) {
        let id = self.plugin.id.clone();
        self.saving += 1;
        ctx.spawn(move || {
            let _guard = WRITES.lock().unwrap_or_else(|e| e.into_inner());
            Msg::PluginConfigSaved { id, what, err: write().err().map(|e| format!("{e:#}")) }
        });
    }

    /// Moves e to the previous or next listed value, saving after a pause.
    fn cycle(&mut self, f: &File, e: &Entry, dir: &str, ctx: &mut Ctx) {
        let opts: Vec<_> = options(&e.key).into_iter().filter(|o| !o.custom).collect();
        if opts.is_empty() {
            return;
        }
        let same = self.pending.as_ref().is_some_and(|p| p.key == e.key.key) && self.pending_file == f.name;
        let value = match &self.pending {
            Some(p) if same => p.value.clone(),
            _ => effective(&as_row(e)),
        };
        let i = step(selected_option(&opts, &value), opts.len(), dir);
        let seq = self.pending.as_ref().map_or(1, |p| p.seq + 1);
        if !same {
            self.flush(ctx);
        }
        self.pending = Some(Pending { key: e.key.key.clone(), value: opts[i].value.clone(), seq });
        self.pending_file = f.name.clone();
        ctx.after(PICK_DELAY, Msg::PluginConfigPick(seq));
    }

    /// Saves the pending value now.
    pub fn flush(&mut self, ctx: &mut Ctx) {
        let Some(p) = self.pending.take() else { return };
        let file = self.cfg.iter().flat_map(|c| c.files.iter()).find(|f| f.name == self.pending_file).cloned();
        if let Some(f) = file {
            self.save(&f, &p.key, p.value, ctx);
        }
    }

    /// Opens a config file in the user's editor; the plugin validates it, not
    /// Herdr, so it is edited in place.
    fn edit_file(&mut self, f: &File, ctx: &mut Ctx) {
        let cmd = match theme::editor_command(&f.path) {
            Ok(cmd) => cmd,
            Err(e) => return ctx.error(e),
        };
        let (id, name) = (self.plugin.id.clone(), f.name.clone());
        ctx.exec(cmd, move |ok| match ok {
            Ok(()) => Msg::PluginConfigReload(id),
            Err(e) => Msg::PluginConfigSaved { id, what: name, err: Some(format!("editor: {e}")) },
        });
    }

    pub fn view(&mut self, width: usize, height: usize, mut lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
        let mut title = vec![heading(format!("Settings · {}", self.plugin.name))];
        if let Some(c) = &self.cfg {
            title.push(dim(format!(" · {}", c.dir.display())));
        }
        lines.push(Line::from(title));
        let cfg = match (&self.err, &self.cfg) {
            (Some(e), _) => {
                lines.extend([line(error(e.clone())), line(dim("r retries"))]);
                return lines;
            }
            (_, None) => {
                lines.push(line(dim("loading…")));
                return lines;
            }
            (_, Some(c)) => c,
        };
        if let Some(e) = &cfg.schema_err {
            lines.push(line(warn(format!("{SCHEMA_FILE}: {e}"))));
        }
        if cfg.files.is_empty() {
            lines.push(Line::raw(format!("{} has no config files yet.", self.plugin.name)));
            lines.push(line(dim(format!("Plugins create them on first run, or ship {SCHEMA_FILE} to describe their settings here."))));
            return lines;
        }
        let detail = self.detail(width);
        let list_height = height.saturating_sub(lines.len() + detail.len() + 1).max(3);
        let key_width = (width / 3).clamp(24, 40);
        let rows = self.list.window(list_height);
        let body = self.list.render(rows, |i, selected| {
            let (fi, ei) = self.rows[i];
            let f = &cfg.files[fi];
            let Some(ei) = ei else {
                return Line::from(vec![heading(f.name.clone()), dim(file_note(f))]);
            };
            let e = &f.entries[ei];
            let mut spans = vec![raw(pad(&e.key.key, key_width)), raw(" ")];
            let opts = options(&e.key);
            if !opts.is_empty() && !e.read_only {
                let pending = self.pending.as_ref().filter(|_| self.pending_file == f.name);
                spans.extend(strip(&as_row(e), &opts, pending));
            } else if let Some(v) = &e.value {
                spans.extend([accent(format_value(v)), raw(" *")]);
            } else {
                spans.push(dim(first_non_empty(&[&e.key.default, "unset"]).to_string()));
            }
            row(Line::from(spans), selected)
        });
        lines.extend(fit(body, width, list_height));
        lines.push(Line::default());
        lines.extend(detail);
        lines
    }

    fn detail(&self, width: usize) -> Vec<Line<'static>> {
        if self.editing {
            return vec![self.input.view()];
        }
        let Some((_, Some(e))) = self.current() else { return Vec::new() };
        let mut lines: Vec<Line<'static>> = wrap(&e.key.description, width).into_iter().map(Line::raw).collect();
        let mut about = e.key.kind.clone();
        if !e.key.values.is_empty() {
            about += &format!(": {}", e.key.values.join(", "));
        }
        if !e.key.default.is_empty() {
            about += &format!(" · default {}", e.key.default);
        }
        if !e.documented {
            about += " · type inferred from the file";
        }
        lines.push(line(dim(about)));
        lines
    }
}

fn file_note(f: &File) -> &'static str {
    match () {
        _ if f.missing => "  not created yet; the first change creates it",
        _ if f.format == Format::Other => "  ↵ opens it in your editor",
        _ if f.entries.is_empty() => "  no settings found; E opens it",
        _ => "",
    }
}

/// The panel's one-line summary of a plugin's settings.
pub fn summary(cfg: &Config) -> String {
    let editable: Vec<&File> = cfg.files.iter().filter(|f| f.format != Format::Other).collect();
    let entries: usize = editable.iter().map(|f| f.entries.len()).sum();
    match editable.len() {
        0 if cfg.files.is_empty() => "none yet".into(),
        0 => format!("{} file(s) to edit", cfg.files.len()),
        1 => format!("{entries} in {}", editable[0].name),
        n => format!("{entries} in {n} files"),
    }
}
