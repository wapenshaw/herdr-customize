//! Keys: Herdr's actions with their bindings, custom commands, and plugin
//! actions that have no key yet, with conflicts and rule problems marked.

use std::collections::HashSet;

use anyhow::{Result, anyhow, bail};
use ratatui::text::{Line, Span};

use super::widgets::*;
use super::{Ctx, Key, Msg, Page, Shared, load_doc};
use crate::config::{self, Field, Value};
use crate::keys::{self, Binding};
use crate::plugins::Plugin;
use crate::schema;

const COMMAND_PATH: [&str; 2] = ["keys", "command"];

fn command_path() -> Vec<String> {
    COMMAND_PATH.iter().map(ToString::to_string).collect()
}

/// The [[keys.command]] kinds Herdr accepts.
#[derive(Clone, Copy, Default, PartialEq)]
enum CommandType {
    #[default]
    PluginAction,
    Popup,
    Pane,
    Shell,
}

impl CommandType {
    const ALL: [CommandType; 4] = [CommandType::PluginAction, CommandType::Popup, CommandType::Pane, CommandType::Shell];

    fn name(self) -> &'static str {
        match self {
            CommandType::PluginAction => "plugin_action",
            CommandType::Popup => "popup",
            CommandType::Pane => "pane",
            CommandType::Shell => "shell",
        }
    }

    /// The type n places to the right, wrapping around.
    fn next(self, n: usize) -> CommandType {
        CommandType::ALL[(self as usize + n) % CommandType::ALL.len()]
    }
}

/// The command form's text fields, in screen order.
#[derive(Clone, Copy, PartialEq)]
enum FormField {
    Key,
    Command,
    Description,
    Width,
    Height,
}

impl FormField {
    const ALL: [FormField; 5] = [FormField::Key, FormField::Command, FormField::Description, FormField::Width, FormField::Height];

    fn name(self) -> &'static str {
        match self {
            FormField::Key => "key",
            FormField::Command => "command",
            FormField::Description => "description",
            FormField::Width => "width",
            FormField::Height => "height",
        }
    }
}

#[derive(Debug, Clone)]
enum Kind {
    Header,
    /// A Herdr action under [keys], with whether config.toml sets it.
    Builtin {
        key: schema::Key,
        action: String,
        changed: bool,
    },
    /// A [[keys.command]] entry; missing marks a plugin_action that is not
    /// installed.
    Command {
        index: usize,
        entry: toml::Table,
        missing: bool,
    },
    /// A plugin action without a key.
    PluginAction {
        qualified: String,
        title: String,
    },
}

#[derive(Debug, Clone)]
struct KeyRow {
    kind: Kind,
    label: String,
    values: Vec<String>,
    owner: String,
    clashes: Vec<String>,
    problems: Vec<String>,
}

impl KeyRow {
    fn new(kind: Kind, label: String) -> KeyRow {
        KeyRow { kind, label, values: Vec::new(), owner: String::new(), clashes: Vec::new(), problems: Vec::new() }
    }

    fn binding(&self) -> Option<Binding> {
        let navigate = match &self.kind {
            Kind::Builtin { action, .. } => keys::navigate(action),
            Kind::Command { .. } => false,
            _ => return None,
        };
        Some(Binding { owner: self.owner.clone(), keys: self.values.clone(), navigate })
    }
}

fn text(entry: &toml::Table, name: &str) -> String {
    entry.get(name).and_then(Value::as_str).unwrap_or_default().to_string()
}

#[derive(PartialEq)]
enum Mode {
    Browse,
    Editing,
    Form,
}

pub struct KeysPage {
    rows: Vec<KeyRow>,
    list: List,
    plugins: Option<Vec<Plugin>>,
    load_err: Option<String>,
    mode: Mode,
    ask: Option<Confirm<KeysPage>>,
    // binding editor
    input: TextInput,
    editing: Option<KeyRow>,
    recording: bool,
    form: CommandForm,
}

impl KeysPage {
    pub fn new() -> KeysPage {
        KeysPage {
            rows: Vec::new(),
            list: List::new(),
            plugins: None,
            load_err: None,
            mode: Mode::Browse,
            ask: None,
            input: TextInput::default(),
            editing: None,
            recording: false,
            form: CommandForm::default(),
        }
    }

