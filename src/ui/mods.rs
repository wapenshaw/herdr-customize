//! Mods and layouts for the Herdr app: what is installed, a marketplace of
//! GitHub repositories under the herdr-mod and herdr-layout topics, and a
//! review of what a package adds before it is installed.

use ratatui::text::Line;

use super::widgets::*;
use super::{self as ui, Ctx, Hint, Key, Page, Shared};
use crate::app::mods::{self, Kind, LAYOUT_TOPIC, MOD_TOPIC, Package, Plan, Report};
use crate::plugins::{self, Repo, Results};
use crate::{app, proc};

const MARKET_PAGE: usize = 50;

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Installed,
    Market,
    Review,
}

#[derive(Clone, Copy, PartialEq)]
enum Row {
    Header(&'static str),
    Package(usize),
    Other(usize),
    Shipped(usize),
    Gap,
    Market,
    AddRepo,
}

#[derive(Clone, Copy, PartialEq)]
enum Input {
    Repo,
    Query,
}

pub struct ModsPage {
    screen: Screen,
    packages: Vec<Package>,
    others: Vec<(Kind, String)>,
    shipped: Vec<String>,
    rows: Vec<Row>,
    list: List,
    // marketplace
    market: List,
    topic: &'static str,
    repos: Option<Vec<Repo>>,
    market_total: usize,
    market_query: String,
    market_sort: String,
    market_err: Option<String>,
    market_busy: bool,
    // review
    plans: Vec<Plan>,
    plan: usize,
    fetching: Option<String>,
    input: Option<(Input, TextInput)>,
    ask: Option<Confirm<ModsPage>>,
    shown: bool,
}

pub enum Msg {
    Loaded { packages: anyhow::Result<Vec<Package>>, others: Vec<(Kind, String)>, shipped: Vec<String> },
    Market { topic: &'static str, query: String, sort: String, page: usize, results: anyhow::Result<Results> },
    Fetched(String, anyhow::Result<Vec<Plan>>),
    Installed(String, anyhow::Result<Report>),
    Removed(String, anyhow::Result<Vec<String>>),
    Ran(String, anyhow::Result<()>),
}

impl From<Msg> for ui::Msg {
    fn from(msg: Msg) -> ui::Msg {
        ui::Msg::Mods(msg)
    }
}

fn load() -> ui::Msg {
    let packages = mods::packages();
    let others = mods::others(packages.as_deref().unwrap_or_default());
    Msg::Loaded { packages, others, shipped: app::shipped().into_iter().collect() }.into()
}

impl ModsPage {
    pub fn new() -> ModsPage {
        ModsPage {
            screen: Screen::Installed,
            packages: Vec::new(),
            others: Vec::new(),
            shipped: Vec::new(),
            rows: Vec::new(),
            list: List::new(),
            market: List::new(),
            topic: MOD_TOPIC,
            repos: None,
            market_total: 0,
            market_query: String::new(),
            market_sort: "stars".into(),
            market_err: None,
            market_busy: false,
            plans: Vec::new(),
            plan: 0,
            fetching: None,
            input: None,
            ask: None,
            shown: false,
        }
    }

    fn selected(&self) -> Option<Row> {
        self.rows.get(self.list.selected()?).copied()
    }

    fn rebuild(&mut self) {
        let (mut rows, mut labels) = (Vec::new(), Vec::new());
        let mut group = |name: &'static str, items: Vec<(Row, String)>| {
            if !items.is_empty() {
                rows.push(Row::Header(name));
                labels.push(name.to_string());
                for (r, l) in items {
                    rows.push(r);
                    labels.push(l);
                }
            }
        };
        group("Installed here", self.packages.iter().enumerate().map(|(i, p)| (Row::Package(i), format!("{} {}", p.label(), p.mods.join(" ")))).collect());
        group("Added by hand", self.others.iter().enumerate().map(|(i, (_, n))| (Row::Other(i), n.clone())).collect());
        group("Shipped with the app", self.shipped.iter().enumerate().map(|(i, n)| (Row::Shipped(i), n.clone())).collect());
        rows.extend([Row::Gap, Row::Market, Row::AddRepo]);
        labels.extend([String::new(), "browse marketplace".into(), "install owner/repo".into()]);
        let headers = rows.iter().map(|r| matches!(r, Row::Header(_) | Row::Gap)).collect();
        self.rows = rows;
        self.list.set_rows(labels, headers);
    }

