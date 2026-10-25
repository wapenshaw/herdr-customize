//! Themes: a searchable list of palettes beside a preview of the selected
//! one, plus adding, editing and deleting your own.

use std::collections::HashMap;
use std::fs;

use ratatui::style::Color;
use ratatui::text::{Line, Span};

use super::widgets::*;
use super::{Ctx, Hint, Key, Msg, Page, Shared};
use crate::theme::preview::{paint, render_mock, render_token_table, rgb};
use crate::theme::{self, Item, Look, Saved, TokenRow, Tokens};

/// Explains how a client attached from another machine picks its theme.
const CLIENT_SETUP: &str = "Themes belong to your client machine.\n\nOpen a local terminal outside Herdr and run:\n\n  herdr-customize ui themes\n\nThen use Reload config in the Herdr client you want to update.\nInstall herdr-customize locally if you are connected to a remote server.\nThis server's theme and terminal colors are left unchanged.";

/// The views ←/→ move between.
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
            Filter::Yours => "Yours",
        }
    }

    /// The view n places to the right, wrapping around.
    fn next(self, n: usize) -> Filter {
        Filter::ALL[(self as usize + n) % Filter::ALL.len()]
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Browse,
    Naming,
    Editing,
    EditingColor,
}

/// A group heading, one theme, or a row that adds themes.
#[derive(Clone, Copy, PartialEq)]
enum Row {
    Header(&'static str),
    Theme(usize),
    Gap,
    AddEditor,
    AddClipboard,
}

pub struct ThemesPage {
    items: Vec<Item>,
    looks: HashMap<String, Look>,
    rows: Vec<Row>,
    filter: Filter,
    show_tokens: bool,
    list: List,
    mode: Mode,
    input: TextInput,
    ask: Option<Confirm<ThemesPage>>,
    remote_only: bool,
    /// A new palette waiting for a name.
    pending_body: String,
    // token editor
    edit_slug: String,
    edit_body: String,
    edit_dirty: bool,
    edit_rows: Vec<TokenRow>,
    edit_tokens: Tokens,
    edit_cursor: usize,
}

fn load_themes() -> Msg {
    match theme::themes() {
        Ok((items, warnings)) => {
            let looks = items.iter().filter_map(|i| Some((i.slug.clone(), theme::load_look(&i.slug).ok()?))).collect();
            Msg::ThemesLoaded { items, looks, warnings, err: None }
        }
        Err(e) => Msg::ThemesLoaded { items: Vec::new(), looks: HashMap::new(), warnings: Vec::new(), err: Some(e) },
    }
}

impl ThemesPage {
    pub fn new() -> ThemesPage {
        let mut list = List::new();
        // The page shows the query in its own search line.
        list.hide_filter = true;
        ThemesPage {
            items: Vec::new(),
            looks: HashMap::new(),
            rows: Vec::new(),
            filter: Filter::All,
            show_tokens: false,
            list,
            mode: Mode::Browse,
            input: TextInput::default(),
            ask: None,
            remote_only: false,
            pending_body: String::new(),
            edit_slug: String::new(),
            edit_body: String::new(),
            edit_dirty: false,
            edit_rows: Vec::new(),
            edit_tokens: Tokens::new(),
            edit_cursor: 0,
        }
    }

    fn selected_item(&self) -> Option<&Item> {
        match self.rows.get(self.list.selected()?)? {
            Row::Theme(i) => self.items.get(*i),
            _ => None,
        }
    }

    fn dark(&self, item: &Item) -> bool {
        self.looks.get(&item.slug).is_none_or(|l| l.dark)
    }

    fn shown(&self, filter: Filter, item: &Item) -> bool {
        match filter {
            Filter::All => true,
            Filter::Dark => self.dark(item),
            Filter::Light => !self.dark(item),
            Filter::Yours => item.user,
        }
    }