    fn current(&self) -> Option<&KeyRow> {
        self.rows.get(self.list.selected()?).filter(|r| !matches!(r.kind, Kind::Header))
    }

    /// Reads the config and recomputes rows, conflicts and problems.
    fn rebuild(&mut self, sh: &Shared) {
        let doc = load_doc(&sh.store, &mut self.load_err);
        // Decoded once: a lookup per key would re-serialize and re-parse the
        // whole document each time.
        let table = doc.decoded();
        let mut rows = vec![KeyRow::new(Kind::Header, "Herdr actions".into())];
        for k in sh.reference.section("keys").map(|s| s.keys.as_slice()).unwrap_or_default() {
            let current = config::lookup(&table, &k.path()).cloned();
            let changed = current.is_some();
            let action = k.key.trim_start_matches("keys.").to_string();
            let values = keys::indexed(&action, keys::values(current.or_else(|| k.default_value()).as_ref()));
            let mut r = KeyRow::new(Kind::Builtin { key: k.clone(), action: action.clone(), changed }, format!("{action} {}", values.join(" ")));
            (r.values, r.owner) = (values, k.key.clone());
            rows.push(r);
        }
        let plugins = self.plugins.as_deref().unwrap_or_default();
        let installed: HashSet<String> = plugins.iter().flat_map(|p| p.actions.iter().map(|a| a.qualified(&p.id))).collect();
        let mut bound = HashSet::new();
        rows.push(KeyRow::new(Kind::Header, "Custom commands".into()));
        for (index, entry) in doc.array_table(&command_path()).into_iter().enumerate() {
            let (key, kind, command, desc) = (text(&entry, "key"), text(&entry, "type"), text(&entry, "command"), text(&entry, "description"));
            let mut missing = false;
            if kind == CommandType::PluginAction.name() {
                bound.insert(command.clone());
                missing = self.plugins.is_some() && !installed.contains(&command) && !installed_suffix(&installed, &command);
            }
            let label = [key.as_str(), &kind, &command, &desc].join(" ");
            let mut r = KeyRow::new(Kind::Command { index, entry, missing }, label);
            r.values = keys::values(Some(&Value::String(key)));
            r.owner = format!("command {} ({})", index + 1, first_non_empty(&[&desc, &command]));
            rows.push(r);
        }
        let mut unbound = Vec::new();
        for p in plugins.iter().filter(|p| p.enabled) {
            for a in &p.actions {
                let q = a.qualified(&p.id);
                if bound.contains(&q) || !a.available() {
                    continue;
                }
                let title = first_non_empty(&[&a.title, &a.id]).to_string();
                unbound.push(KeyRow::new(Kind::PluginAction { qualified: q.clone(), title: title.clone() }, format!("{q} {title}")));
            }
        }
        if !unbound.is_empty() {
            rows.push(KeyRow::new(Kind::Header, "Plugin actions without a key".into()));
            rows.extend(unbound);
        }
        self.rows = rows;
        self.annotate();
        let labels = self.rows.iter().map(|r| r.label.clone()).collect();
        let headers = self.rows.iter().map(|r| matches!(r.kind, Kind::Header)).collect();
        self.list.set_rows(labels, headers);
    }

    fn bindings(&self) -> Vec<Binding> {
        self.rows.iter().filter_map(KeyRow::binding).collect()
    }

    fn annotate(&mut self) {
        let conflicts = keys::conflicts(&self.bindings());
        for r in &mut self.rows {
            let Some(b) = r.binding() else { continue };
            r.clashes = keys::clashes(&conflicts, &b);
            r.problems.clear();
            let action = match &r.kind {
                Kind::Builtin { key, .. } if key.kind != schema::KEYBINDING => continue,
                Kind::Builtin { action, .. } => action.clone(),
                _ => "command".into(),
            };
            r.problems = r.values.iter().flat_map(|v| keys::problems(&action, v)).collect();
        }
    }

    fn bind_action(&mut self, qualified: &str, title: &str) {
        let mut entry = toml::Table::new();
        entry.insert("type".into(), Value::String("plugin_action".into()));
        entry.insert("command".into(), Value::String(qualified.into()));
        entry.insert("description".into(), Value::String(title.into()));
        self.form = CommandForm::new(entry, None);
        self.mode = Mode::Form;
    }

