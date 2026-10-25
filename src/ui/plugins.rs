//! Plugins: the installed list, and a panel with everything the selected one
//! can do (→ or ↵ enters it, ← or esc leaves). The marketplace, recent runs
//! and a plugin's settings are screens of their own.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::SystemTime;

use ratatui::style::Color;
use ratatui::text::{Line, Span};
use regex::Regex;

use super::pluginconfig::{self, PluginConfig};
use super::widgets::{self, *};
use super::{self as ui, Ctx, Hint, Key, Page, Shared, load_plugins};
use crate::fsutil;
use crate::pluginconfig::Config;
use crate::plugins::{self, Log, MARKET_TOPIC, Plugin, Repo, Results, State, Status};
use crate::proc;

/// What this page's background work reports, its settings screen's included.
pub enum Msg {
    Checks(HashMap<String, Status>),
    Market { query: String, sort: String, page: usize, results: anyhow::Result<Results> },
    Op { text: String, err: Option<anyhow::Error> },
    Pin { plugin: Box<Plugin>, sha: anyhow::Result<String> },
    Logs { id: String, logs: anyhow::Result<Vec<Log>> },
    RegistryEdited(anyhow::Result<()>),
    Detail { id: String, settings: String, last: String },
    Config { id: String, cfg: anyhow::Result<Config> },
    ConfigSaved { id: String, what: String, err: Option<anyhow::Error> },
    ConfigReload(String),
}

impl From<Msg> for ui::Msg {
    fn from(msg: Msg) -> ui::Msg {
        ui::Msg::Plugins(msg)
    }
}

/// How many results each marketplace request fetches.
const MARKET_PAGE: usize = 50;

#[derive(PartialEq)]
enum Screen {
    Installed,
    Market,
    Logs,
    Config,
}

#[derive(PartialEq)]
enum Mode {
    Browse,
    InstallInput,
    MarketQuery,
}

#[derive(Clone, Copy, PartialEq)]
enum Row {
    Plugin(usize),
    Gap,
    Install,
    Market,
}

/// What the panel loads lazily for a plugin.
#[derive(Clone)]
struct Detail {
    settings: String,
    last: String,
}

/// What a panel menu line does.
#[derive(Clone)]
enum Action {
    Invoke(String),
    Settings,
    Logs,
    Update,
    Toggle,
    Remove,
    Open,
}

struct MenuItem {
    icon: &'static str,
    label: String,
    info: String,
    key: &'static str,
    action: Action,
}

pub struct PluginsPage {
    screen: Screen,
    mode: Mode,
    ask: Option<Confirm<PluginsPage>>,
    input: TextInput,
    plugins: Vec<Plugin>,
    load_err: Option<String>,
    loaded: bool,
    rows: Vec<Row>,
    list: List,
    statuses: HashMap<String, Status>,
    checking: bool,
    /// Update checks and details wait until the page is first shown.
    shown: bool,
    have_git: bool,
    // marketplace
    market: List,
    repos: Option<Vec<Repo>>,
    market_total: usize,
    market_query: String,
    market_sort: String,
    market_busy: bool,
    market_err: Option<String>,
    // logs
    logs: Vec<Log>,
    logs_list: List,
    logs_for: String,
    logs_err: Option<String>,
    /// The settings screen of one plugin.
    config: Option<PluginConfig>,
    /// True while keys go to the selected plugin's menu.
    panel: bool,
    menu_cursor: usize,
    details: HashMap<String, Detail>,
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

/// A short relative time.
fn ago(t: SystemTime) -> String {
    let secs = SystemTime::now().duration_since(t).map_or(0, |d| d.as_secs());
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{}m ago", secs / 60),
        3600..172_800 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86400),
    }
}