    fn search(&mut self, page: usize, ctx: &mut Ctx) {
        if page == 1 {
            (self.repos, self.market_total, self.market_err) = (None, 0, None);
            self.market.set_rows(Vec::new(), Vec::new());
        }
        self.market_busy = true;
        let (topic, query, sort) = (self.topic, self.market_query.clone(), self.market_sort.clone());
        ctx.spawn(move || {
            let results = plugins::search(topic, &query, &sort, page, MARKET_PAGE);
            Msg::Market { topic, query, sort, page, results }.into()
        });
    }

    fn open_market(&mut self, ctx: &mut Ctx) {
        self.screen = Screen::Market;
        if self.repos.is_none() && !self.market_busy {
            self.search(1, ctx);
        }
    }

    fn fetch(&mut self, spec: String, ctx: &mut Ctx) {
        self.fetching = Some(spec.clone());
        self.plans.clear();
        self.plan = 0;
        self.screen = Screen::Review;
        ctx.spawn(move || {
            let plans = mods::fetch(&spec);
            Msg::Fetched(spec, plans).into()
        });
    }

    fn installed(&self, full_name: &str) -> bool {
        self.packages.iter().any(|p| p.repo.eq_ignore_ascii_case(full_name))
    }

    /// Shows a layout, or opens a mod, in the app.
    fn use_it(kind: Kind, name: String, ctx: &mut Ctx) {
        ctx.spawn(move || {
            let result = match kind {
                Kind::Layout => app::run("layout.switch", &[("name", &name)]),
                Kind::Mod => app::run("mod.open", &[("name", &name)]),
            };
            Msg::Ran(name, result).into()
        });
    }

    fn installed_key(&mut self, key: &Key, ctx: &mut Ctx) {
        if self.list.handle(key) {
            return;
        }
        let row = self.selected();
        let package = match row {
            Some(Row::Package(i)) => self.packages.get(i).cloned(),
            _ => None,
        };
        match key.name.as_str() {
            "m" => return self.open_market(ctx),
            "i" => return self.input = Some((Input::Repo, TextInput::new("install owner/repo[/folder]: ", ""))),
            "r" => return ctx.spawn(load),
            _ => {}
        }
        match (key.name.as_str(), row) {
            ("enter", Some(Row::Market)) => self.open_market(ctx),
            ("enter", Some(Row::AddRepo)) => self.input = Some((Input::Repo, TextInput::new("install owner/repo[/folder]: ", ""))),
            ("enter", Some(Row::Other(i))) => {
                let (kind, name) = self.others[i].clone();
                Self::use_it(kind, name, ctx);
            }
            ("enter", Some(Row::Shipped(i))) => {
                let name = self.shipped[i].clone();
                match name.strip_prefix("layout:") {
                    Some(l) => Self::use_it(Kind::Layout, l.into(), ctx),
                    None => Self::use_it(Kind::Mod, name, ctx),
                }
            }
            ("enter", Some(Row::Package(_))) => {
                let p = package.expect("a package row");
                match (p.layouts.first(), p.mods.first()) {
                    (Some(l), _) => Self::use_it(Kind::Layout, l.clone(), ctx),
                    (None, Some(m)) => Self::use_it(Kind::Mod, m.clone(), ctx),
                    _ => ctx.status("it has nothing to open"),
                }
            }
            ("u", Some(Row::Package(_))) => {
                let p = package.expect("a package row");
                self.fetch(p.label(), ctx);
            }
            ("x" | "d" | "delete", Some(Row::Package(_))) => {
                let p = package.expect("a package row");
                self.ask = Some(Confirm::new(format!("Remove {} and the keys it added?", p.label()), move |_: &mut ModsPage, ctx| {
                    let label = p.label();
                    ctx.write(move || Msg::Removed(label, mods::remove(&p.repo, &p.subdir)).into());
                }));
            }
            ("o", Some(Row::Package(_))) => {
                let p = package.expect("a package row");
                let url = plugins::github_url(&p.repo);
                match proc::open_url(&url) {
                    Ok(()) => ctx.status(format!("opened {url}")),
                    Err(e) => ctx.error(e),
                }
            }
            _ => {}
        }
    }