    /// Lists the themes the filter shows, keeping the selection.
    fn rebuild(&mut self) {
        let keep = self.selected_item().map(|i| i.slug.clone());
        let mut groups: [Vec<usize>; 4] = Default::default(); // in use, yours, dark, light
        for (i, item) in self.items.iter().enumerate() {
            if !self.shown(self.filter, item) {
                continue;
            }
            let g = match self.filter {
                Filter::Dark => 2,
                Filter::Light => 3,
                Filter::Yours => 1,
                Filter::All if item.applied => 0,
                Filter::All if item.user => 1,
                Filter::All if self.dark(item) => 2,
                Filter::All => 3,
            };
            groups[g].push(i);
        }
        let (mut rows, mut labels) = (Vec::new(), Vec::new());
        for (g, name) in ["In use", "Yours", "Dark", "Light"].into_iter().enumerate() {
            if groups[g].is_empty() {
                continue;
            }
            if self.filter == Filter::All {
                rows.push(Row::Header(name));
                labels.push(name.to_string());
            }
            for &i in &groups[g] {
                rows.push(Row::Theme(i));
                labels.push(self.items[i].slug.clone());
            }
        }
        rows.extend([Row::Gap, Row::AddEditor, Row::AddClipboard]);
        labels.extend(["".into(), "add new theme in editor".into(), "add theme from clipboard paste".into()]);
        let headers = rows.iter().map(|r| matches!(r, Row::Header(_) | Row::Gap)).collect();
        self.rows = rows;
        self.list.set_rows(labels, headers);
        if let Some(keep) = keep
            && let Some(i) = self.rows.iter().position(|r| matches!(r, Row::Theme(i) if self.items[*i].slug == keep))
        {
            self.list.select_row(i);
        }
        self.list.skip_headers(1);
    }

    /// Opens a palette template in the user's editor.
    fn add_in_editor(&mut self, ctx: &mut Ctx) {
        let path = match theme::new_theme_file() {
            Ok(p) => p,
            Err(e) => return ctx.error(e),
        };
        ctx.edit_file(path.clone(), move |body| {
            let _ = fs::remove_file(&path);
            Msg::ThemeEditorDone(body)
        });
    }

    fn add_from_clipboard(&mut self, ctx: &mut Ctx) {
        match theme::read_clipboard() {
            Err(e) => ctx.error(e),
            Ok(clip) if clip.trim().is_empty() => ctx.error("clipboard is empty (copy a Ghostty theme from https://terminalcolors.com first)"),
            Ok(clip) => self.ask_name(clip, ctx),
        }
    }

    /// Validates a new palette, then prompts for its name.
    fn ask_name(&mut self, body: String, ctx: &mut Ctx) {
        if let Err(e) = theme::validate_theme_body(&body) {
            return ctx.error(e);
        }
        self.pending_body = body;
        self.mode = Mode::Naming;
        self.input = TextInput::new("Theme name: ", "");
    }

    fn save_new(&mut self, overwrite: bool, ctx: &mut Ctx) {
        match theme::save_user_theme(&self.input.value(), &self.pending_body, overwrite) {
            Ok(Saved::Exists(slug)) => {
                self.mode = Mode::Browse;
                self.ask = Some(Confirm::new(format!("{slug:?} already exists. Overwrite?"), |p: &mut ThemesPage, ctx| {
                    p.mode = Mode::Naming;
                    p.save_new(true, ctx);
                }));
            }
            Ok(Saved::Saved(slug)) => {
                self.mode = Mode::Browse;
                self.pending_body.clear();
                ctx.quit_then(move || theme::apply_theme(&slug));
            }
            Err(e) => ctx.error(e),
        }
    }

    fn begin_edit(&mut self, slug: &str) -> anyhow::Result<()> {
        let body = theme::user_theme_body(slug)?;
        self.edit_slug = slug.to_string();
        self.edit_cursor = 0;
        self.edit_dirty = false;
        self.mode = Mode::Editing;
        self.set_edit_body(body, false)
    }

    fn set_edit_body(&mut self, body: String, dirty: bool) -> anyhow::Result<()> {
        let (rows, tokens) = theme::token_rows(&body)?;
        (self.edit_body, self.edit_rows, self.edit_tokens) = (body, rows, tokens);
        self.edit_dirty |= dirty;
        Ok(())
    }