    fn edit_key(&mut self, key: &Key, ctx: &mut Ctx) {
        if self.recording {
            if key.is("esc") {
                self.recording = false;
            } else if let Some(k) = record_key(key) {
                self.input.set_value(&k);
                self.recording = false;
            }
            return;
        }
        match key.name.as_str() {
            "esc" => (self.mode, self.editing) = (Mode::Browse, None),
            "ctrl+r" => self.recording = true,
            "enter" => {
                self.mode = Mode::Browse;
                let Some(KeyRow { kind: Kind::Builtin { key, action, .. }, .. }) = self.editing.take() else { return };
                let values = split_values(&self.input.value());
                let path = key.path();
                if values.is_empty() {
                    return ctx.save(format!("{action} default"), key.key, move |doc| doc.unset(&path).map(|_| ()));
                }
                let value = match values.as_slice() {
                    [one] => Value::String(one.clone()),
                    _ => Value::Array(values.into_iter().map(Value::String).collect()),
                };
                ctx.save(action, key.key, move |doc| doc.set(&path, &value));
            }
            _ => {
                self.input.handle(key);
            }
        }
    }

    /// The other owners bound to any of me's keys, ignoring except's own
    /// current binding.
    fn clashes(&self, except: Option<&str>, me: &Binding) -> Vec<String> {
        let mut all: Vec<Binding> = self.bindings().into_iter().filter(|b| Some(b.owner.as_str()) != except).collect();
        all.push(me.clone());
        keys::clashes(&keys::conflicts(&all), me)
    }

    /// Problems and clashes for the value being typed.
    fn edit_hints(&self) -> Vec<String> {
        let Some(r) = &self.editing else { return Vec::new() };
        let Kind::Builtin { key, action, .. } = &r.kind else { return Vec::new() };
        let values = keys::indexed(action, split_values(&self.input.value()));
        let mut hints = Vec::new();
        if key.kind == schema::KEYBINDING {
            hints.extend(values.iter().flat_map(|v| keys::problems(action, v)));
        }
        let clashes = self.clashes(Some(&r.owner), &Binding { owner: r.owner.clone(), keys: values.clone(), navigate: keys::navigate(action) });
        if !clashes.is_empty() {
            hints.push(format!("also used by {}", clashes.join(", ")));
        }
        if values.is_empty() {
            hints.push(format!("empty: reset to the default ({})", key.default));
        }
        hints
    }

    fn form_key(&mut self, key: &Key, ctx: &mut Ctx) {
        let f = &mut self.form;
        let n = f.fields.len();
        match key.name.as_str() {
            "esc" => self.mode = Mode::Browse,
            "tab" | "down" => f.focus_on((f.focus + 1) % n),
            "shift+tab" | "up" => f.focus_on((f.focus + n - 1) % n),
            "ctrl+right" | "ctrl+n" => f.kind = f.kind.next(1),
            "ctrl+left" | "ctrl+p" => f.kind = f.kind.next(CommandType::ALL.len() - 1),
            "enter" => match f.entry() {
                Err(e) => ctx.error(e),
                Ok(fields) => {
                    self.mode = Mode::Browse;
                    match f.index {
                        None => ctx.save("new command", COMMAND_PATH.join("."), move |doc| doc.append_array_table(&command_path(), &fields)),
                        Some(i) => ctx.save("command", COMMAND_PATH.join("."), move |doc| doc.update_array_table(&command_path(), i, &fields)),
                    }
                }
            },
            _ => {
                f.fields[f.focus].handle(key);
            }
        }
    }