    fn market_key(&mut self, key: &Key, ctx: &mut Ctx) {
        if self.market.handle(key) {
            let loaded = self.repos.as_ref().map_or(0, Vec::len);
            if self.market.query().is_empty() && !self.market_busy && self.market_err.is_none() && self.market.cursor + 5 >= loaded && loaded < self.market_total {
                self.search(loaded / MARKET_PAGE + 1, ctx);
            }
            return;
        }
        match key.name.as_str() {
            "m" => return self.screen = Screen::Installed,
            "/" => return self.input = Some((Input::Query, TextInput::new("search: ", &self.market_query))),
            "left" | "right" => {
                self.topic = if self.topic == MOD_TOPIC { LAYOUT_TOPIC } else { MOD_TOPIC };
                return self.search(1, ctx);
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
            "enter" | "i" => self.fetch(repo.full_name, ctx),
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

    fn install(&mut self, replace_keys: bool, ctx: &mut Ctx) {
        let Some(plan) = self.plans.get(self.plan).cloned() else { return };
        if let Some(t) = plan.taken.first() {
            return ctx.error(format!("{t} is already there and is not from {}; remove it first", plan.label()));
        }
        let run = move |p: &mut ModsPage, ctx: &mut Ctx| {
            p.screen = Screen::Installed;
            let label = plan.label();
            ctx.status(format!("installing {label}…"));
            ctx.write(move || Msg::Installed(label, mods::install(&plan, replace_keys)).into());
        };
        let edited = self.plans[self.plan].edited.len();
        match (edited, replace_keys) {
            (0, false) => run(self, ctx),
            (0, true) => self.ask = Some(Confirm::new("Replace your keys that clash with its own?", run)),
            (n, _) => self.ask = Some(Confirm::new(format!("{n} file(s) you changed get replaced. Continue?"), run)),
        }
    }

    fn review_key(&mut self, key: &Key, ctx: &mut Ctx) {
        if let Some((cursor, _)) = step_cursor(self.plan, self.plans.len(), key) {
            self.plan = cursor;
            return;
        }
        match key.name.as_str() {
            "enter" => self.install(false, ctx),
            "k" if self.plans.get(self.plan).is_some_and(|p| !p.key_clashes.is_empty()) => self.install(true, ctx),
            _ => {}
        }
    }

    fn input_key(&mut self, key: &Key, ctx: &mut Ctx) {
        let Some((kind, input)) = &mut self.input else { return };
        match key.name.as_str() {
            "esc" => self.input = None,
            "enter" => {
                let (kind, text) = (*kind, input.value().trim().to_string());
                self.input = None;
                match kind {
                    Input::Query => {
                        self.market_query = text;
                        self.search(1, ctx);
                    }
                    Input::Repo if !text.is_empty() => self.fetch(text, ctx),
                    Input::Repo => {}
                }
            }
            _ => {
                input.handle(key);
            }
        }
    }

    fn installed_view(&mut self, width: usize, height: usize) -> Vec<Line<'static>> {
        let left_width = if width < 90 { width } else { 44 };
        let rows = self.list.window(height.saturating_sub(1));
        let mut lines = self.list.render(rows, |i, s| self.draw_row(i, s, left_width));
        lines.truncate(height.saturating_sub(1));
        match self.selected() {
            Some(r) if width >= 90 => columns(lines, left_width + 1, self.detail(r), height),
            _ => lines,
        }
    }

    fn draw_row(&self, i: usize, selected: bool, width: usize) -> Line<'static> {
        let text = |t: String| if selected { styled(SELECTED, t) } else { raw(t) };
        let kind = |k: Kind| dim(if k == Kind::Mod { "  mod" } else { "  layout" });
        match self.rows[i] {
            Row::Header(h) => line(dim(format!("  {}", h.to_uppercase()))),
            Row::Gap => Line::default(),
            Row::Market => row(Line::from(vec![accent("＋ Browse mods and layouts…"), dim("  m")]), selected),
            Row::AddRepo => row(Line::from(vec![accent("＋ Install from owner/repo…"), dim("  i")]), selected),
            Row::Package(p) => {
                let p = &self.packages[p];
                let what = [(p.mods.len(), "mod"), (p.layouts.len(), "layout")]
                    .iter()
                    .filter(|(n, _)| *n > 0)
                    .map(|(n, w)| format!("{n} {w}{}", if *n == 1 { "" } else { "s" }))
                    .collect::<Vec<_>>()
                    .join(", ");
                row(Line::from(vec![text(clip_str(&p.label(), width.saturating_sub(16))), dim(format!("  {what}"))]), selected)
            }
            Row::Other(o) => {
                let (k, n) = &self.others[o];
                row(Line::from(vec![text(n.clone()), kind(*k)]), selected)
            }
            Row::Shipped(s) => {
                let n = &self.shipped[s];
                let (k, name) = n.strip_prefix("layout:").map_or_else(|| (Kind::Mod, n.clone()), |l| (Kind::Layout, l.to_string()));
                row(Line::from(vec![text(name), kind(k)]), selected)
            }
        }
    }

    fn detail(&self, row: Row) -> Vec<Line<'static>> {
        let list = |label: &str, names: &[String]| (!names.is_empty()).then(|| Line::from(vec![dim(format!("{label:<8}")), raw(names.join(", "))]));
        match row {
            Row::Package(i) => {
                let p = &self.packages[i];
                let mut lines = vec![line(heading(p.label())), line(dim(format!("commit {}", p.commit.get(..7).unwrap_or(&p.commit)))), Line::default()];
                lines.extend([list("Mods", &p.mods), list("Layouts", &p.layouts), list("Themes", &p.themes)].into_iter().flatten());
                if !p.keys.is_empty() {
                    lines.push(line(dim("Keys")));
                    lines.extend(p.keys.iter().map(|[s, k, a]| line(raw(format!("  {k} → {a}  ({s})")))));
                }
                lines.extend([Line::default(), line(dim("↵ open · u update · x remove · o GitHub"))]);
                lines
            }
            Row::Other(i) => {
                let (k, n) = &self.others[i];
                vec![
                    line(heading(n.clone())),
                    Line::default(),
                    line(raw(if *k == Kind::Mod { "A mod in the app's mods folder." } else { "A layout in the app's layouts folder." })),
                    line(dim("Not installed from here, so it is left alone.")),
                    Line::default(),
                    line(dim("↵ open")),
                ]
            }
            Row::Shipped(i) => vec![line(heading(self.shipped[i].clone())), Line::default(), line(raw("Ships with the app, which puts it back on every start.")), Line::default(), line(dim("↵ open"))],
            Row::Market => vec![
                line(heading("Marketplace")),
                Line::default(),
                line(raw(format!("GitHub repositories tagged {MOD_TOPIC} or {LAYOUT_TOPIC}."))),
                line(dim("Each is reviewed before anything is written.")),
            ],
            _ => Vec::new(),
        }
    }