/// Runs herdr on the terminal; install and update show Herdr's trust
/// preview there.
fn interactive(ctx: &mut Ctx, what: String, args: Vec<String>) {
    match plugins::interactive(&args) {
        Ok(cmd) => ctx.exec(cmd, move |ok| match ok {
            Ok(()) => Msg::Op { text: format!("{what} done"), err: None }.into(),
            Err(_) => Msg::Op { text: what, err: Some(anyhow::anyhow!("cancelled or failed")) }.into(),
        }),
        Err(e) => ctx.error(e),
    }
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// What herdr plugin install takes, plus an optional @ref.
static SPEC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+(/[A-Za-z0-9_./-]+)?(@[^\s@]+)?$").unwrap());

fn install_args(spec: &str) -> anyhow::Result<Vec<String>> {
    let spec = spec.trim();
    let spec = spec.strip_prefix("https://github.com/").unwrap_or(spec).trim();
    if !SPEC.is_match(spec) {
        anyhow::bail!("{spec:?} is not owner/repo[/subdir][@ref]");
    }
    Ok(match spec.split_once('@') {
        Some((name, r)) => args(&["plugin", "install", name, "--ref", r]),
        None => args(&["plugin", "install", spec]),
    })
}

impl PluginsPage {
    pub fn new() -> PluginsPage {
        PluginsPage {
            screen: Screen::Installed,
            mode: Mode::Browse,
            ask: None,
            input: TextInput::default(),
            plugins: Vec::new(),
            load_err: None,
            loaded: false,
            rows: Vec::new(),
            list: List::new(),
            statuses: HashMap::new(),
            checking: false,
            shown: false,
            have_git: plugins::have_git(),
            market: List::new(),
            repos: None,
            market_total: 0,
            market_query: String::new(),
            market_sort: "stars".into(),
            market_busy: false,
            market_err: None,
            logs: Vec::new(),
            logs_list: List::new(),
            logs_for: String::new(),
            logs_err: None,
            config: None,
            panel: false,
            menu_cursor: 0,
            details: HashMap::new(),
        }
    }

    /// The plugin on the list's row, if it is one.
    fn selected(&self) -> Option<&Plugin> {
        match self.selected_row()? {
            Row::Plugin(i) => self.plugins.get(i),
            _ => None,
        }
    }

    fn selected_row(&self) -> Option<Row> {
        self.rows.get(self.list.selected()?).copied()
    }

    fn rebuild(&mut self) {
        let (mut rows, mut labels): (Vec<Row>, Vec<String>) = self.plugins.iter().enumerate().map(|(i, p)| (Row::Plugin(i), format!("{} {} {}", p.id, p.name, p.description))).unzip();
        if self.load_err.is_none() {
            rows.extend([Row::Gap, Row::Install, Row::Market]);
            labels.extend(["".into(), "install from github".into(), "marketplace browse".into()]);
        }
        let headers = rows.iter().map(|r| *r == Row::Gap).collect();
        self.rows = rows;
        self.list.set_rows(labels, headers);
        self.list.skip_headers(1);
    }

    /// Fetches the panel's settings and last-run lines for the selected
    /// plugin once.
    fn load_detail(&mut self, ctx: &mut Ctx) {
        let Some(p) = self.selected() else { return };
        if self.details.contains_key(&p.id) {
            return;
        }
        let (id, root) = (p.id.clone(), p.root.clone());
        self.details.insert(id.clone(), Detail { settings: "…".into(), last: "…".into() });
        ctx.spawn(move || {
            let settings = match plugins::config_dir(&id) {
                Err(_) => "unavailable".into(),
                Ok(dir) => match crate::pluginconfig::load(&dir, std::path::Path::new(&root)) {
                    Ok(cfg) => pluginconfig::summary(&cfg),
                    Err(e) => format!("unreadable: {e:#}"),
                },
            };
            let last = match plugins::logs(&id, 1).ok().and_then(|l| l.into_iter().next()) {
                Some(l) => format!("{} {} {}", l.trigger(), if l.status == "succeeded" { "✓" } else { "✗" }, ago(l.started_at())),
                None => "none yet".into(),
            };
            Msg::Detail { id, settings, last }.into()
        });
    }

    fn check_updates(&mut self, ctx: &mut Ctx) {
        if !self.have_git {
            return;
        }
        self.checking = true;
        let list = self.plugins.clone();
        ctx.spawn(move || Msg::Checks(plugins::check_all(&list)).into());
    }

    fn update_info(&self, p: &Plugin) -> String {
        let status = self.statuses.get(&p.id).cloned().unwrap_or_default();
        match () {
            _ if !self.have_git => "install git to check".into(),
            _ if self.checking => "checking…".into(),
            _ if p.pinned() => format!("pinned to {}", short(p.git_ref())),
            _ if status.state == State::Behind && !status.version.is_empty() && status.version != p.version => format!("{} → {}", p.version, status.version),
            _ if status.state == State::Behind => format!("→ {}", short(&status.remote)),
            _ if status.state == State::Current => "up to date".into(),
            _ if status.state == State::Failed => "could not check".into(),
            _ => String::new(),
        }
    }

    /// What the panel offers for p.
    fn menu(&self, p: &Plugin) -> Vec<MenuItem> {
        let item = |icon, label: &str, info: String, key, action| MenuItem { icon, label: label.into(), info, key, action };
        let mut items: Vec<MenuItem> =
            p.actions.iter().filter(|a| a.available()).map(|a| item("▶", first_non_empty(&[&a.title, &a.id]), a.qualified(&p.id), "", Action::Invoke(a.qualified(&p.id)))).collect();
        let detail = self.details.get(&p.id);
        items.push(item("⚙", "Settings", detail.map(|d| d.settings.clone()).unwrap_or_default(), "s", Action::Settings));
        items.push(item("≡", "Recent runs", detail.map(|d| d.last.clone()).unwrap_or_default(), "l", Action::Logs));
        if p.github() {
            items.push(item("↑", "Update", self.update_info(p), "u", Action::Update));
        }
        items.push(item("⏻", if p.enabled { "Disable" } else { "Enable" }, String::new(), "e", Action::Toggle));
        items.push(item("✕", if p.github() { "Uninstall" } else { "Unlink" }, String::new(), "x", Action::Remove));
        if p.github() {
            items.push(item("↗", "Open repository", format!("github.com/{}", p.slug()), "o", Action::Open));
        }
        items
    }

    fn run(&mut self, p: Plugin, action: Action, ctx: &mut Ctx) {
        match action {
            Action::Invoke(q) => ctx.quit_then(move || plugins::invoke_after_close(&q)),
            Action::Settings => self.open_config(p, ctx),
            Action::Logs => self.open_logs(p, ctx),
            Action::Update => self.update_one(p, ctx),
            Action::Toggle => {
                let verb = if p.enabled { "disabled" } else { "enabled" };
                ctx.spawn(move || Msg::Op { text: format!("{verb} {}", p.name), err: plugins::set_enabled(&p.id, !p.enabled).err() }.into());
            }
            Action::Remove => {
                let verb = if p.github() { "uninstall" } else { "unlink" };
                self.ask = Some(Confirm::new(format!("{verb} {} ({})?", p.name, p.id), move |page: &mut PluginsPage, ctx| {
                    page.panel = false;
                    ctx.spawn(move || Msg::Op { text: format!("{verb}ed {}", p.name), err: plugins::remove(&p).err() }.into());
                }));
            }
            Action::Open if !p.github() => ctx.status(format!("{} is linked from a local checkout", p.name)),
            Action::Open => match proc::open_url(&p.repo_url()) {
                Ok(()) => ctx.status(format!("opened {}", p.repo_url())),
                Err(e) => ctx.error(e),
            },
        }
    }

    fn update_one(&mut self, p: Plugin, ctx: &mut Ctx) {
        if !p.github() {
            return ctx.status(format!("{} is linked from a local checkout; update it there", p.name));
        }
        if p.pinned() {
            return ctx.spawn(move || {
                let sha = plugins::resolve_remote(&p.slug(), "");
                Msg::Pin { plugin: Box::new(p), sha }.into()
            });
        }
        if self.statuses.get(&p.id).is_some_and(|s| s.state == State::Current) {
            return ctx.status(format!("{} is up to date", p.name));
        }
        interactive(ctx, format!("update {}", p.name), args(&["plugin", "update", &p.id]));
    }

    /// Offers to move an exact-commit pin to the default branch.
    fn confirm_pin_move(&mut self, p: Plugin, sha: String, ctx: &mut Ctx) {
        if sha.eq_ignore_ascii_case(&p.source.commit) {
            return ctx.status(format!("{} is pinned to the latest commit", p.name));
        }
        let question = format!("{} is pinned to {}; move the pin to {} (default branch)?", p.name, short(&p.source.commit), short(&sha));
        self.ask = Some(Confirm::new(question, move |_: &mut PluginsPage, ctx| interactive(ctx, format!("update {}", p.name), args(&["plugin", "install", &p.spec(), "--ref", &sha]))));
    }

    /// Updates every plugin that is behind. Exact-commit pins are left alone:
    /// moving one is a decision per plugin.
    fn update_all(&mut self, ctx: &mut Ctx) {
        if !self.have_git {
            return ctx.error("install git to check for updates");
        }
        if self.checking {
            return ctx.status("still checking for updates");
        }
        let (mut ids, mut names) = (Vec::new(), Vec::new());
        for p in &self.plugins {
            let Some(status) = self.statuses.get(&p.id) else { continue };
            if !p.github() || p.pinned() || status.state != State::Behind {
                continue;
            }
            ids.push(p.id.clone());
            names.push(if status.version.is_empty() { p.name.clone() } else { format!("{} → {}", p.name, status.version) });
        }
        if ids.is_empty() {
            return ctx.status("nothing to update");
        }
        self.ask = Some(Confirm::new(format!("Update {} plugin(s): {}?", ids.len(), names.join(", ")), move |_: &mut PluginsPage, ctx| {
            let n = ids.len();
            let mut a = args(&["plugin", "update"]);
            a.extend(ids);
            interactive(ctx, format!("update of {n} plugin(s)"), a);
        }));
    }

    fn input_key(&mut self, key: &Key, ctx: &mut Ctx) {
        match key.name.as_str() {
            "esc" => self.mode = Mode::Browse,
            "enter" => {
                let text = self.input.value().trim().to_string();
                let mode = std::mem::replace(&mut self.mode, Mode::Browse);
                if mode == Mode::MarketQuery {
                    self.market_query = text;
                    return self.search(1, ctx);
                }
                if text.is_empty() {
                    return;
                }
                match install_args(&text) {
                    Ok(a) => interactive(ctx, format!("install {}", a[2]), a),
                    Err(e) => ctx.error(e),
                }
            }
            _ => {
                self.input.handle(key);
            }
        }
    }

    fn edit_registry(&mut self, ctx: &mut Ctx) {
        let path = plugins::registry_path();
        if !path.exists() {
            return ctx.error(format!("{} not found", path.display()));
        }
        ctx.edit_file(path, |text| Msg::RegistryEdited(text.map(drop)).into());
    }

    /// Fetches one page of marketplace results; page 1 starts over.
    fn search(&mut self, page: usize, ctx: &mut Ctx) {
        if page == 1 {
            (self.repos, self.market_total, self.market_err) = (None, 0, None);
            self.market.set_rows(Vec::new(), Vec::new());
        }
        self.market_busy = true;
        let (query, sort) = (self.market_query.clone(), self.market_sort.clone());
        ctx.spawn(move || {
            let results = plugins::search(&query, &sort, page, MARKET_PAGE);
            Msg::Market { query, sort, page, results }.into()
        });
    }

    fn installed(&self, full_name: &str) -> bool {
        self.plugins.iter().any(|p| p.github() && p.slug().eq_ignore_ascii_case(full_name))
    }

    fn market_key(&mut self, key: &Key, ctx: &mut Ctx) {
        if self.market.handle(key) {
            // Fetch the next page as the cursor nears the end of what is loaded.
            let loaded = self.repos.as_ref().map_or(0, Vec::len);
            if self.market.query().is_empty() && !self.market_busy && self.market_err.is_none() && self.market.cursor + 5 >= loaded && loaded < self.market_total {
                self.search(loaded / MARKET_PAGE + 1, ctx);
            }
            return;
        }
        match key.name.as_str() {
            "m" => return self.screen = Screen::Installed,
            "/" => {
                self.mode = Mode::MarketQuery;
                self.input = TextInput::new("search the marketplace: ", &self.market_query);
                return;
            }
            "s" => {
                self.market_sort = if self.market_sort == "stars" { "updated" } else { "stars" }.into();
                return self.search(1, ctx);
            }
            "r" => return self.search(1, ctx),
            _ => {}
        }
        let Some(repo) = self.market.selected().and_then(|i| self.repos.as_ref()?.get(i)).cloned() else { return };
        match key.name.as_str() {
            "enter" | "i" => {
                if self.installed(&repo.full_name) {
                    return ctx.status(format!("{} is already installed; update it from the list", repo.full_name));
                }
                self.ask = Some(Confirm::new(format!("Install {} (★ {})?", repo.full_name, repo.stars), move |_: &mut PluginsPage, ctx| {
                    interactive(ctx, format!("install {}", repo.full_name), args(&["plugin", "install", &repo.full_name]));
                }));
            }
            "o" => {
                let url = plugins::github_url(&repo.full_name);
                match proc::open_url(&url) {
                    Ok(()) => ctx.status(format!("opened {url}")),
                    Err(e) => ctx.error(e),
                }
            }
            _ => {}
        }
    }

    fn load_logs(&self, ctx: &mut Ctx) {
        let id = self.logs_for.clone();
        ctx.spawn(move || {
            let logs = plugins::logs(&id, 50);
            Msg::Logs { id, logs }.into()
        });
    }

    fn open_market(&mut self, ctx: &mut Ctx) {
        self.screen = Screen::Market;
        if self.repos.is_none() && !self.market_busy {
            self.search(1, ctx);
        }
    }

    fn start_install(&mut self) {
        self.mode = Mode::InstallInput;
        self.input = TextInput::new("install owner/repo[/subdir][@ref]: ", "");
    }

    fn open_config(&mut self, p: Plugin, ctx: &mut Ctx) {
        // The summary is read again on the way back.
        self.details.remove(&p.id);
        ctx.spawn(pluginconfig::load(&p));
        self.screen = Screen::Config;
        self.config = Some(PluginConfig::new(p));
    }

    fn open_logs(&mut self, p: Plugin, ctx: &mut Ctx) {
        (self.screen, self.logs_for, self.logs, self.logs_err) = (Screen::Logs, p.id, Vec::new(), None);
        self.logs_list.set_rows(Vec::new(), Vec::new());
        self.load_logs(ctx);
    }

    fn installed_key(&mut self, key: &Key, ctx: &mut Ctx) {
        if self.panel {
            if self.panel_key(key, ctx) {
                return;
            }
        } else if self.list.handle(key) {
            return self.load_detail(ctx);
        }
        match key.name.as_str() {
            "m" => return self.open_market(ctx),
            "i" => return self.start_install(),
            "c" => return self.edit_registry(ctx),
            "r" => {
                self.details.clear();
                ctx.spawn(load_plugins);
                return ctx.status("refreshing…");
            }
            "U" => return self.update_all(ctx),
            _ => {}
        }
        let Some(p) = self.selected().cloned() else {
            if matches!(key.name.as_str(), "enter" | "right" | "l") {
                match self.selected_row() {
                    Some(Row::Install) => self.start_install(),
                    Some(Row::Market) => self.open_market(ctx),
                    _ => {}
                }
            }
            return;
        };
        let action = match key.name.as_str() {
            "enter" | "right" => {
                (self.panel, self.menu_cursor) = (true, 0);
                return self.load_detail(ctx);
            }
            "s" => Action::Settings,
            "l" => Action::Logs,
            "u" => Action::Update,
            "e" => Action::Toggle,
            "x" => Action::Remove,
            "o" => Action::Open,
            _ => return,
        };
        self.run(p, action, ctx);
    }

    /// Moves in and runs the panel's menu. Keys it does not use fall through
    /// to the plugin's letter keys.
    fn panel_key(&mut self, key: &Key, ctx: &mut Ctx) -> bool {
        let Some(p) = self.selected().cloned() else {
            self.panel = false;
            return false;
        };
        let items = self.menu(&p);
        if let Some((cursor, _)) = step_cursor(self.menu_cursor, items.len(), key) {
            self.menu_cursor = cursor;
            return true;
        }
        match key.name.as_str() {
            "left" | "h" | "esc" => self.panel = false,
            "enter" | " " => {
                if let Some(item) = items.into_iter().nth(self.menu_cursor) {
                    self.run(p, item.action, ctx);
                }
            }
            "b" => match items.get(self.menu_cursor) {
                Some(MenuItem { action: Action::Invoke(q), label, .. }) => ctx.goto("keys", Some(ui::Msg::BindAction { qualified: q.clone(), title: label.clone() })),
                _ => ctx.status("select an action to bind it"),
            },
            _ => return false,
        }
        true
    }

    fn status_glyph(&self, p: &Plugin) -> Span<'static> {
        match self.statuses.get(&p.id).map(|s| s.state) {
            _ if !p.github() => dim("⌂"),
            Some(State::Current) => ok("●"),
            Some(State::Behind) => warn("↑"),
            Some(State::Failed) => error("?"),
            _ => dim("·"),
        }
    }

    /// The update state of every plugin, under the list.
    fn summary(&self) -> Line<'static> {
        if !self.have_git {
            return line(dim("install git to check for updates"));
        }
        if self.checking {
            return line(dim("checking for updates…"));
        }
        let count = |state| self.statuses.values().filter(|s| s.state == state).count();
        let (behind, failed) = (count(State::Behind), count(State::Failed));
        let mut spans = vec![if behind > 0 { warn(format!("↑ {behind} update(s) · U updates all")) } else { dim("all plugins up to date") }];
        if failed > 0 {
            spans.push(error(format!(" · {failed} not checked")));
        }
        Line::from(spans)
    }

    fn installed_view(&mut self, width: usize, height: usize, mut lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
        if !self.loaded {
            return vec![Line::raw("loading plugins…")];
        }
        if let Some(e) = &self.load_err {
            return vec![line(error(format!("herdr plugin list: {e}"))), line(dim("i installs a plugin by owner/repo · m opens the marketplace"))];
        }
        let narrow = width < 84;
        let left_width = if narrow { width } else { (width / 3).clamp(28, 36) };
        let list_height = height.saturating_sub(lines.len() + 2).max(3);
        let name_width = left_width.saturating_sub(14);
        let rows = self.list.window(list_height);
        let body = self.list.render(rows, |i, selected| {
            let p = match self.rows[i] {
                Row::Gap => return Line::default(),
                Row::Install => return row(line(accent("＋ Install from GitHub…")), selected),
                Row::Market => return row(line(accent("◆ Browse the marketplace…")), selected),
                Row::Plugin(i) => &self.plugins[i],
            };
            let name = pad(&clip_str(&p.name, name_width), name_width);
            let name = match () {
                _ if !p.enabled => dim(name),
                _ if selected && self.panel => heading(name),
                _ if selected => styled(SELECTED, name),
                _ => raw(name),
            };
            let version = if p.enabled { clip_str(&p.version, 9) } else { "off".into() };
            row(Line::from(vec![self.status_glyph(p), raw(" "), name, raw(" "), dim(version)]), selected)
        });
        lines.extend(fit(body, left_width, list_height));
        lines.push(Line::default());
        lines.push(self.summary());
        if narrow && !self.panel {
            return lines;
        }
        let panel = self.panel_view(width.saturating_sub(left_width + 1), height);
        if narrow {
            return panel;
        }
        columns(lines, left_width + 1, panel, height)
    }

    /// Describes the selected row; for a plugin, with its menu.
    fn panel_view(&self, width: usize, height: usize) -> Vec<Line<'static>> {
        let inner = width.saturating_sub(4).clamp(20, 72);
        let prose = |title: &str, text: &str, hint: &str| {
            let mut lines = vec![line(heading(title.to_string())), Line::default()];
            lines.extend(wrap(text, inner).into_iter().map(Line::raw));
            lines.extend([Line::default(), line(dim(hint.to_string()))]);
            lines
        };
        let lines = match self.selected_row() {
            Some(Row::Install) => prose(
                "Install from GitHub",
                "Type owner/repo, optionally with /subdir and @ref (a branch, tag or commit). Herdr shows what the plugin will run and asks before anything is installed.",
                "↵ start",
            ),
            Some(Row::Market) => {
                prose("Marketplace", &format!("GitHub repositories tagged {MARKET_TOPIC}, by stars or recent activity, searchable. Installing one goes through Herdr's trust prompt."), "↵ open")
            }
            Some(Row::Plugin(i)) => self.plugin_panel(&self.plugins[i], inner),
            _ => Vec::new(),
        };
        let n = lines.len().min(height.saturating_sub(2).max(3));
        boxed(fit(lines, inner, n), if self.panel { Color::Blue } else { Color::DarkGray }, Some(inner))
    }

    fn plugin_panel(&self, p: &Plugin, width: usize) -> Vec<Line<'static>> {
        let state = match () {
            _ if !p.enabled => "disabled".into(),
            _ if !p.github() => "linked".into(),
            _ => self.update_info(p),
        };
        let title = Line::from(vec![heading(p.name.clone()), dim(format!("  {}", p.version))]);
        let mut title = pad_line(title, width.saturating_sub(widgets::width(&state) + 2));
        title.spans.extend([self.status_glyph(p), raw(" "), dim(state)]);
        let source = if p.github() {
            let r = p.git_ref();
            if r.is_empty() { p.spec() } else { format!("{}@{r}", p.spec()) }
        } else {
            format!("linked from {}", p.root)
        };
        let mut lines = vec![clip(title, width), line(dim(clip_str(&source, width))), Line::default()];
        if !p.description.is_empty() {
            lines.extend(wrap(&p.description, width).into_iter().map(Line::raw));
            lines.push(Line::default());
        }
        if let Some(s) = self.statuses.get(&p.id).filter(|s| s.state == State::Failed && !s.error.is_empty()) {
            lines.extend([line(error(clip_str(&s.error, width))), Line::default()]);
        }
        let items = self.menu(p);
        let label_width = items.iter().map(|i| widgets::width(&i.label)).max().unwrap_or(0).min(width / 2);
        for (i, item) in items.iter().enumerate() {
            let selected = self.panel && i == self.menu_cursor;
            let label = pad(&clip_str(&item.label, label_width), label_width);
            let info = clip_str(&item.info, width.saturating_sub(label_width + 10));
            let text = Line::from(vec![dim(item.icon), raw(" "), if selected { styled(SELECTED, label) } else { raw(label) }, raw("  "), dim(info)]);
            let mut text = pad_line(text, width.saturating_sub(3));
            text.spans.push(accent(item.key));
            lines.push(row(text, selected));
        }
        lines
    }

    fn market_view(&mut self, width: usize, height: usize, mut lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
        let sort = if self.market_sort == "updated" { "recently updated" } else { "most stars" };
        let mut title = format!("Marketplace · topic:{MARKET_TOPIC} · {sort}");
        if !self.market_query.is_empty() {
            title += &format!(" · {:?}", self.market_query);
        }
        let loaded = self.repos.as_ref().map_or(0, Vec::len);
        if self.market_total > 0 {
            title += &format!(" · {loaded} of {} loaded", self.market_total);
        }
        lines.push(line(heading(title)));
        if let Some(e) = &self.market_err {
            lines.extend([line(error(e.clone())), line(dim("r retries"))]);
            return lines;
        }
        let repos = match &self.repos {
            None if self.market_busy => return [lines, vec![line(dim("searching GitHub…"))]].concat(),
            Some(r) if !r.is_empty() => r,
            _ => return [lines, vec![line(dim("no plugins found"))]].concat(),
        };
        let list_height = height.saturating_sub(lines.len() + 2).max(3);
        let rows = self.market.window(list_height);
        let body = self.market.render(rows, |i, selected| {
            let r = &repos[i];
            let mark = if self.installed(&r.full_name) { ok("✓ ") } else { raw("  ") };
            row(Line::from(vec![mark, raw(format!("★ {:<6} {:<36} ", r.stars, r.full_name)), dim(r.description.clone())]), selected)
        });
        lines.extend(fit(body, width, list_height));
        if self.market_busy {
            lines.push(line(dim("loading more…")));
        }
        lines
    }

    fn logs_view(&mut self, width: usize, height: usize, mut lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
        lines.push(line(heading(format!("Recent commands · {}", self.logs_for))));
        if let Some(e) = &self.logs_err {
            lines.push(line(error(e.clone())));
            return lines;
        }
        if self.logs.is_empty() {
            lines.push(line(dim("no commands recorded")));
            return lines;
        }
        let detail = self.log_detail(width);
        let list_height = height.saturating_sub(lines.len() + detail.len() + 1).max(3);
        let rows = self.logs_list.window(list_height);
        let body = self.logs_list.render(rows, |i, selected| {
            let l = &self.logs[i];
            let status = pad(&l.status, 9);
            let status = if l.status == "succeeded" { ok(status) } else { error(status) };
            row(Line::from(vec![dim(fsutil::format_local(l.started_at(), "%Y-%m-%d %H:%M:%S")), raw(" "), status, raw(format!(" {}", l.trigger()))]), selected)
        });
        lines.extend(fit(body, width, list_height));
        lines.push(Line::default());
        lines.extend(detail);
        lines
    }

    fn log_detail(&self, width: usize) -> Vec<Line<'static>> {
        let Some(l) = self.logs_list.selected().and_then(|i| self.logs.get(i)) else { return Vec::new() };
        let mut first = vec![dim(format!("$ {}", l.command.join(" ")))];
        if let Some(code) = l.exit_code {
            first.push(dim(format!("  exit {code} · {}ms", l.finished - l.started)));
        }
        let mut lines = vec![Line::from(first)];
        let mut tail = |label: &str, text: &str, style: fn(String) -> Span<'static>| {
            let text = text.trim();
            if text.is_empty() {
                return;
            }
            let out: Vec<&str> = text.lines().collect();
            lines.push(line(dim(label.to_string())));
            for l in &out[out.len().saturating_sub(4)..] {
                lines.push(line(style(format!("  {}", clip_str(l, width.saturating_sub(2))))));
            }
        };
        tail("stdout", &l.stdout, dim);
        tail("stderr", &l.stderr, error);
        lines
    }
}