    fn marks(r: &KeyRow) -> Vec<Span<'static>> {
        let mut marks = Vec::new();
        if !r.clashes.is_empty() {
            marks.extend([raw(" "), warn("⚠ conflict")]);
        }
        if let Some(p) = r.problems.first() {
            marks.extend([raw(" "), error(format!("✗ {p}"))]);
        }
        if matches!(r.kind, Kind::Command { missing: true, .. }) {
            marks.extend([raw(" "), error("✗ action not installed")]);
        }
        marks
    }

    fn draw_row(&self, i: usize, selected: bool) -> Line<'static> {
        let r = &self.rows[i];
        let mut spans = match &r.kind {
            Kind::Header => return line(heading(r.label.clone())),
            Kind::Builtin { action, changed, .. } => {
                let values = pad(&r.values.join(", "), 24);
                vec![raw(format!("{action:<28} ")), if *changed { accent(values) } else { raw(values) }]
            }
            Kind::Command { entry, .. } => {
                let mut spans = vec![raw(format!("{:<18} {:<14} {}", text(entry, "key"), text(entry, "type"), text(entry, "command")))];
                let desc = text(entry, "description");
                if !desc.is_empty() {
                    spans.push(dim(format!("  {desc}")));
                }
                spans
            }
            Kind::PluginAction { qualified, title } => vec![raw(format!("{qualified:<40} ")), dim(title.clone())],
        };
        spans.extend(Self::marks(r));
        row(Line::from(spans), selected)
    }

    /// The selected row, or the binding being edited.
    fn detail(&self) -> Vec<Line<'static>> {
        if let Some(e) = &self.load_err {
            return vec![line(error(format!("config: {e}")))];
        }
        if self.mode == Mode::Editing {
            let mut lines = vec![self.input.view()];
            if self.recording {
                lines.push(line(accent("recording: press the key combination")));
            }
            lines.extend(self.edit_hints().into_iter().map(|h| line(warn(format!("  {h}")))));
            return lines;
        }
        let Some(r) = self.current() else { return Vec::new() };
        let mut lines = Vec::new();
        match &r.kind {
            Kind::Builtin { key, .. } => lines.push(line(dim(format!("{} Default: {}", key.description, key.default)))),
            Kind::Command { missing: true, .. } => lines.push(line(error("No installed plugin has this action. Edit it to point at an installed one, or delete it."))),
            Kind::PluginAction { .. } => lines.push(line(dim("Not bound to any key. Press enter to add a binding."))),
            _ => {}
        }
        if !r.clashes.is_empty() {
            lines.push(line(warn(format!("Same key as: {}", r.clashes.join(", ")))));
        }
        lines.extend(r.problems.iter().map(|p| line(error(p.clone()))));
        lines
    }

    fn command_owner(&self, index: usize) -> String {
        self.rows.iter().find(|r| matches!(r.kind, Kind::Command { index: i, .. } if i == index)).map(|r| r.owner.clone()).unwrap_or_default()
    }

    fn form_view(&self) -> Vec<Line<'static>> {
        let f = &self.form;
        let title = f.index.map_or_else(|| "New custom command".into(), |i| format!("Edit custom command {}", i + 1));
        let mut lines = vec![line(heading(title)), Line::default()];
        let mut types = vec![raw(format!("{:<12}", "type"))];
        for t in CommandType::ALL {
            let label = format!(" {} ", t.name());
            types.push(if t == f.kind { styled(TAB_ACTIVE, label) } else { dim(label) });
            types.push(raw(" "));
        }
        types.push(dim("  ctrl+←/→ change"));
        lines.push(Line::from(types));
        let popup = f.kind == CommandType::Popup;
        for (field, input) in FormField::ALL.into_iter().zip(&f.fields) {
            if !popup && matches!(field, FormField::Width | FormField::Height) {
                continue;
            }
            lines.push(input.view());
        }
        lines.push(Line::default());
        match f.kind {
            CommandType::PluginAction => {
                lines.push(line(dim("command is a plugin action id such as herdr-customize.open")));
                let q = f.value(FormField::Command);
                if let (false, Some(plugins)) = (q.is_empty(), &self.plugins)
                    && !plugins.iter().any(|p| p.actions.iter().any(|a| a.qualified(&p.id) == q))
                {
                    lines.push(line(warn(format!("no installed plugin has the action {q}"))));
                }
            }
            CommandType::Popup => lines.push(line(dim("opens a popup running command; width/height are cells or a percentage"))),
            CommandType::Pane => lines.push(line(dim("opens a temporary zoomed pane running command"))),
            CommandType::Shell => lines.push(line(dim("runs command detached in the background"))),
        }
        let key = f.value(FormField::Key);
        if !key.is_empty() {
            lines.extend(keys::problems("command", &key).into_iter().map(|p| line(warn(p))));
            let mine = f.index.map(|i| self.command_owner(i));
            let clashes = self.clashes(mine.as_deref(), &Binding { owner: "editing".into(), keys: vec![key], navigate: false });
            if !clashes.is_empty() {
                lines.push(line(warn(format!("also used by {}", clashes.join(", ")))));
            }
        }
        lines
    }
}