    fn market_view(&mut self, width: usize, height: usize) -> Vec<Line<'static>> {
        let mut title = format!("Marketplace · topic:{} · {}", self.topic, self.market_sort);
        if !self.market_query.is_empty() {
            title += &format!(" · \"{}\"", self.market_query);
        }
        let mut lines = vec![line(heading(title))];
        if let Some(e) = &self.market_err {
            lines.push(line(error(format!("✗ {e}"))));
        }
        let Some(repos) = &self.repos else {
            lines.push(line(dim(if self.market_busy { "  searching GitHub…" } else { "  nothing yet" })));
            return lines;
        };
        if repos.is_empty() {
            lines.push(line(dim("  no repositories")));
            return lines;
        }
        let rows = self.market.window(height.saturating_sub(2));
        let name_width = 40.min(width / 2);
        lines.extend(self.market.render(rows, |i, s| {
            let r = &repos[i];
            let mark = if self.installed(&r.full_name) { ok("✓ ") } else { raw("  ") };
            let name = pad(&clip_str(&r.full_name, name_width), name_width);
            row(Line::from(vec![mark, if s { styled(SELECTED, name) } else { raw(name) }, dim(format!(" ★{:<5}", r.stars)), raw(r.description.clone())]), s)
        }));
        lines.push(line(dim(format!("  {} of {}", repos.len(), self.market_total))));
        lines
    }

