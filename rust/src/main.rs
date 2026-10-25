//! Rust test run of the Themes page, for comparing startup time and memory
//! with the Go build. It loads the same bundled palettes, maps them to the
//! same tokens and draws the list, search line and Herdr mock.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::{DefaultTerminal, Frame};

struct Look {
    slug: String,
    dark: bool,
    ansi: [Color; 16],
    tokens: HashMap<&'static str, Color>,
}

fn hex(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn rgb((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb(r, g, b)
}

fn darken((r, g, b): (u8, u8, u8), percent: u32) -> (u8, u8, u8) {
    let f = 100 - percent;
    (
        (r as u32 * f / 100) as u8,
        (g as u32 * f / 100) as u8,
        (b as u32 * f / 100) as u8,
    )
}

fn load(path: &Path, slug: &str) -> Option<Look> {
    let text = std::fs::read_to_string(path).ok()?;
    let (mut bg, mut fg, mut sel) = (None, None, None);
    let mut pal = [None; 16];
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        match k.trim() {
            "background" => bg = hex(v),
            "foreground" => fg = hex(v),
            "selection-background" => sel = hex(v),
            "palette" => {
                if let Some((i, c)) = v.split_once('=') {
                    if let Ok(i) = i.trim().parse::<usize>() {
                        if i < 16 {
                            pal[i] = hex(c);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let (bg, fg) = (bg?, fg?);
    let p = |i: usize| pal[i].unwrap_or(fg);
    let sel = sel.unwrap_or(p(8));
    let peach = pal[9].unwrap_or(p(3));
    let tokens = HashMap::from([
        ("panel_bg", rgb(bg)),
        ("sidebar_bg", rgb(darken(bg, 15))),
        ("active_row_bg", rgb(p(0))),
        ("selection_bg", rgb(sel)),
        ("surface1", rgb(p(8))),
        ("surface_dim", rgb(darken(bg, 8))),
        ("overlay0", rgb(p(8))),
        ("text", rgb(fg)),
        ("subtext0", rgb(p(7))),
        ("accent", rgb(p(4))),
        ("green", rgb(p(2))),
        ("blue", rgb(p(4))),
        ("peach", rgb(peach)),
    ]);
    let lum = 0.299 * bg.0 as f64 + 0.587 * bg.1 as f64 + 0.114 * bg.2 as f64;
    let mut ansi = [Color::Reset; 16];
    for (i, c) in ansi.iter_mut().enumerate() {
        *c = rgb(p(i));
    }
    Some(Look { slug: slug.to_string(), dark: lum < 128.0, ansi, tokens })
}

fn root() -> PathBuf {
    if let Ok(r) = std::env::var("HERDR_PLUGIN_ROOT") {
        return PathBuf::from(r);
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_default()
}

fn load_all() -> Vec<Look> {
    let dir = root().join("themes");
    let index = std::fs::read_to_string(dir.join("index.txt")).unwrap_or_default();
    index
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|slug| load(&dir.join(slug), slug))
        .collect()
}

struct App {
    looks: Vec<Look>,
    query: String,
    matches: Vec<usize>,
    cursor: usize,
    filter: usize,
}

const FILTERS: [&str; 3] = ["All", "Dark", "Light"];

impl App {
    fn refilter(&mut self) {
        let q = self.query.to_lowercase();
        self.matches = (0..self.looks.len())
            .filter(|&i| {
                let l = &self.looks[i];
                (self.filter == 0 || (self.filter == 1) == l.dark) && l.slug.contains(&q)
            })
            .collect();
        self.cursor = self.cursor.min(self.matches.len().saturating_sub(1));
    }
}

fn main() -> io::Result<()> {
    let mut app = App { looks: load_all(), query: String::new(), matches: vec![], cursor: 0, filter: 0 };
    app.refilter();
    let mut terminal = ratatui::init();
    let chosen = run(&mut terminal, &mut app);
    ratatui::restore();
    if let Some(slug) = chosen? {
        println!("{slug}");
    }
    Ok(())
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<Option<String>> {
    loop {
        terminal.draw(|f| draw(f, app))?;
        let Event::Key(key) = event::read()? else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Esc if app.query.is_empty() => return Ok(None),
            KeyCode::Esc => app.query.clear(),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(None),
            KeyCode::Enter => return Ok(app.matches.get(app.cursor).map(|&i| app.looks[i].slug.clone())),
            KeyCode::Up => app.cursor = app.cursor.saturating_sub(1),
            KeyCode::Down => app.cursor = (app.cursor + 1).min(app.matches.len().saturating_sub(1)),
            KeyCode::Left => app.filter = (app.filter + FILTERS.len() - 1) % FILTERS.len(),
            KeyCode::Right => app.filter = (app.filter + 1) % FILTERS.len(),
            KeyCode::Backspace => {
                app.query.pop();
            }
            KeyCode::Char(c) => {
                app.query.push(c);
                app.cursor = 0;
            }
            _ => {}
        }
        app.refilter();
    }
}

fn draw(f: &mut Frame, app: &App) {
    let dim = Style::new().fg(Color::DarkGray);
    let [top, search, body, help] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(f.area());

    let chips: Vec<Span> = FILTERS
        .iter()
        .enumerate()
        .flat_map(|(i, name)| {
            let style = if i == app.filter { Style::new().add_modifier(Modifier::REVERSED) } else { dim };
            [Span::styled(format!(" {name} "), style), Span::raw(" ")]
        })
        .collect();
    f.render_widget(Line::from(chips), top);
    let search_line = if app.query.is_empty() {
        Line::styled("⌕ type to search", dim)
    } else {
        Line::from(format!("⌕ {}  ({} matches)", app.query, app.matches.len()))
    };
    f.render_widget(search_line, search);
    f.render_widget(Line::styled("type to search · ↵ apply · ←/→ filter · esc close", dim), help);

    let [left, right] = Layout::horizontal([Constraint::Length(38), Constraint::Fill(1)]).areas(body);
    let height = left.height as usize;
    let start = app.cursor.saturating_sub(height.saturating_sub(1));
    let rows: Vec<Line> = app
        .matches
        .iter()
        .enumerate()
        .skip(start)
        .take(height)
        .map(|(n, &i)| {
            let l = &app.looks[i];
            let mut spans = vec![Span::raw(if n == app.cursor { "› " } else { "  " })];
            spans.extend((1..7).map(|c| Span::styled("▀", Style::new().fg(l.ansi[c]).bg(l.ansi[c + 8]))));
            let style = if n == app.cursor { Style::new().add_modifier(Modifier::BOLD) } else { Style::new() };
            spans.push(Span::styled(format!(" {}", l.slug), style));
            Line::from(spans)
        })
        .collect();
    f.render_widget(Paragraph::new(rows), left);

    if let Some(&i) = app.matches.get(app.cursor) {
        let l = &app.looks[i];
        let card = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(l.tokens["accent"]))
            .title(format!(" {} ", l.slug));
        let mut lines = mock(&l.tokens);
        lines.push(Line::raw(""));
        for half in [0, 8] {
            lines.push(Line::from(
                (0..8).map(|c| Span::styled("    ", Style::new().bg(l.ansi[half + c]))).collect::<Vec<_>>(),
            ));
        }
        f.render_widget(Paragraph::new(lines).block(card), right);
    }
}

fn pad(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    let len = out.chars().count();
    out.extend(std::iter::repeat_n(' ', n - len));
    out
}

/// A small Herdr workspace in the theme's colors, the same layout as
/// theme.RenderMock.
fn mock(t: &HashMap<&'static str, Color>) -> Vec<Line<'static>> {
    let c = |bg: &str, fg: &str, s: String| Span::styled(s, Style::new().bg(t[bg]).fg(t[fg]));
    const SB: usize = 18;
    const PA: usize = 26;
    const PB: usize = 16;
    let sb = |fg: &str, s: &str| c("sidebar_bg", fg, pad(s, SB));
    let a_edge = |l: &str, r: &str| c("panel_bg", "accent", format!("{l}{}{r}", "─".repeat(PA - 2)));
    let a_row = |fg: &str, s: &str| {
        vec![c("panel_bg", "accent", "│".into()), c("panel_bg", fg, pad(s, PA - 2)), c("panel_bg", "accent", "│".into())]
    };
    let i_edge = |l: &str, r: &str| c("surface_dim", "overlay0", format!("{l}{}{r}", "─".repeat(PB - 2)));
    let i_row = |fg: &str, s: &str| {
        vec![
            c("surface_dim", "overlay0", "│".into()),
            c("surface_dim", fg, pad(s, PB - 2)),
            c("surface_dim", "overlay0", "│".into()),
        ]
    };
    let join = |parts: Vec<Vec<Span<'static>>>| Line::from(parts.concat());
    let blank = pad("", PA + PB);
    vec![
        Line::from(vec![
            sb("subtext0", " spaces"),
            c("accent", "panel_bg", " 1 · Shell ".into()),
            c("surface1", "subtext0", " 2 · agent ".into()),
        ]),
        Line::from(vec![c("active_row_bg", "accent", pad(" ○ space1", SB)), a_edge("┌", "┐"), i_edge("┌", "┐")]),
        join(vec![vec![sb("text", "     main")], a_row("peach", " ~   ✓ 09:31"), i_row("overlay0", "187 command")]),
        join(vec![vec![sb("subtext0", "   space2")], a_row("text", " 187 command ="), i_row("subtext0", "188 descrip")]),
        join(vec![vec![sb("overlay0", "     main")], a_row("blue", " 188 command ="), i_row("subtext0", "189 descrip")]),
        join(vec![
            vec![sb("sidebar_bg", ""), c("panel_bg", "accent", "│".into())],
            vec![c("selection_bg", "panel_bg", pad(" 229 key = prefix+down", PA - 2)), c("panel_bg", "accent", "│".into())],
            i_row("overlay0", ""),
        ]),
        Line::from(vec![sb("sidebar_bg", ""), a_edge("└", "┘"), i_edge("└", "┘")]),
        Line::from(vec![sb("overlay0", " new      menu"), c("panel_bg", "panel_bg", blank.clone())]),
        Line::from(vec![sb("overlay0", " agents   priority"), c("panel_bg", "panel_bg", blank)]),
        Line::from(vec![c("panel_bg", "subtext0", " AGENT ".into())]),
        Line::from(vec![
            c("active_row_bg", "green", " ● ".into()),
            c("active_row_bg", "accent", "claude ".into()),
            c("active_row_bg", "overlay0", "herdr-customize".into()),
        ]),
    ]
}