/// An unqualified action id that matches exactly one installed action, which
/// Herdr resolves too.
fn installed_suffix(installed: &HashSet<String>, command: &str) -> bool {
    !command.contains('.') && installed.iter().filter(|q| q.ends_with(&format!(".{command}"))).count() == 1
}

fn split_values(text: &str) -> Vec<String> {
    text.split(',').map(str::trim).filter(|v| !v.is_empty()).map(String::from).collect()
}

/// A key press in Herdr notation. A plain printable key becomes a prefix
/// binding, since a direct one would intercept typing.
fn record_key(key: &Key) -> Option<String> {
    if key.paste.is_some() {
        return None;
    }
    let name = match key.name.as_str() {
        " " => "space".to_string(),
        n => n.replace("pgup", "pageup").replace("pgdown", "pagedown"),
    };
    let mut c = keys::parse(&name).ok()?;
    if c.plain() {
        c.prefix = true;
    }
    Some(c.to_string())
}

/// Edits one [[keys.command]] entry.
#[derive(Default)]
struct CommandForm {
    /// None for a new entry.
    index: Option<usize>,
    kind: CommandType,
    fields: Vec<TextInput>,
    focus: usize,
    original: toml::Table,
}

impl CommandForm {
    fn new(entry: toml::Table, index: Option<usize>) -> CommandForm {
        let kind = CommandType::ALL.into_iter().find(|t| t.name() == text(&entry, "type")).unwrap_or_default();
        let fields = FormField::ALL
            .iter()
            .map(|field| {
                let name = field.name();
                let value = match entry.get(name) {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Integer(n)) => n.to_string(),
                    _ => String::new(),
                };
                let mut input = TextInput::new(format!("{name:<12}"), &value);
                input.blur();
                input
            })
            .collect();
        let mut f = CommandForm { index, kind, fields, focus: 0, original: entry };
        f.focus_on(0);
        f
    }

    fn focus_on(&mut self, i: usize) {
        self.fields[self.focus].blur();
        self.focus = i;
        self.fields[i].focus();
    }

    fn value(&self, field: FormField) -> String {
        self.fields[field as usize].value().trim().to_string()
    }

    fn entry(&self) -> Result<Vec<Field>> {
        let key = self.value(FormField::Key);
        if key.is_empty() {
            bail!("a key is required");
        }
        if let Some(p) = keys::problems("command", &key).into_iter().next() {
            bail!(p);
        }
        let command = self.value(FormField::Command);
        if command.is_empty() {
            bail!("a command is required");
        }
        let s = |v: &str| Value::String(v.into());
        let mut fields: Vec<Field> = vec![("key".into(), s(&key)), ("type".into(), s(self.kind.name())), ("command".into(), s(&command))];
        let desc = self.value(FormField::Description);
        if !desc.is_empty() {
            fields.push(("description".into(), s(&desc)));
        }
        for field in [FormField::Width, FormField::Height] {
            let (name, value) = (field.name(), self.value(field));
            if value.is_empty() || self.kind != CommandType::Popup {
                continue;
            }
            if let Ok(n) = value.parse::<i64>() {
                fields.push((name.into(), Value::Integer(n)));
            } else if value.ends_with('%') {
                fields.push((name.into(), s(&value)));
            } else {
                return Err(anyhow!("{name} must be a number of cells or a percentage"));
            }
        }
        // Keep keys this form does not know about.
        for (k, v) in &self.original {
            if k != "type" && !FormField::ALL.iter().any(|f| f.name() == k) {
                fields.push((k.clone(), v.clone()));
            }
        }
        Ok(fields)
    }
}