    fn review_view(&self, width: usize) -> Vec<Line<'static>> {
        if let Some(spec) = &self.fetching {
            return vec![line(heading(format!("Downloading {spec}…")))];
        }
        let mut lines = Vec::new();
        if self.plans.len() > 1 {
            lines.push(line(heading("This repository holds several packages; pick one:")));
            for (i, p) in self.plans.iter().enumerate() {
                lines.push(row(line(raw(p.label())), i == self.plan));
            }
            lines.push(Line::default());
        }
        let Some(p) = self.plans.get(self.plan) else { return lines };
        lines.push(Line::from(vec![heading(p.label()), dim(format!(" · {}", if p.update { "update" } else { "new" }))]));
        for l in wrap(&p.summary, width.saturating_sub(4).min(100)) {
            lines.push(line(dim(l)));
        }
        lines.push(Line::default());
        let list = |label: &str, names: &[String]| (!names.is_empty()).then(|| Line::from(vec![dim(format!("{label:<8}")), raw(names.join(", "))]));
        lines.extend([list("Mods", &p.mods), list("Layouts", &p.layouts), list("Themes", &p.themes)].into_iter().flatten());
        if !p.keys.is_empty() {
            lines.push(line(dim("Keys")));
            for b in &p.keys {
                let clash = p.key_clashes.iter().find(|(c, _)| c.section == b.section && c.key == b.key);
                let mut spans = vec![raw(format!("  {} → {}  ({})", b.key, b.action, b.section))];
                if let Some((_, yours)) = clash {
                    spans.push(warn(format!("  yours runs {yours}; kept unless k")));
                }
                lines.push(Line::from(spans));
            }
        }
        lines.push(Line::default());
        for u in &p.uses {
            lines.push(line(warn(format!("⚠ its code {u}"))));
        }
        for t in &p.taken {
            lines.push(line(error(format!("✗ {t} is already there and not from this package"))));
        }
        for e in &p.edited {
            lines.push(line(warn(format!("⚠ you changed {e}; installing replaces it"))));
        }
        lines
    }
}

