//! Themes for the Herdr app: its own themes, theme files in its folder, and
//! every palette herdr-customize has, which is written there when picked.

use std::fs;

use ratatui::text::Line;

use super::themes::{framed, swatch};
use super::widgets::*;
use super::{self as ui, Ctx, Hint, Key, Page, Shared};
use crate::app::themes::{self as app_themes, AppTheme, Current, Origin, Summary};
use crate::theme::{self, Saved};

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Dark,
    Light,
    Yours,
}

impl Filter {
    const ALL: [Filter; 4] = [Filter::All, Filter::Dark, Filter::Light, Filter::Yours];

    fn name(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Dark => "Dark",
            Filter::Light => "Light",
            Filter::Yours => "Added",
        }
    }

    fn next(self, n: usize) -> Filter {
        Filter::ALL[(self as usize + n) % Filter::ALL.len()]
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Row {
    Header(&'static str),
    Theme(usize),
    Default,
    Gap,
    AddEditor,
    AddClipboard,
}

pub struct AppThemesPage {
    items: Vec<AppTheme>,
    current: Option<Current>,
    rows: Vec<Row>,
    filter: Filter,
    list: List,
    /// Naming a new palette, held in pending_body.
    naming: bool,
    input: TextInput,
    pending_body: String,
    ask: Option<Confirm<AppThemesPage>>,
    shown: bool,
}

pub enum Msg {
    Loaded { items: Vec<AppTheme>, current: Current, warnings: Vec<String>, err: Option<anyhow::Error> },
    Applied(String, anyhow::Result<()>),
    Installed(anyhow::Result<Summary>),
    EditorDone(anyhow::Result<String>),
}

impl From<Msg> for ui::Msg {
    fn from(msg: Msg) -> ui::Msg {
        ui::Msg::AppThemes(msg)
    }
}

fn load() -> ui::Msg {
    let current = app_themes::current();
    match app_themes::list() {
        Ok((items, warnings)) => Msg::Loaded { items, current, warnings, err: None },
        Err(e) => Msg::Loaded { items: Vec::new(), current, warnings: Vec::new(), err: Some(e) },
    }
    .into()
}

impl AppThemesPage {
    pub fn new() -> AppThemesPage {
        let mut list = List::new();
        list.hide_filter = true;
        AppThemesPage {
            items: Vec::new(),
            current: None,
            rows: Vec::new(),
            filter: Filter::All,
            list,
            naming: false,
            input: TextInput::default(),
            pending_body: String::new(),
            ask: None,
            shown: false,
        }
    }

    fn selected_row(&self) -> Option<Row> {
        self.rows.get(self.list.selected()?).copied()
    }

    fn selected_item(&self) -> Option<&AppTheme> {
        match self.selected_row()? {
            Row::Theme(i) => self.items.get(i),
            _ => None,
        }
    }

    /// "dark", "light" or "dark and light" when the app shows name now.
    fn in_use(&self, name: &str) -> Option<&'static str> {
        let c = self.current.as_ref()?;
        match (c.dark == name, c.light == name) {
            (true, true) => Some("dark and light"),
            (true, false) => Some("dark"),
            (false, true) => Some("light"),
            _ => None,
        }
    }

    fn yours(item: &AppTheme) -> bool {
        matches!(item.origin, Origin::File | Origin::Mine)
    }

    fn shown_in(&self, filter: Filter, item: &AppTheme) -> bool {
        match filter {
            Filter::All => true,
            Filter::Dark => item.dark,
            Filter::Light => !item.dark,
            Filter::Yours => Self::yours(item),
        }
    }

    fn rebuild(&mut self) {
        let keep = self.selected_item().map(|i| i.name.clone());
        let mut groups: [Vec<usize>; 5] = Default::default(); // in use, the app's, yours, dark, light
        for (i, item) in self.items.iter().enumerate() {
            if !self.shown_in(self.filter, item) {
                continue;
            }
            let g = match self.filter {
                Filter::All if self.in_use(&item.name).is_some() => 0,
                Filter::All if item.origin == Origin::Preset => 1,
                Filter::All | Filter::Yours if Self::yours(item) => 2,
                _ if item.dark => 3,
                _ => 4,
            };
            groups[g].push(i);
        }
        let (mut rows, mut labels) = (Vec::new(), Vec::new());
        for (g, name) in ["In use", "Herdr app", "Added", "Dark", "Light"].into_iter().enumerate() {
            if groups[g].is_empty() {
                continue;
            }
            if self.filter == Filter::All {
                rows.push(Row::Header(name));
                labels.push(name.to_string());
            }
            for &i in &groups[g] {
                rows.push(Row::Theme(i));
                labels.push(self.items[i].name.clone());
            }
        }
        rows.push(Row::Gap);
        labels.push(String::new());
        if self.filter == Filter::All {
            rows.push(Row::Default);
            labels.push("herdr app default graphite paper".into());
        }
        rows.extend([Row::AddEditor, Row::AddClipboard]);
        labels.extend(["add new theme in editor".into(), "add theme from clipboard paste".into()]);
        let headers = rows.iter().map(|r| matches!(r, Row::Header(_) | Row::Gap)).collect();
        self.rows = rows;
        self.list.set_rows(labels, headers);
        if let Some(keep) = keep
            && let Some(i) = self.rows.iter().position(|r| matches!(r, Row::Theme(i) if self.items[*i].name == keep))
        {
            self.list.select_row(i);
        }
        self.list.skip_headers(1);
    }

    fn apply(&mut self, item: AppTheme, ctx: &mut Ctx) {
        ctx.status(format!("applying {}…", item.name));
        ctx.write(move || Msg::Applied(item.name.clone(), app_themes::apply(&item.name, item.origin)).into());
    }

    fn add_in_editor(&mut self, ctx: &mut Ctx) {
        let path = match theme::new_theme_file() {
            Ok(p) => p,
            Err(e) => return ctx.error(e),
        };
        ctx.edit_file(path.clone(), move |body| {
            let _ = fs::remove_file(&path);
            Msg::EditorDone(body).into()
        });
    }

    fn add_from_clipboard(&mut self, ctx: &mut Ctx) {
        match theme::read_clipboard() {
            Err(e) => ctx.error(e),
            Ok(clip) if clip.trim().is_empty() => ctx.error("clipboard is empty (copy a Ghostty theme from https://terminalcolors.com first)"),
            Ok(clip) => self.ask_name(clip, ctx),
        }
    }

    fn ask_name(&mut self, body: String, ctx: &mut Ctx) {
        if let Err(e) = theme::validate_theme_body(&body) {
            return ctx.error(e);
        }
        self.pending_body = body;
        self.naming = true;
        self.input = TextInput::new("Theme name: ", "");
    }

    fn save_new(&mut self, overwrite: bool, ctx: &mut Ctx) {
        match app_themes::add(&self.input.value(), &self.pending_body, overwrite) {
            Ok(Saved::Exists(name)) => {
                self.naming = false;
                self.ask = Some(Confirm::new(format!("themes/{name}.json already exists. Overwrite?"), |p: &mut AppThemesPage, ctx| {
                    p.naming = true;
                    p.save_new(true, ctx);
                }));
            }
            Ok(Saved::Saved(name)) => {
                self.naming = false;
                self.pending_body.clear();
                self.apply(AppTheme { name, origin: Origin::File, look: None, dark: true, installed: true }, ctx);
            }
            Err(e) => ctx.error(e),
        }
    }

    fn delete(&mut self, item: AppTheme, ctx: &mut Ctx) {
        if item.origin == Origin::Preset || !item.installed {
            return ctx.error("only theme files in the app's themes folder can be deleted");
        }
        let what = if item.origin == Origin::File { "" } else { " (the palette stays here)" };
        self.ask = Some(Confirm::new(format!("Delete themes/{}.json{what}?", item.name), move |_: &mut AppThemesPage, ctx| match app_themes::remove(&item.name) {
            Ok(()) => {
                ctx.spawn(load);
                ctx.status(format!("deleted themes/{}.json", item.name));
            }
            Err(e) => ctx.error(e),
        }));
    }

    fn browse_key(&mut self, key: &Key, ctx: &mut Ctx) {
        if let Some(text) = &key.paste {
            if text.contains("palette") && text.contains('\n') {
                return self.ask_name(text.clone(), ctx);
            }
            let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
            return self.list.set_query(&(self.list.query() + &words));
        }
        if let Some(c) = key.ch {
            return self.list.set_query(&format!("{}{c}", self.list.query()));
        }
        match key.name.as_str() {
            "backspace" => {
                let mut q = self.list.query();
                q.pop();
                return self.list.set_query(&q);
            }
            "ctrl+u" => return self.list.set_query(""),
            "up" | "down" | "pgup" | "pgdown" | "home" | "end" => {
                self.list.handle(key);
                return;
            }
            _ => {}
        }
        let item = self.selected_item().cloned();
        match key.name.as_str() {
            "left" => {
                self.filter = self.filter.next(Filter::ALL.len() - 1);
                self.rebuild();
            }
            "right" => {
                self.filter = self.filter.next(1);
                self.rebuild();
            }
            "enter" => match (item, self.selected_row()) {
                (Some(item), _) => self.apply(item, ctx),
                (None, Some(Row::Default)) => {
                    ctx.status("back to graphite and paper…");
                    ctx.write(|| Msg::Applied("graphite and paper".into(), app_themes::reset()).into());
                }
                (None, Some(Row::AddEditor)) => self.add_in_editor(ctx),
                (None, Some(Row::AddClipboard)) => self.add_from_clipboard(ctx),
                _ => {}
            },
            "ctrl+a" => self.add_in_editor(ctx),
            "ctrl+v" => self.add_from_clipboard(ctx),
            "ctrl+d" => match item {
                Some(item) => self.delete(item, ctx),
                None => ctx.error("pick a theme to delete"),
            },
            "ctrl+s" => {
                ctx.status("writing every palette to the app…");
                ctx.write(|| Msg::Installed(app_themes::install_all(false)).into());
            }
            "ctrl+r" => ctx.spawn(load),
            _ => {}
        }
    }

    fn chips(&self) -> Line<'static> {
        let mut spans = vec![raw(" ")];
        for filter in Filter::ALL {
            let count = self.items.iter().filter(|item| self.shown_in(filter, item)).count();
            let label = format!(" {} {count} ", filter.name());
            spans.push(if filter == self.filter { styled(TAB_ACTIVE, label) } else { dim(label) });
        }
        Line::from(spans)
    }

    fn search_line(&self) -> Line<'static> {
        let query = self.list.query();
        if query.is_empty() {
            return line(dim(" ⌕ type to search"));
        }
        let matches = self.list.matches.iter().filter(|&&i| matches!(self.rows[i], Row::Theme(_))).count();
        Line::from(vec![accent(" ⌕ "), raw(query), styled(SELECTED, "▏"), dim(format!("  {matches} match(es)"))])
    }

    fn theme_row(&self, i: usize, selected: bool, name_width: usize) -> Line<'static> {
        let item = match self.rows[i] {
            Row::Header(h) => return line(dim(format!("  {}", h.to_uppercase()))),
            Row::Gap => return Line::default(),
            Row::AddEditor => return row(Line::from(vec![accent("＋ Add new theme…"), dim("  ctrl+a")]), selected),
            Row::AddClipboard => return row(Line::from(vec![accent("＋ Add from clipboard…"), dim("  ctrl+v")]), selected),
            Row::Default => {
                let name = "Herdr app default (graphite, paper)";
                return row(Line::from(vec![accent("↩ "), if selected { styled(SELECTED, name) } else { raw(name) }]), selected);
            }
            Row::Theme(i) => &self.items[i],
        };
        let marker = match (self.in_use(&item.name), item.origin) {
            (Some(_), _) => ok("● "),
            (None, Origin::File | Origin::Mine) => warn("★ "),
            _ => raw("  "),
        };
        let name = pad(&clip_str(&item.name, name_width), name_width);
        let mut spans = vec![marker, if selected { styled(SELECTED, name) } else { raw(name) }, raw(" ")];
        match item.origin {
            Origin::Preset => spans.push(dim("built in")),
            _ => spans.extend(swatch(item.look.as_ref())),
        }
        row(Line::from(spans), selected)
    }

    fn card(&self, item: &AppTheme) -> Vec<Line<'static>> {
        let mut title = vec![heading(item.name.clone()), dim(if item.dark { " · dark" } else { " · light" })];
        if let Some(half) = self.in_use(&item.name) {
            title.push(ok(format!(" · ● in use ({half})")));
        }
        title.push(dim(match item.origin {
            Origin::Preset => " · the app's own",
            Origin::File => " · ★ a file in the app's themes",
            Origin::Mine => " · ★ yours",
            Origin::Bundled if item.installed => " · in the app's themes",
            Origin::Bundled => " · written there when picked",
        }));
        match &item.look {
            Some(look) => framed(Line::from(title), look, false),
            None => vec![Line::from(title), Line::default(), line(raw("One of the app's own themes; its colors live in the app."))],
        }
    }

    fn default_card(&self) -> Vec<Line<'static>> {
        let mut lines = vec![line(heading("Herdr app default")), Line::default(), line(raw("Shows graphite when the system is dark")), line(raw("and paper when it is light."))];
        if let Some(c) = &self.current {
            lines.extend([Line::default(), line(dim(format!("Now: {} (dark), {} (light)", c.dark, c.light)))]);
        }
        lines
    }
}