impl Page for KeysPage {
    fn title(&self) -> &'static str {
        "Keys"
    }

    fn init(&mut self, ctx: &mut Ctx) {
        self.rebuild(ctx.sh);
    }

    fn update(&mut self, msg: &mut Msg, ctx: &mut Ctx) {
        match msg {
            Msg::PluginList(list) => {
                if let Ok(list) = list {
                    self.plugins = Some(list.clone());
                }
                self.rebuild(ctx.sh);
            }
            Msg::Reference(_) | Msg::Saved(_) => self.rebuild(ctx.sh),
            Msg::BindAction { qualified, title } => self.bind_action(qualified, title),
            _ => {}
        }
    }

    fn key(&mut self, key: &Key, ctx: &mut Ctx) {
        if Confirm::handle(self.ask.take(), self, key, ctx) {
            return;
        }
        match self.mode {
            Mode::Editing => return self.edit_key(key, ctx),
            Mode::Form => return self.form_key(key, ctx),
            Mode::Browse => {}
        }
        if self.list.handle(key) {
            return;
        }
        match key.name.as_str() {
            "n" => {
                let mut entry = toml::Table::new();
                entry.insert("type".into(), Value::String("popup".into()));
                self.form = CommandForm::new(entry, None);
                self.mode = Mode::Form;
                return;
            }
            "r" => {
                self.rebuild(ctx.sh);
                ctx.spawn(super::load_plugins);
                return;
            }
            _ => {}
        }
        let Some(r) = self.current().cloned() else { return };
        match (&r.kind, key.name.as_str()) {
            (Kind::Builtin { action, .. }, "enter") => {
                let mut value = r.values.join(", ");
                if action.starts_with("indexed.") {
                    value = value.replace("+1..9", "");
                }
                self.input = TextInput::new(format!("{action} = "), &value);
                (self.mode, self.recording) = (Mode::Editing, false);
                self.editing = Some(r);
            }
            (Kind::Builtin { action, changed: false, .. }, "x") => ctx.status(format!("{action} already uses the default")),
            (Kind::Builtin { key, action, .. }, "x") => {
                let path = key.path();
                ctx.save(format!("{action} default"), key.key.clone(), move |doc| doc.unset(&path).map(|_| ()));
            }
            (Kind::Command { index, entry, .. }, "enter") => {
                self.form = CommandForm::new(entry.clone(), Some(*index));
                self.mode = Mode::Form;
            }
            (Kind::Command { index, .. }, "d") => {
                let index = *index;
                self.ask = Some(Confirm::new(format!("Delete {}?", r.owner), move |_: &mut KeysPage, ctx| {
                    ctx.save("command removal", COMMAND_PATH.join("."), move |doc| doc.delete_array_table(&command_path(), index));
                }));
            }
            (Kind::PluginAction { qualified, title }, "enter") => self.bind_action(qualified, title),
            _ => {}
        }
    }

    fn view(&mut self, _sh: &Shared, width: usize, height: usize) -> Vec<Line<'static>> {
        if self.mode == Mode::Form {
            return self.form_view();
        }
        let detail = self.detail();
        let list_height = height.saturating_sub(detail.len() + 1).max(3);
        let conflicts = self.rows.iter().filter(|r| !r.clashes.is_empty()).count();
        let rows = self.list.window(list_height);
        let body = self.list.render(rows, |i, s| self.draw_row(i, s));
        let mut lines =
            vec![if conflicts > 0 && self.mode == Mode::Browse && self.ask.is_none() { line(warn(format!("{conflicts} bindings share a key with another binding (⚠)"))) } else { Line::default() }];
        lines.extend(body);
        let mut lines = fit(lines, width, list_height + 1);
        lines.extend(detail);
        lines
    }

    fn help(&self) -> String {
        if let Some(help) = Confirm::help(self.ask.as_ref()) {
            return help;
        }
        match self.mode {
            Mode::Editing if self.recording => return "press the key to record · esc stop recording".into(),
            Mode::Editing => return "↵ save · ctrl+r record a key · empty resets to default · esc cancel".into(),
            Mode::Form => return "tab/shift+tab field · ctrl+←/→ change type · ↵ save · esc cancel".into(),
            Mode::Browse => {}
        }
        match self.current().map(|r| &r.kind) {
            None | Some(Kind::Header) => "/ search · n new command",
            Some(Kind::Builtin { .. }) => "↵ change · x reset to default · / search · n new command",
            Some(Kind::Command { .. }) => "↵ edit · d delete · / search · n new command",
            Some(Kind::PluginAction { .. }) => "↵ bind a key · / search · n new command",
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