impl Page for ModsPage {
    fn title(&self) -> &'static str {
        "Mods"
    }

    fn activated(&mut self, ctx: &mut Ctx) {
        if !self.shown {
            self.shown = true;
            ctx.spawn(load);
        }
    }

    fn update(&mut self, msg: &mut ui::Msg, ctx: &mut Ctx) {
        let ui::Msg::Mods(msg) = msg else { return };
        match msg {
            Msg::Loaded { packages, others, shipped } => {
                match std::mem::replace(packages, Ok(Vec::new())) {
                    Ok(p) => self.packages = p,
                    Err(e) => ctx.error(e),
                }
                (self.others, self.shipped) = (std::mem::take(others), std::mem::take(shipped));
                self.rebuild();
            }
            Msg::Market { topic, query, sort, page, results } => {
                // A later search replaced this one.
                if *topic != self.topic || *query != self.market_query || *sort != self.market_sort {
                    return;
                }
                self.market_busy = false;
                match std::mem::replace(results, Ok(Results::default())) {
                    Ok(r) => {
                        let repos = self.repos.get_or_insert_with(Vec::new);
                        if *page == 1 {
                            repos.clear();
                        }
                        repos.extend(r.repos);
                        self.market_total = r.total.min(1000);
                        let labels = repos.iter().map(|r| format!("{} {}", r.full_name, r.description)).collect();
                        let n = repos.len();
                        self.market.set_rows(labels, vec![false; n]);
                    }
                    Err(e) => self.market_err = Some(format!("{e:#}")),
                }
            }
            Msg::Fetched(spec, plans) => {
                if self.fetching.as_ref() != Some(spec) {
                    return;
                }
                self.fetching = None;
                match std::mem::replace(plans, Ok(Vec::new())) {
                    Ok(p) => self.plans = p,
                    Err(e) => {
                        self.screen = Screen::Installed;
                        ctx.error(e);
                    }
                }
            }
            Msg::Installed(label, result) => {
                match result {
                    Ok(r) => {
                        let mut text = format!("{label} installed");
                        if r.kept_keys > 0 {
                            text += &format!("; kept {} of your keys", r.kept_keys);
                        }
                        let failed: Vec<String> = r.checks.iter().filter_map(|(m, e)| Some(format!("{m}: {}", e.as_ref()?))).collect();
                        if failed.is_empty() {
                            ctx.status(text);
                        } else {
                            ctx.error(format!("{text}, but the app's check failed: {}", failed.join("; ")));
                        }
                    }
                    Err(e) => ctx.error(format!("{label} not installed: {e:#}")),
                }
                self.plans.clear();
                ctx.spawn(load);
            }
            Msg::Removed(label, result) => {
                match result {
                    Ok(kept) if kept.is_empty() => ctx.status(format!("{label} removed")),
                    Ok(kept) => ctx.status(format!("{label} removed; kept the files you changed: {}", kept.join(", "))),
                    Err(e) => ctx.error(e),
                }
                ctx.spawn(load);
            }
            Msg::Ran(name, result) => match result {
                Ok(()) => ctx.status(format!("opened {name} in the app")),
                Err(e) => ctx.error(e),
            },
        }
    }

    fn key(&mut self, key: &Key, ctx: &mut Ctx) {
        if Confirm::handle(self.ask.take(), self, key, ctx) {
            return;
        }
        if self.input.is_some() {
            return self.input_key(key, ctx);
        }
        match self.screen {
            Screen::Installed => self.installed_key(key, ctx),
            Screen::Market => self.market_key(key, ctx),
            Screen::Review => self.review_key(key, ctx),
        }
    }

    fn view(&mut self, _sh: &Shared, width: usize, height: usize) -> Vec<Line<'static>> {
        let input_height = usize::from(self.input.is_some()) * 2;
        let body_height = height.saturating_sub(input_height);
        let mut lines = match self.screen {
            Screen::Installed => self.installed_view(width, body_height),
            Screen::Market => self.market_view(width, body_height),
            Screen::Review => self.review_view(width),
        };
        if let Some((_, input)) = &self.input {
            lines = fit(lines, width, body_height);
            lines.extend([Line::default(), input.view()]);
        }
        lines
    }

    fn help(&self) -> String {
        if let Some(help) = Confirm::help(self.ask.as_ref()) {
            return help;
        }
        if self.input.is_some() {
            return "↵ go · esc cancel".into();
        }
        match self.screen {
            Screen::Installed => "↵ open · m marketplace · i install owner/repo · u update · x remove · ? keys".into(),
            Screen::Market => "↵ review · ←/→ mods/layouts · / search · s sort · o GitHub · esc back".into(),
            Screen::Review if self.plans.get(self.plan).is_some_and(|p| !p.key_clashes.is_empty()) => "↵ install, keeping your keys · k install and take them · esc back".into(),
            Screen::Review => "↵ install · esc back".into(),
        }
    }

    fn capturing(&self) -> bool {
        self.input.is_some() || self.ask.is_some() || self.list.filtering || self.market.filtering
    }

    fn back(&mut self) -> bool {
        match self.screen {
            Screen::Installed => self.list.clear_filter(),
            Screen::Market if self.market.clear_filter() => true,
            Screen::Market => {
                self.screen = Screen::Installed;
                true
            }
            Screen::Review => {
                self.plans.clear();
                self.fetching = None;
                self.screen = if self.repos.is_some() { Screen::Market } else { Screen::Installed };
                true
            }
        }
    }

    fn sheet(&self) -> Vec<Hint> {
        vec![
            ("↵", "open a mod or show a layout in the app"),
            ("m", "the marketplace: herdr-mod and herdr-layout repositories"),
            ("i", "install from owner/repo, or owner/repo/folder"),
            ("u", "update a package installed here"),
            ("x", "remove it and the keys it added; files you changed stay"),
            ("o", "open it on GitHub"),
            ("r", "reload"),
            ("", "Installing"),
            ("", "Every package is reviewed first: what it adds, the keys it takes,"),
            ("", "and whether its code runs programs, reaches the network or files."),
        ]
    }
}
