//! Keys for the Herdr app: keys.json by section, the commands left unbound,
//! and a form that checks a key and its action before writing them.

use ratatui::text::Line;

use super::widgets::*;
use super::{self as ui, Ctx, Hint, Key, Page, Shared};
use crate::app::keys::{self, Binding};
use crate::app::{self, AppCommand, mods};

#[derive(Clone, Copy, PartialEq)]
enum Row {
    Section(usize),
    Binding(usize),
    Gap,
    Unbound,
    Command(usize),
}

/// A binding being added or changed.
struct Form {
    fields: [TextInput; 3],
    focus: usize,
    /// The binding's section and key before the edit.
    old: Option<(String, String)>,
}

impl Form {
    fn new(section: &str, key: &str, action: &str, old: Option<(String, String)>) -> Form {
        let mut fields = [TextInput::new("Section ", section), TextInput::new("Key     ", key), TextInput::new("Action  ", action)];
        let focus = if key.is_empty() { 1 } else { 2 };
        for (i, f) in fields.iter_mut().enumerate() {
            if i != focus {
                f.blur();
            }
        }
        Form { fields, focus, old }
    }

    fn value(&self, i: usize) -> String {
        self.fields[i].value().trim().to_string()
    }

    fn move_focus(&mut self, by: usize) {
        self.fields[self.focus].blur();
        self.focus = (self.focus + by) % 3;
        self.fields[self.focus].focus();
    }
}

pub struct AppKeysPage {
    bindings: Vec<Binding>,
    sections: Vec<String>,
    commands: Vec<AppCommand>,
    commands_err: Option<String>,
    mods: Vec<String>,
    rows: Vec<Row>,
    list: List,
    form: Option<Form>,
    ask: Option<Confirm<AppKeysPage>>,
    shown: bool,
}

pub enum Msg {
    Loaded { bindings: anyhow::Result<Vec<Binding>>, commands: anyhow::Result<Vec<AppCommand>>, mods: Vec<String> },
    Saved(String, anyhow::Result<()>),
    EditorDone(anyhow::Result<()>),
}

impl From<Msg> for ui::Msg {
    fn from(msg: Msg) -> ui::Msg {
        ui::Msg::AppKeys(msg)
    }
}

fn load() -> ui::Msg {
    Msg::Loaded { bindings: keys::load(), commands: app::commands(), mods: mods::mod_names() }.into()
}

/// When a section's keys count.
fn scope(section: &str) -> String {
    match section.split_once(':') {
        _ if section == "global" => "everywhere, unless a more specific section binds the key".into(),
        _ if section == "terminal" => "while a terminal has focus; wins over layouts and global".into(),
        Some(("layout", name)) => format!("while the {name} layout is shown; wins over global"),
        Some(("mod", name)) => format!("while the {name} mod has focus"),
        _ => "a section the app does not know: global, terminal, layout:<name> or mod:<name>".into(),
    }
}

impl AppKeysPage {
    pub fn new() -> AppKeysPage {
        AppKeysPage { bindings: Vec::new(), sections: Vec::new(), commands: Vec::new(), commands_err: None, mods: Vec::new(), rows: Vec::new(), list: List::new(), form: None, ask: None, shown: false }
    }

    fn selected(&self) -> Option<Row> {
        self.rows.get(self.list.selected()?).copied()
    }

    /// Whether some binding runs command, with or without a value.
    fn bound(&self, command: &str) -> bool {
        self.bindings.iter().any(|b| b.action == command || b.action.strip_prefix(command).is_some_and(|r| r.starts_with(':')))
    }

    fn rebuild(&mut self) {
        self.sections.clear();
        for b in &self.bindings {
            if !self.sections.contains(&b.section) {
                self.sections.push(b.section.clone());
            }
        }
        let (mut rows, mut labels) = (Vec::new(), Vec::new());
        for (s, section) in self.sections.iter().enumerate() {
            rows.push(Row::Section(s));
            labels.push(section.clone());
            for (i, b) in self.bindings.iter().enumerate().filter(|(_, b)| b.section == *section) {
                rows.push(Row::Binding(i));
                labels.push(format!("{} {} {}", b.key, b.action, b.section));
            }
        }
        let unbound: Vec<usize> = (0..self.commands.len()).filter(|&i| !self.bound(&self.commands[i].name)).collect();
        if !unbound.is_empty() {
            rows.extend([Row::Gap, Row::Unbound]);
            labels.extend([String::new(), "Unbound commands".into()]);
            for i in unbound {
                rows.push(Row::Command(i));
                labels.push(format!("{} {}", self.commands[i].name, self.commands[i].about));
            }
        }
        let headers = rows.iter().map(|r| !matches!(r, Row::Binding(_) | Row::Command(_))).collect();
        self.rows = rows;
        self.list.set_rows(labels, headers);
    }