    fn editor_key(&mut self, key: &Key, ctx: &mut Ctx) {
        if let Some((cursor, _)) = step_cursor(self.edit_cursor, self.edit_rows.len(), key) {
            self.edit_cursor = cursor;
            return;
        }
        match key.name.as_str() {
            "esc" | "q" => {
                self.mode = Mode::Browse;
                if std::mem::take(&mut self.edit_dirty) {
                    ctx.status("edits discarded");
                }
            }
            "enter" if let Some(r) = self.edit_rows.get(self.edit_cursor) => {
                self.input = TextInput::new(format!("{}: ", r.token), &r.color);
                self.mode = Mode::EditingColor;
            }
            "s" => {
                if let Err(e) = theme::commit_user_theme(&self.edit_slug, &self.edit_body) {
                    return ctx.error(e);
                }
                let slug = self.edit_slug.clone();
                ctx.quit_then(move || theme::apply_theme(&slug));
            }
            _ => {}
        }
    }

    fn browse_key(&mut self, key: &Key, ctx: &mut Ctx) {
        if let Some(text) = &key.paste {
            // A pasted palette is added rather than searched for.
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
            "ctrl+t" => self.show_tokens = !self.show_tokens,
            "enter" => match (item, self.list.selected().and_then(|i| self.rows.get(i))) {
                (Some(item), _) => ctx.quit_then(move || theme::apply_theme(&item.slug)),
                (None, Some(Row::AddEditor)) => self.add_in_editor(ctx),
                (None, Some(Row::AddClipboard)) => self.add_from_clipboard(ctx),
                _ => {}
            },
            "ctrl+a" => self.add_in_editor(ctx),
            "ctrl+v" => self.add_from_clipboard(ctx),
            "ctrl+e" => match item {
                Some(item) if item.user => {
                    if let Err(e) = self.begin_edit(&item.slug) {
                        ctx.error(e);
                    }
                }
                _ => ctx.error("only ★ user-added themes can be edited"),
            },
            "ctrl+d" => match item {
                Some(item) if item.user => {
                    self.ask = Some(Confirm::new(format!("Delete {:?}?", item.slug), move |_: &mut ThemesPage, ctx| match theme::remove_user_theme(&item.slug) {
                        Ok(()) => {
                            ctx.spawn(load_themes);
                            ctx.status(format!("deleted {:?}", item.slug));
                        }
                        Err(e) => ctx.error(e),
                    }));
                }
                _ => ctx.error("only ★ user-added themes can be deleted"),
            },
            "ctrl+r" => ctx.spawn(load_themes),
            _ => {}
        }
    }

    /// The filters with the current one highlighted.
    fn chips(&self) -> Line<'static> {
        let mut spans = vec![raw(" ")];
        for filter in Filter::ALL {
            let count = self.items.iter().filter(|item| self.shown(filter, item)).count();
            let label = format!(" {} {count} ", filter.name());
            spans.push(if filter == self.filter { styled(TAB_ACTIVE, label) } else { dim(label) });
        }
        Line::from(spans)
    }

    /// The query, or how to start one.
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
            Row::Theme(i) => &self.items[i],
        };
        let marker = if item.applied {
            ok("● ")
        } else if item.user {
            warn("★ ")
        } else {
            raw("  ")
        };
        let name = pad(&clip_str(&item.slug, name_width), name_width);
        let name = if selected { styled(SELECTED, name) } else { raw(name) };
        let mut spans = vec![marker, name, raw(" ")];
        spans.extend(swatch(self.looks.get(&item.slug)));
        row(Line::from(spans), selected)
    }

    /// Frames the preview of item in its own accent color.
    fn card(&self, item: &Item) -> Vec<Line<'static>> {
        let Some(look) = self.looks.get(&item.slug) else {
            return vec![line(error(format!("  cannot read the palette for {}", item.slug)))];
        };
        let mut title = vec![heading(item.slug.clone()), dim(if look.dark { " · dark" } else { " · light" })];
        if item.applied {
            title.push(ok(" · ● in use"));
        }
        if item.user {
            title.push(warn(" · ★ yours"));
        }
        framed(Line::from(title), look, self.show_tokens)
    }

    /// A user theme's tokens beside a live preview.
    fn editor_view(&self, width: usize, height: usize) -> Vec<Line<'static>> {
        let mut lines = vec![line(heading(format!("Editing {}", self.edit_slug))), Line::default()];
        for (i, r) in self.edit_rows.iter().enumerate() {
            let mut spans = vec![paint(&r.color, &r.color, "  "), raw(format!(" {:<14} {}", r.token, r.color))];
            if r.overridden {
                spans.push(accent(" *"));
            }
            lines.push(row(Line::from(spans), i == self.edit_cursor));
        }
        if self.mode == Mode::EditingColor {
            lines.extend([Line::default(), self.input.view()]);
        } else if let Some(r) = self.edit_rows.get(self.edit_cursor) {
            lines.extend([Line::default(), line(dim(format!("{} · {}", r.origin, r.role)))]);
        }
        if width < 90 {
            return lines;
        }
        let t = &self.edit_tokens;
        let get = |k: &str| t.get(k).cloned().unwrap_or_default();
        let mut look = Look { tokens: t.clone(), background: get("panel_bg"), foreground: get("text"), dark: theme::is_dark(&get("panel_bg")), ..Default::default() };
        for (i, token) in ["surface0", "red", "green", "yellow", "blue", "mauve", "teal", "text"].into_iter().enumerate() {
            (look.ansi[i], look.ansi[i + 8]) = (get(token), get(token));
        }
        let mut title = vec![heading(self.edit_slug.clone()), dim(" · editing")];
        if self.edit_dirty {
            title.push(warn(" · unsaved"));
        }
        columns(lines, 38, framed(Line::from(title), &look, false), height)
    }
}