impl Page for AppThemesPage {
    fn title(&self) -> &'static str {
        "Themes"
    }

    fn activated(&mut self, ctx: &mut Ctx) {
        if !self.shown {
            self.shown = true;
            ctx.spawn(load);
        }
    }

    fn update(&mut self, msg: &mut ui::Msg, ctx: &mut Ctx) {
        let ui::Msg::AppThemes(msg) = msg else { return };
        match msg {
            Msg::Loaded { items, current, warnings, err } => {
                if let Some(e) = err {
                    return ctx.error(e);
                }
                self.items = std::mem::take(items);
                self.current = Some(current.clone());
                self.rebuild();
                if !warnings.is_empty() {
                    ctx.status(warnings.join("; "));
                }
            }
            Msg::Applied(name, result) => {
                match result {
                    Ok(()) => ctx.status(format!("{name} applied")),
                    Err(e) => ctx.error(e),
                }
                ctx.spawn(load);
            }
            Msg::Installed(result) => {
                match result {
                    Ok(s) if s.kept.is_empty() => ctx.status(format!("{} palettes written, {} already there", s.written, s.same)),
                    Ok(s) => ctx.status(format!("{} palettes written, {} already there; kept your edits to {}", s.written, s.same, s.kept.join(", "))),
                    Err(e) => ctx.error(e),
                }
                ctx.spawn(load);
            }
            Msg::EditorDone(body) => match body {
                Ok(body) => self.ask_name(std::mem::take(body), ctx),
                Err(e) => ctx.error(e),
            },
        }
    }

    fn key(&mut self, key: &Key, ctx: &mut Ctx) {
        if Confirm::handle(self.ask.take(), self, key, ctx) {
            return;
        }
        if !self.naming {
            return self.browse_key(key, ctx);
        }
        match key.name.as_str() {
            "esc" => {
                self.naming = false;
                self.pending_body.clear();
                ctx.status("not saved");
            }
            "enter" => self.save_new(false, ctx),
            _ => {
                self.input.handle(key);
            }
        }
    }

    fn view(&mut self, _sh: &Shared, width: usize, height: usize) -> Vec<Line<'static>> {
        let left_width = if width < 90 { width } else { 38 };
        let mut list_height = height.saturating_sub(4);
        if self.naming {
            list_height = list_height.saturating_sub(2);
        }
        let name_width = left_width.saturating_sub(14);
        let rows = self.list.window(list_height);
        let body = self.list.render(rows, |i, s| self.theme_row(i, s, name_width));
        let mut lines = vec![self.chips(), self.search_line()];
        lines.extend(fit(body, left_width, list_height));
        lines.push(Line::default());
        if self.naming {
            lines.extend([self.input.view(), Line::default()]);
        }
        lines.push(line(dim("  palettes: terminalcolors.com → Ghostty")));
        let card = match (self.selected_item(), self.selected_row()) {
            (Some(item), _) => Some(self.card(item)),
            (None, Some(Row::Default)) => Some(self.default_card()),
            _ => None,
        };
        // A card clipped at the edge reads as broken, so it shows only whole.
        match card {
            Some(card) if left_width + 1 + card.iter().map(Line::width).max().unwrap_or(0) <= width => columns(lines, left_width + 1, card, height),
            _ => lines,
        }
    }

    fn help(&self) -> String {
        if let Some(help) = Confirm::help(self.ask.as_ref()) {
            return help;
        }
        if self.naming {
            return "type a name · ↵ save and apply · esc cancel".into();
        }
        "type to search · ↵ apply · ←/→ all/dark/light/added · ctrl+a add · ctrl+v paste · ctrl+d delete · ? keys".into()
    }

    fn capturing(&self) -> bool {
        self.naming || self.ask.is_some()
    }

    fn claims(&self, key: &Key) -> bool {
        if self.naming || self.ask.is_some() {
            return false;
        }
        if key.paste.is_some() {
            return true;
        }
        match key.ch {
            Some('?') | None => false,
            Some('1'..='3') => !self.list.query().is_empty(),
            Some(_) => true,
        }
    }

    fn back(&mut self) -> bool {
        self.list.clear_filter()
    }

    fn sheet(&self) -> Vec<Hint> {
        vec![
            ("a–z …", "type to search"),
            ("⌫ ctrl+u", "delete a character, clear the search"),
            ("↑/↓", "move; the preview follows"),
            ("←/→", "show all, dark, light or added themes"),
            ("↵", "apply: the app shows it for its lightness, dark or light"),
            ("esc", "clear the search, then close"),
            ("", "Theme files"),
            ("ctrl+a", "add: write or paste a palette in your editor"),
            ("ctrl+v", "add a palette from the clipboard (terminalcolors.com)"),
            ("ctrl+d", "delete a theme file from the app's themes folder"),
            ("ctrl+s", "write every palette to the app, for its own picker"),
            ("ctrl+r", "reload the list"),
        ]
    }
}