    fn command(&self, action: &str) -> Option<&AppCommand> {
        let name = action.split_once(':').map_or(action, |(n, _)| n);
        self.commands.iter().find(|c| c.name == name)
    }

    /// Other sections binding the same key.
    fn shadows(&self, b: &Binding) -> Vec<&Binding> {
        let key = keys::normalize(&b.key);
        self.bindings.iter().filter(|o| o.section != b.section && keys::normalize(&o.key) == key).collect()
    }

    fn open(&mut self) {
        self.form = match self.selected() {
            Some(Row::Binding(i)) => {
                let b = &self.bindings[i];
                Some(Form::new(&b.section, &b.key, &b.action, Some((b.section.clone(), b.key.clone()))))
            }
            Some(Row::Command(i)) => {
                let c = &self.commands[i];
                let value = if c.args.iter().any(|a| a.required) { ":" } else { "" };
                Some(Form::new("global", "", &format!("{}{value}", c.name), None))
            }
            _ => Some(Form::new("global", "", "", None)),
        };
    }

    /// The form's errors, which stop a save, and its warnings.
    fn check(&self, form: &Form) -> (Vec<String>, Vec<String>) {
        let (section, key, action) = (form.value(0), form.value(1), form.value(2));
        let (mut errors, warnings) = keys::problems(&section, &key);
        if section.is_empty() {
            errors.push("a section is required".into());
        }
        if let Some(e) = keys::action_problem(&action, &self.commands, &self.mods) {
            errors.push(e);
        }
        (errors, warnings)
    }

    fn save(&mut self, ctx: &mut Ctx) {
        let Some(form) = &self.form else { return };
        let (errors, _) = self.check(form);
        if let Some(e) = errors.first() {
            return ctx.error(e);
        }
        let (section, key, action) = (form.value(0), form.value(1), form.value(2));
        let old = form.old.clone();
        let norm = keys::normalize(&key);
        let clash = self.bindings.iter().find(|b| b.section == section && keys::normalize(&b.key) == norm && old.as_ref() != Some(&(b.section.clone(), b.key.clone()))).cloned();
        let question = clash.as_ref().map(|c| format!("{} in {} runs {}. Replace it?", c.key, c.section, c.action));
        let write = move |p: &mut AppKeysPage, ctx: &mut Ctx| {
            p.form = None;
            let what = format!("{key} → {action}");
            ctx.write(move || {
                let result = (|| {
                    if let Some(c) = &clash {
                        keys::remove(&c.section, &c.key)?;
                    }
                    let same = old.as_ref().filter(|(s, _)| *s == section);
                    if let Some((s, k)) = old.as_ref().filter(|(s, _)| *s != section) {
                        keys::remove(s, k)?;
                    }
                    keys::set(&section, same.map(|(_, k)| k.as_str()), &key, &action)
                })();
                Msg::Saved(what, result).into()
            });
        };
        match question {
            Some(q) => self.ask = Some(Confirm::new(q, write)),
            None => write(self, ctx),
        }
    }

    fn form_key(&mut self, key: &Key, ctx: &mut Ctx) {
        let Some(form) = &mut self.form else { return };
        match key.name.as_str() {
            "esc" => {
                self.form = None;
                ctx.status("not saved");
            }
            "tab" | "down" => form.move_focus(1),
            "shift+tab" | "up" => form.move_focus(2),
            "enter" => self.save(ctx),
            _ => {
                form.fields[form.focus].handle(key);
            }
        }
    }