/// A palette's 16 colors in 8 cells: normal over bright.
fn swatch(look: Option<&Look>) -> Vec<Span<'static>> {
    match look {
        Some(look) => (0..8).map(|i| paint(&look.ansi[i + 8], &look.ansi[i], "▀")).collect(),
        None => vec![dim("unreadable")],
    }
}

fn framed(title: Line<'static>, look: &Look, tokens: bool) -> Vec<Line<'static>> {
    let t = &look.tokens;
    let get = |k: &str| t.get(k).cloned().unwrap_or_default();
    let mut lines = vec![title, Line::default()];
    lines.extend(if tokens { render_token_table(t) } else { render_mock(t) });
    let (bg, fg) = (&look.background, &look.foreground);
    lines.push(Line::default());
    lines.push(Line::from(vec![
        paint(bg, fg, " $ git push "),
        paint(bg, &get("green"), "✓ done "),
        paint(bg, &get("red"), "✗ 2 errors "),
        paint(bg, &get("yellow"), "⚠ 1 warning "),
        paint(bg, &get("blue"), "→ main "),
        paint(&get("selection_bg"), fg, " selected "),
    ]));
    lines.push(Line::default());
    for row in [0, 8] {
        lines.push(Line::from((row..row + 8).map(|i| paint(&look.ansi[i], &look.ansi[i], "    ")).collect::<Vec<_>>()));
    }
    let accent = get("accent");
    boxed(lines, if accent.is_empty() { Color::Blue } else { rgb(&accent) }, None)
}

impl Page for ThemesPage {
    fn title(&self) -> &'static str {
        "Themes"
    }

    fn init(&mut self, ctx: &mut Ctx) {
        // The client check scans every process, so it runs beside the first
        // frame instead of in front of it.
        ctx.spawn(load_themes);
        ctx.spawn(|| Msg::RemoteOnly(theme::only_remote_clients()));
    }

    fn update(&mut self, msg: &mut Msg, ctx: &mut Ctx) {
        match msg {
            Msg::RemoteOnly(only) => {
                self.remote_only = *only;
                if *only {
                    self.mode = Mode::Browse;
                    self.ask = None;
                }
            }
            Msg::ThemesLoaded { items, looks, warnings, err } => {
                if let Some(e) = err {
                    return ctx.error(e);
                }
                self.items = std::mem::take(items);
                self.looks = std::mem::take(looks);
                self.rebuild();
                if !warnings.is_empty() {
                    ctx.status(warnings.join("; "));
                }
            }
            Msg::ThemeEditorDone(body) => match body {
                Ok(body) => self.ask_name(std::mem::take(body), ctx),
                Err(e) => ctx.error(e),
            },
            _ => {}
        }
    }

    fn key(&mut self, key: &Key, ctx: &mut Ctx) {
        if Confirm::handle(self.ask.take(), self, key, ctx) {
            return;
        }
        match self.mode {
            Mode::Naming => match key.name.as_str() {
                "esc" => {
                    self.mode = Mode::Browse;
                    self.pending_body.clear();
                    ctx.status("not saved");
                }
                "enter" => self.save_new(false, ctx),
                _ => {
                    self.input.handle(key);
                }
            },
            Mode::Editing => self.editor_key(key, ctx),
            Mode::EditingColor => match key.name.as_str() {
                "esc" => self.mode = Mode::Editing,
                "enter" => {
                    let token = self.edit_rows[self.edit_cursor].token;
                    let result = theme::set_override(&self.edit_body, token, &self.input.value()).and_then(|body| self.set_edit_body(body, true));
                    match result {
                        Ok(()) => self.mode = Mode::Editing,
                        Err(e) => ctx.error(e),
                    }
                }
                _ => {
                    self.input.handle(key);
                }
            },
            Mode::Browse if self.remote_only => {}
            Mode::Browse => self.browse_key(key, ctx),
        }
    }

    fn view(&mut self, _sh: &Shared, width: usize, height: usize) -> Vec<Line<'static>> {
        if self.remote_only {
            let mut lines = vec![Line::default()];
            lines.extend(CLIENT_SETUP.lines().map(|l| Line::raw(l.to_string())));
            return lines;
        }
        if matches!(self.mode, Mode::Editing | Mode::EditingColor) {
            return self.editor_view(width, height);
        }
        let left_width = if width < 90 { width } else { 38 };
        let mut list_height = height.saturating_sub(4);
        if self.mode == Mode::Naming {
            list_height = list_height.saturating_sub(2);
        }
        let name_width = left_width.saturating_sub(14);
        let rows = self.list.window(list_height);
        let body = self.list.render(rows, |i, s| self.theme_row(i, s, name_width));
        let mut lines = vec![self.chips(), self.search_line()];
        lines.extend(fit(body, left_width, list_height));
        lines.push(Line::default());
        if self.mode == Mode::Naming {
            lines.extend([self.input.view(), Line::default()]);
        }
        lines.push(line(dim("  palettes: terminalcolors.com → Ghostty")));
        match self.selected_item() {
            Some(item) if width >= 90 => columns(lines, left_width + 1, self.card(item), height),
            _ => lines,
        }
    }

    fn help(&self) -> String {
        if let Some(help) = Confirm::help(&self.ask) {
            return help;
        }
        match self.mode {
            Mode::Naming => return "type a name · ↵ save and apply · esc cancel".into(),
            Mode::Editing => return "↵ change color · s save & apply · esc discard".into(),
            Mode::EditingColor => return "#rrggbb · ↵ set · esc cancel".into(),
            Mode::Browse if self.remote_only => return String::new(),
            Mode::Browse => {}
        }
        let mut help = "type to search · ↵ apply · ←/→ all/dark/light/yours · ctrl+a add · ctrl+v paste".to_string();
        if self.selected_item().is_some_and(|i| i.user) {
            help += " · ctrl+e edit · ctrl+d delete";
        }
        help + " · ? keys"
    }

    fn capturing(&self) -> bool {
        self.mode != Mode::Browse || self.ask.is_some() || self.list.filtering
    }

    /// Takes typed characters for the search, ahead of the app's letter keys.
    /// ? still opens the key sheet, and 1–4 still switch pages until a search
    /// has started.
    fn claims(&self, key: &Key) -> bool {
        if self.mode != Mode::Browse || self.ask.is_some() || self.remote_only {
            return false;
        }
        if key.paste.is_some() {
            return true;
        }
        match key.ch {
            Some('?') | None => false,
            Some('1'..='4') => !self.list.query().is_empty(),
            Some(_) => true,
        }
    }

    fn back(&mut self) -> bool {
        self.list.clear_filter()
    }

    fn sheet(&self) -> Vec<Hint> {
        vec![
            ("a–z …", "type to search; letters in order match too, names rank first"),
            ("⌫ ctrl+u", "delete a character, clear the search"),
            ("↑/↓", "move; the preview follows"),
            ("←/→", "show all, dark, light or your themes"),
            ("↵", "apply the theme and close"),
            ("ctrl+t", "token table instead of the preview"),
            ("esc", "clear the search, then close"),
            ("", "Your themes"),
            ("ctrl+a", "add: write or paste a palette in your editor"),
            ("ctrl+v", "add a palette from the clipboard (terminalcolors.com)"),
            ("ctrl+e", "edit a ★ theme's token colors"),
            ("ctrl+d", "delete a ★ theme"),
            ("ctrl+r", "reload the list"),
        ]
    }
}