impl Page for PluginsPage {
    fn title(&self) -> &'static str {
        "Plugins"
    }

    fn activated(&mut self, ctx: &mut Ctx) {
        if !self.shown {
            self.shown = true;
            if self.loaded {
                self.check_updates(ctx);
                self.load_detail(ctx);
            }
        }
    }

    fn update(&mut self, msg: &mut ui::Msg, ctx: &mut Ctx) {
        match msg {
            ui::Msg::PluginList(list) => {
                self.loaded = true;
                match list {
                    Ok(list) => (self.plugins, self.load_err) = (list.clone(), None),
                    Err(e) => self.load_err = Some(format!("{e:#}")),
                }
                self.details.clear();
                self.rebuild();
                if self.selected().is_none() {
                    self.panel = false;
                }
                if self.shown {
                    self.check_updates(ctx);
                    self.load_detail(ctx);
                }
            }
            ui::Msg::Plugins(Msg::Detail { id, settings, last }) => {
                self.details.insert(id.clone(), Detail { settings: std::mem::take(settings), last: std::mem::take(last) });
            }
            ui::Msg::Plugins(Msg::Checks(statuses)) => {
                self.checking = false;
                self.statuses = std::mem::take(statuses);
            }
            ui::Msg::Plugins(Msg::Market { query, sort, page, results }) => {
                if *query != self.market_query || *sort != self.market_sort {
                    return;
                }
                self.market_busy = false;
                match results {
                    Err(e) => self.market_err = Some(format!("{e:#}")),
                    Ok(results) => {
                        let repos = self.repos.get_or_insert_default();
                        if *page == 1 {
                            repos.clear();
                        }
                        repos.extend(std::mem::take(&mut results.repos));
                        self.market_total = results.total;
                        let labels = repos.iter().map(|r| format!("{} {}", r.full_name, r.description)).collect();
                        self.market.set_rows(labels, Vec::new());
                    }
                }
            }
            ui::Msg::Plugins(Msg::Logs { id, logs }) if *id == self.logs_for => match logs {
                Ok(logs) => {
                    self.logs = std::mem::take(logs);
                    self.logs_err = None;
                    let labels = self.logs.iter().map(|l| format!("{} {}", l.trigger(), l.status)).collect();
                    self.logs_list.set_rows(labels, Vec::new());
                }
                Err(e) => self.logs_err = Some(format!("{e:#}")),
            },
            ui::Msg::Plugins(Msg::Op { text, err }) => {
                match err {
                    Some(e) => ctx.error(format!("{text}: {e:#}")),
                    None => ctx.status(text.clone()),
                }
                ctx.spawn(load_plugins);
            }
            ui::Msg::Plugins(Msg::Pin { plugin, sha }) => match sha {
                Ok(sha) => {
                    let (plugin, sha) = (Plugin::clone(plugin), sha.clone());
                    self.confirm_pin_move(plugin, sha, ctx);
                }
                Err(e) => ctx.error(e),
            },
            ui::Msg::Plugins(Msg::RegistryEdited(result)) => {
                if let Err(e) = result {
                    ctx.error(e);
                }
                ctx.spawn(load_plugins);
            }
            ui::Msg::PendingSave { .. } | ui::Msg::Plugins(Msg::Config { .. } | Msg::ConfigSaved { .. } | Msg::ConfigReload(_)) => {
                if let Some(c) = &mut self.config {
                    c.update(msg, ctx);
                }
            }
            _ => {}
        }
    }

    fn key(&mut self, key: &Key, ctx: &mut Ctx) {
        if Confirm::handle(self.ask.take(), self, key, ctx) {
            return;
        }
        if self.mode != Mode::Browse {
            return self.input_key(key, ctx);
        }
        match self.screen {
            Screen::Market => self.market_key(key, ctx),
            Screen::Logs => {
                if !self.logs_list.handle(key) {
                    match key.name.as_str() {
                        "r" => self.load_logs(ctx),
                        "l" => self.screen = Screen::Installed,
                        _ => {}
                    }
                }
            }
            Screen::Config => {
                let Some(c) = &mut self.config else { return };
                if key.is("esc") && !c.editing {
                    if c.list.clear_filter() {
                        return;
                    }
                    c.flush(ctx);
                    self.screen = Screen::Installed;
                    return self.load_detail(ctx);
                }
                c.key(key, ctx);
            }
            Screen::Installed => self.installed_key(key, ctx),
        }
    }

    fn view(&mut self, _sh: &Shared, width: usize, height: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        if matches!(self.mode, Mode::InstallInput | Mode::MarketQuery) {
            lines.push(self.input.view());
        }
        match self.screen {
            Screen::Market => self.market_view(width, height, lines),
            Screen::Logs => self.logs_view(width, height, lines),
            Screen::Config => match &mut self.config {
                Some(c) => c.view(width, height, lines),
                None => lines,
            },
            Screen::Installed => self.installed_view(width, height, lines),
        }
    }

    fn help(&self) -> String {
        if let Some(help) = Confirm::help(&self.ask) {
            return help;
        }
        if self.mode != Mode::Browse {
            return "↵ confirm · esc cancel".into();
        }
        match self.screen {
            Screen::Market => "↵ install · o open repo · / search · s sort · r refresh · esc back",
            Screen::Logs => "↑/↓ move · r refresh · esc back",
            Screen::Config => self.config.as_ref().map_or("", PluginConfig::help),
            Screen::Installed if self.panel => {
                let on_action = self.selected().is_some_and(|p| matches!(self.menu(p).get(self.menu_cursor), Some(MenuItem { action: Action::Invoke(_), .. })));
                if on_action { "↵ run · b bind a key · ↑/↓ move · ← back · ? keys" } else { "↵ choose · ↑/↓ move · ← back · ? keys" }
            }
            Screen::Installed => "↵/→ open · ↑/↓ move · / search · m marketplace · i install · ? keys",
        }
        .into()
    }

    fn capturing(&self) -> bool {
        if self.mode != Mode::Browse || self.ask.is_some() {
            return true;
        }
        match self.screen {
            Screen::Market => self.market.filtering,
            Screen::Logs => self.logs_list.filtering,
            Screen::Config => self.config.as_ref().is_some_and(|c| c.editing || c.list.filtering),
            Screen::Installed => self.list.filtering,
        }
    }

    fn claims(&self, key: &Key) -> bool {
        self.screen == Screen::Config && self.mode == Mode::Browse && self.ask.is_none() && self.config.as_ref().is_some_and(|c| c.claims(key))
    }

    /// Leaves the marketplace, logs or the panel before the popup closes.
    fn back(&mut self) -> bool {
        match self.screen {
            Screen::Market | Screen::Logs => {
                self.screen = Screen::Installed;
                true
            }
            _ if self.panel => {
                self.panel = false;
                true
            }
            _ => self.list.clear_filter(),
        }
    }

    fn sheet(&self) -> Vec<Hint> {
        vec![
            ("↑/↓ j/k", "move"),
            ("↵ →", "open the plugin's panel (or start install / marketplace)"),
            ("← esc", "back to the list"),
            ("/", "search installed plugins"),
            ("", "On a plugin, from the list or the panel"),
            ("s", "settings: its config files, typed"),
            ("l", "recent runs and their output"),
            ("u", "update (moves an exact-commit pin after asking)"),
            ("e", "enable or disable"),
            ("x", "uninstall, or unlink a local checkout"),
            ("o", "open the repository"),
            ("b", "bind the selected action to a key"),
            ("", "Everywhere"),
            ("U", "update every plugin that is behind"),
            ("m", "marketplace of topic:herdr-plugin repositories"),
            ("i", "install owner/repo[/subdir][@ref]"),
            ("c", "edit plugins.json"),
            ("r", "reload"),
        ]
    }
}