    fn form_view(&self, form: &Form) -> Vec<Line<'static>> {
        let title = if form.old.is_some() { "Change binding" } else { "New binding" };
        let mut lines = vec![line(heading(title)), Line::default()];
        lines.extend(form.fields.iter().map(TextInput::view));
        lines.push(Line::default());
        let section = form.value(0);
        lines.push(line(dim(scope(&section))));
        let (errors, warnings) = self.check(form);
        lines.extend(errors.into_iter().map(|e| line(error(format!("✗ {e}")))));
        lines.extend(warnings.into_iter().map(|w| line(warn(format!("⚠ {w}")))));
        if let Some(c) = self.command(&form.value(2)) {
            lines.push(Line::default());
            lines.push(Line::from(vec![accent(c.name.clone()), dim(format!("  {}", c.about))]));
            for a in &c.args {
                lines.push(line(dim(format!("  :{}{}", a.name, if a.required { " (required)" } else { "" }))));
            }
        }
        lines.push(Line::default());
        lines.push(line(dim("Keys: ctrl-shift-t, alt-], f2, ctrl--; a space starts a sequence.")));
        lines.push(line(dim("Actions: a command, command:value, send:text, terminal or <mod>.<command>.")));
        lines
    }

    fn detail(&self, row: Row, width: usize) -> Vec<Line<'static>> {
        let prose = |text: &str| wrap(text, width).into_iter().map(|l| line(dim(l))).collect::<Vec<_>>();
        match row {
            Row::Binding(i) => {
                let b = &self.bindings[i];
                let mut lines = vec![Line::from(vec![heading(b.key.clone()), dim(format!(" · {}", b.section))]), Line::default(), line(raw(format!("Runs {}", b.action)))];
                if let Some(c) = self.command(&b.action) {
                    lines.extend(prose(&c.about));
                }
                lines.push(Line::default());
                lines.extend(prose(&format!("Counts {}.", scope(&b.section))));
                for o in self.shadows(b) {
                    lines.push(line(warn(format!("Also bound in {} to {}", o.section, o.action))));
                }
                let (errors, warnings) = keys::problems(&b.section, &b.key);
                lines.extend(errors.into_iter().map(|e| line(error(format!("✗ {e}")))));
                lines.extend(warnings.into_iter().map(|w| line(warn(format!("⚠ {w}")))));
                if let Some(e) = keys::action_problem(&b.action, &self.commands, &self.mods) {
                    lines.push(line(error(format!("✗ {e}"))));
                }
                lines
            }
            Row::Command(i) => {
                let c = &self.commands[i];
                let mut lines = vec![line(heading(c.name.clone())), Line::default()];
                lines.extend(prose(&c.about));
                for a in &c.args {
                    lines.push(line(dim(format!("  :{}{}", a.name, if a.required { " (required)" } else { "" }))));
                }
                lines.extend([Line::default(), line(dim("No key runs it. ↵ binds one."))]);
                lines
            }
            _ => Vec::new(),
        }
    }

    fn draw_row(&self, i: usize, selected: bool, key_width: usize) -> Line<'static> {
        match self.rows[i] {
            Row::Section(s) => line(dim(format!("  {}", self.sections[s].to_uppercase()))),
            Row::Unbound => line(dim("  UNBOUND COMMANDS")),
            Row::Gap => Line::default(),
            Row::Binding(b) => {
                let b = &self.bindings[b];
                let bad = !keys::problems(&b.section, &b.key).0.is_empty() || keys::action_problem(&b.action, &self.commands, &self.mods).is_some();
                let mark = if bad {
                    error("✗ ")
                } else if !self.shadows(b).is_empty() {
                    warn("◆ ")
                } else {
                    raw("  ")
                };
                let k = pad(&clip_str(&b.key, key_width), key_width);
                row(Line::from(vec![mark, if selected { styled(SELECTED, k) } else { accent(k) }, raw(format!(" {}", b.action))]), selected)
            }
            Row::Command(c) => {
                let c = &self.commands[c];
                let name = pad(&c.name, key_width + 2);
                row(Line::from(vec![if selected { styled(SELECTED, name) } else { raw(name) }, dim(format!(" {}", c.about))]), selected)
            }
        }
    }
}

impl Page for AppKeysPage {
    fn title(&self) -> &'static str {
        "Keys"
    }

    fn activated(&mut self, ctx: &mut Ctx) {
        if !self.shown {
            self.shown = true;
            ctx.spawn(load);
        }
    }

    fn update(&mut self, msg: &mut ui::Msg, ctx: &mut Ctx) {
        let ui::Msg::AppKeys(msg) = msg else { return };
        match msg {
            Msg::Loaded { bindings, commands, mods } => {
                match std::mem::replace(bindings, Ok(Vec::new())) {
                    Ok(b) => self.bindings = b,
                    Err(e) => ctx.error(e),
                }
                match std::mem::replace(commands, Ok(Vec::new())) {
                    Ok(c) => (self.commands, self.commands_err) = (c, None),
                    Err(e) => self.commands_err = Some(format!("{e:#}")),
                }
                self.mods = std::mem::take(mods);
                self.rebuild();
            }
            Msg::Saved(what, result) => {
                match result {
                    Ok(()) => ctx.status(format!("{what} saved; the app picks it up")),
                    Err(e) => ctx.error(format!("not saved: {e:#}")),
                }
                ctx.spawn(load);
            }
            Msg::EditorDone(result) => {
                if let Err(e) = result {
                    ctx.error(e);
                }
                ctx.spawn(load);
            }
        }
    }

    fn key(&mut self, key: &Key, ctx: &mut Ctx) {
        if Confirm::handle(self.ask.take(), self, key, ctx) {
            return;
        }
        if self.form.is_some() {
            return self.form_key(key, ctx);
        }
        if self.list.handle(key) {
            return;
        }
        match key.name.as_str() {
            "enter" => self.open(),
            "n" => self.form = Some(Form::new("global", "", "", None)),
            "d" | "delete" => {
                if let Some(Row::Binding(i)) = self.selected() {
                    let b = self.bindings[i].clone();
                    self.ask = Some(Confirm::new(format!("Unbind {} in {}?", b.key, b.section), move |_: &mut AppKeysPage, ctx| {
                        let what = format!("{} unbound", b.key);
                        ctx.write(move || Msg::Saved(what, keys::remove(&b.section, &b.key)).into());
                    }));
                }
            }
            "o" => ctx.edit_file(keys::path(), |text| Msg::EditorDone(text.map(drop)).into()),
            "r" => ctx.spawn(load),
            _ => {}
        }
    }

    fn view(&mut self, _sh: &Shared, width: usize, height: usize) -> Vec<Line<'static>> {
        if let Some(form) = &self.form {
            return self.form_view(form);
        }
        let left_width = if width < 90 { width } else { width / 2 };
        let mut lines = Vec::new();
        if let Some(e) = &self.commands_err {
            lines.push(line(warn(format!("⚠ commands unknown, actions go unchecked: {e}"))));
        }
        lines.push(line(dim(format!("  {}", keys::path().display()))));
        let list_height = height.saturating_sub(lines.len()).max(1);
        let rows = self.list.window(list_height);
        let key_width = 18;
        lines.extend(self.list.render(rows, |i, s| self.draw_row(i, s, key_width)));
        if self.bindings.is_empty() && self.commands.is_empty() {
            lines.push(line(dim("  no bindings yet; n adds one")));
        }
        match self.selected() {
            Some(r) if width >= 90 => columns(lines, left_width, self.detail(r, width - left_width - 1), height),
            _ => lines,
        }
    }

    fn help(&self) -> String {
        if let Some(help) = Confirm::help(self.ask.as_ref()) {
            return help;
        }
        if self.form.is_some() {
            return "tab next field · ↵ save · esc cancel".into();
        }
        "↵ change or bind · n new · d unbind · o open keys.json · / search · ? keys".into()
    }

    fn capturing(&self) -> bool {
        self.form.is_some() || self.ask.is_some() || self.list.filtering
    }

    fn back(&mut self) -> bool {
        self.list.clear_filter()
    }

    fn sheet(&self) -> Vec<Hint> {
        vec![
            ("↑/↓ /", "move, search"),
            ("↵", "change a binding, or bind an unbound command"),
            ("n", "a new binding"),
            ("d", "unbind"),
            ("o", "open keys.json in your editor"),
            ("r", "reload"),
            ("", "Marks"),
            ("✗", "the app cannot use it: a bad key or an unknown command"),
            ("◆", "another section binds the same key; the more specific one wins"),
        ]
    }
}
