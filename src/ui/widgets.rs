//! Drawing helpers and the small widgets every page shares: a text field, a
//! filterable list and a yes/no question.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::LazyLock;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use super::{Ctx, Key};

/* -------------------------------------------------------------- styles */

// Styles use the terminal's own ANSI colors so the popup follows the theme.
pub const DIM: Style = Style::new().fg(Color::DarkGray);
pub const ACCENT: Style = Style::new().fg(Color::Cyan);
pub const OK: Style = Style::new().fg(Color::Green);
pub const WARN: Style = Style::new().fg(Color::Yellow);
pub const ERROR: Style = Style::new().fg(Color::Red);
pub const HEADING: Style = Style::new().add_modifier(Modifier::BOLD);
pub const SELECTED: Style = Style::new().fg(Color::Blue).add_modifier(Modifier::BOLD);
pub const TAB_ACTIVE: Style = Style::new().fg(Color::Black).bg(Color::Blue).add_modifier(Modifier::BOLD);

pub fn styled(style: Style, text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), style)
}
pub fn dim(text: impl Into<String>) -> Span<'static> {
    styled(DIM, text)
}
pub fn accent(text: impl Into<String>) -> Span<'static> {
    styled(ACCENT, text)
}
pub fn ok(text: impl Into<String>) -> Span<'static> {
    styled(OK, text)
}
pub fn warn(text: impl Into<String>) -> Span<'static> {
    styled(WARN, text)
}
pub fn error(text: impl Into<String>) -> Span<'static> {
    styled(ERROR, text)
}
pub fn heading(text: impl Into<String>) -> Span<'static> {
    styled(HEADING, text)
}
pub fn raw(text: impl Into<String>) -> Span<'static> {
    Span::raw(text.into())
}

/// A line of one span.
pub fn line(span: Span<'static>) -> Line<'static> {
    Line::from(span)
}

/* --------------------------------------------------------------- layout */

pub fn width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

/// Cuts a string to n columns, ending in … when it was longer.
pub fn clip_str(s: &str, n: usize) -> String {
    if width(s) <= n {
        return s.to_string();
    }
    if n == 0 {
        return String::new();
    }
    take_width(s, n - 1) + "…"
}

/// The longest prefix of s that fits in room columns.
fn take_width(s: &str, room: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > room {
            break;
        }
        out.push(c);
        used += w;
    }
    out
}

/// Pads a string with spaces to n columns.
pub fn pad(s: &str, n: usize) -> String {
    format!("{s}{}", " ".repeat(n.saturating_sub(width(s))))
}

/// Cuts a line to n columns, keeping each span's style.
pub fn clip(line: Line<'static>, n: usize) -> Line<'static> {
    if line.width() <= n {
        return line;
    }
    let mut spans = Vec::new();
    let mut used = 0;
    for span in line.spans {
        let w = span.width();
        if used + w <= n.saturating_sub(1) {
            used += w;
            spans.push(span);
            continue;
        }
        let room = n.saturating_sub(1) - used;
        spans.push(Span::styled(take_width(&span.content, room), span.style));
        break;
    }
    if n > 0 {
        spans.push(Span::raw("…"));
    }
    Line::from(spans)
}

/// Pads a line with spaces to n columns.
pub fn pad_line(mut line: Line<'static>, n: usize) -> Line<'static> {
    let w = line.width();
    if w < n {
        line.spans.push(Span::raw(" ".repeat(n - w)));
    }
    line
}

/// Clips lines to width and pads or cuts them to exactly height lines.
pub fn fit(mut lines: Vec<Line<'static>>, width: usize, height: usize) -> Vec<Line<'static>> {
    lines.truncate(height);
    let mut out: Vec<Line<'static>> = lines.into_iter().map(|l| clip(l, width)).collect();
    out.resize(height, Line::default());
    out
}

/// Joins two blocks side by side; the left one is padded to left_width.
pub fn columns(left: Vec<Line<'static>>, left_width: usize, right: Vec<Line<'static>>, height: usize) -> Vec<Line<'static>> {
    let mut right = right.into_iter();
    fit(left, left_width, height)
        .into_iter()
        .map(|l| {
            let mut l = pad_line(l, left_width);
            if let Some(r) = right.next() {
                l.spans.extend(r.spans);
            }
            l
        })
        .collect()
}

/// Frames lines in a rounded border with one column of padding. The inner
/// width is the widest line unless given.
pub fn boxed(lines: Vec<Line<'static>>, color: Color, inner: Option<usize>) -> Vec<Line<'static>> {
    let inner = inner.unwrap_or_else(|| lines.iter().map(Line::width).max().unwrap_or(0));
    let border = Style::new().fg(color);
    let mut out = vec![Line::from(Span::styled(format!("╭{}╮", "─".repeat(inner + 2)), border))];
    for l in lines {
        let mut spans = vec![Span::styled("│ ", border)];
        spans.extend(pad_line(clip(l, inner), inner).spans);
        spans.push(Span::styled(" │", border));
        out.push(Line::from(spans));
    }
    out.push(Line::from(Span::styled(format!("╰{}╯", "─".repeat(inner + 2)), border)));
    out
}

/// Splits text into lines of at most width columns.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(20);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// A list row with the selection marker.
pub fn row(mut text: Line<'static>, selected: bool) -> Line<'static> {
    let marker = if selected { styled(SELECTED, "› ") } else { raw("  ") };
    text.spans.insert(0, marker);
    text
}

pub fn first_non_empty<'a>(values: &[&'a str]) -> &'a str {
    values.iter().find(|v| !v.is_empty()).copied().unwrap_or("")
}

/* ----------------------------------------------------------- text input */

/// A one-line text field.
#[derive(Debug, Clone, Default)]
pub struct TextInput {
    pub prompt: String,
    value: Vec<char>,
    pos: usize,
    focused: bool,
}

impl TextInput {
    pub fn new(prompt: impl Into<String>, value: &str) -> TextInput {
        let mut t = TextInput { prompt: prompt.into(), focused: true, ..Default::default() };
        t.set_value(value);
        t
    }

    pub fn value(&self) -> String {
        self.value.iter().collect()
    }

    /// Replaces the text and moves the cursor to its end.
    pub fn set_value(&mut self, value: &str) {
        self.value = value.chars().collect();
        self.pos = self.value.len();
    }

    pub fn focus(&mut self) {
        self.focused = true;
    }

    pub fn blur(&mut self) {
        self.focused = false;
    }

    fn insert(&mut self, text: &str) {
        for c in text.chars() {
            // Pasted line breaks and tabs become spaces.
            let c = if c.is_control() { ' ' } else { c };
            self.value.insert(self.pos, c);
            self.pos += 1;
        }
    }

    /// Edits the text. Reports whether the key was used.
    pub fn handle(&mut self, key: &Key) -> bool {
        if let Some(text) = &key.paste {
            self.insert(text);
            return true;
        }
        if let Some(c) = key.ch {
            self.insert(&c.to_string());
            return true;
        }
        let word_start = |v: &[char], mut i: usize| {
            while i > 0 && v[i - 1] == ' ' {
                i -= 1;
            }
            while i > 0 && v[i - 1] != ' ' {
                i -= 1;
            }
            i
        };
        match key.name.as_str() {
            "backspace" | "ctrl+h" if self.pos > 0 => {
                self.pos -= 1;
                self.value.remove(self.pos);
            }
            "delete" | "ctrl+d" if self.pos < self.value.len() => {
                self.value.remove(self.pos);
            }
            "left" | "ctrl+b" => self.pos = self.pos.saturating_sub(1),
            "right" | "ctrl+f" => self.pos = (self.pos + 1).min(self.value.len()),
            "home" | "ctrl+a" => self.pos = 0,
            "end" | "ctrl+e" => self.pos = self.value.len(),
            "ctrl+u" => {
                self.value.drain(..self.pos);
                self.pos = 0;
            }
            "ctrl+k" => self.value.truncate(self.pos),
            "ctrl+w" | "alt+backspace" => {
                let start = word_start(&self.value, self.pos);
                self.value.drain(start..self.pos);
                self.pos = start;
            }
            "alt+left" | "ctrl+left" => self.pos = word_start(&self.value, self.pos),
            "alt+right" | "ctrl+right" => {
                while self.pos < self.value.len() && self.value[self.pos] == ' ' {
                    self.pos += 1;
                }
                while self.pos < self.value.len() && self.value[self.pos] != ' ' {
                    self.pos += 1;
                }
            }
            "backspace" | "ctrl+h" | "delete" | "ctrl+d" => {}
            _ => return false,
        }
        true
    }

    pub fn view(&self) -> Line<'static> {
        let mut spans = vec![raw(self.prompt.clone())];
        let before: String = self.value[..self.pos].iter().collect();
        spans.push(raw(before));
        if self.focused {
            let at = self.value.get(self.pos).map_or(" ".to_string(), |c| c.to_string());
            spans.push(styled(Style::new().add_modifier(Modifier::REVERSED), at));
            spans.push(raw(self.value.iter().skip(self.pos + 1).collect::<String>()));
        } else {
            spans.push(raw(self.value[self.pos..].iter().collect::<String>()));
        }
        Line::from(spans)
    }
}

/* ------------------------------------------------------------------ list */

/// A scrolling, filterable selection over rows.
#[derive(Debug, Default)]
pub struct List {
    labels: Vec<String>,
    /// Rows that title a group; the cursor skips them and a search hides them.
    headers: Vec<bool>,
    pub matches: Vec<usize>,
    pub cursor: usize,
    offset: usize,
    pub filter: TextInput,
    pub filtering: bool,
    /// Leaves showing the query to the page.
    pub hide_filter: bool,
}

impl List {
    pub fn new() -> List {
        List { filter: TextInput { prompt: "/".into(), ..Default::default() }, ..Default::default() }
    }

    /// Replaces the rows, keeping the selected label when it still exists.
    /// headers marks group titles.
    pub fn set_rows(&mut self, labels: Vec<String>, headers: Vec<bool>) {
        let keep = self.selected().and_then(|i| self.labels.get(i).cloned());
        self.labels = labels;
        self.headers = headers;
        self.refilter();
        if let Some(keep) = keep
            && let Some(c) = self.matches.iter().position(|&i| self.labels[i] == keep)
        {
            self.cursor = c;
        }
    }

    fn is_header(&self, i: usize) -> bool {
        self.headers.get(i).copied().unwrap_or(false)
    }

    pub fn query(&self) -> String {
        self.filter.value()
    }

    fn refilter(&mut self) {
        let query = self.filter.value().trim().to_string();
        self.matches = if query.is_empty() {
            (0..self.labels.len()).collect()
        } else {
            let candidates: Vec<usize> = (0..self.labels.len()).filter(|&i| !self.is_header(i)).collect();
            fzf_rank(&self.labels, &candidates, &query).unwrap_or_else(|| text_rank(&self.labels, &candidates, &query))
        };
        self.cursor = self.cursor.min(self.matches.len().saturating_sub(1));
        self.skip_headers(1);
    }

    /// Searches for query, starting at the best match.
    pub fn set_query(&mut self, query: &str) {
        if query == self.filter.value() {
            return;
        }
        self.filter.set_value(query);
        self.cursor = 0;
        self.refilter();
    }

    /// Moves the cursor off a header row, in direction dir first.
    pub fn skip_headers(&mut self, dir: isize) {
        for d in [dir, -dir] {
            let mut c = self.cursor as isize;
            while c >= 0 && (c as usize) < self.matches.len() && self.is_header(self.matches[c as usize]) {
                c += d;
            }
            if c >= 0 && (c as usize) < self.matches.len() {
                self.cursor = c as usize;
                return;
            }
        }
    }

    /// Drops an applied filter and reports whether there was one. The
    /// selected row stays selected.
    pub fn clear_filter(&mut self) -> bool {
        if self.filter.value().is_empty() {
            return false;
        }
        let keep = self.selected();
        self.filter.set_value("");
        self.refilter();
        if let Some(i) = keep {
            self.select_row(i);
        }
        true
    }

    /// The selected row index.
    pub fn selected(&self) -> Option<usize> {
        self.matches.get(self.cursor).copied()
    }

    /// Moves the cursor to row i when it is visible.
    pub fn select_row(&mut self, i: usize) {
        if let Some(c) = self.matches.iter().position(|&m| m == i) {
            self.cursor = c;
        }
    }

    /// Moves the cursor or edits the filter. Reports whether the key was used.
    pub fn handle(&mut self, key: &Key) -> bool {
        if self.filtering {
            match key.name.as_str() {
                "enter" => {
                    self.filtering = false;
                    self.filter.blur();
                    return true;
                }
                "esc" => {
                    self.filtering = false;
                    self.filter.blur();
                    self.clear_filter();
                    return true;
                }
                "up" | "down" | "ctrl+p" | "ctrl+n" => {}
                _ => {
                    let before = self.filter.value();
                    self.filter.handle(key);
                    if self.filter.value() != before {
                        // A new search starts at its best match.
                        self.cursor = 0;
                    }
                    self.refilter();
                    return true;
                }
            }
        }
        let last = self.matches.len().saturating_sub(1);
        let mut dir = 1;
        match key.name.as_str() {
            "/" => {
                self.filtering = true;
                self.filter.focus();
                return true;
            }
            "up" | "k" | "ctrl+p" => (self.cursor, dir) = (self.cursor.saturating_sub(1), -1),
            "down" | "j" | "ctrl+n" => self.cursor = (self.cursor + 1).min(last),
            "pgup" | "ctrl+u" => (self.cursor, dir) = (self.cursor.saturating_sub(10), -1),
            "pgdown" | "ctrl+d" => self.cursor = (self.cursor + 10).min(last),
            "home" | "g" => self.cursor = 0,
            "end" | "G" => (self.cursor, dir) = (last, -1),
            _ => return false,
        }
        self.skip_headers(dir);
        true
    }

    /// Scrolls so the cursor is visible in height lines and returns the rows
    /// to draw, with whether each is selected. Call render with them.
    pub fn window(&mut self, height: usize) -> Vec<(usize, bool)> {
        let height = (height - usize::from(self.shows_filter()).min(height)).max(1);
        if self.cursor < self.offset {
            self.offset = self.cursor;
        }
        if self.cursor >= self.offset + height {
            self.offset = self.cursor + 1 - height;
        }
        self.offset = self.offset.min(self.matches.len().saturating_sub(height));
        (self.offset..self.matches.len().min(self.offset + height)).map(|c| (self.matches[c], c == self.cursor)).collect()
    }

    fn shows_filter(&self) -> bool {
        !self.hide_filter && (self.filtering || !self.filter.value().is_empty())
    }

    /// The filter line, when one is active, and the rendered rows.
    pub fn render(&self, rows: Vec<(usize, bool)>, mut draw: impl FnMut(usize, bool) -> Line<'static>) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        if self.shows_filter() {
            lines.push(self.filter.view());
        }
        lines.extend(rows.into_iter().map(|(i, s)| draw(i, s)));
        if self.matches.is_empty() {
            lines.push(line(dim("  no matches")));
        }
        lines
    }
}

/// The fzf that ranks searches; without one, text_rank does.
pub static FZF: LazyLock<Option<std::path::PathBuf>> = LazyLock::new(|| crate::proc::look_path("fzf"));

/// Orders the candidate rows matching query by fzf's score, with fzf's own
/// matching rules. None when fzf is missing or fails.
fn fzf_rank(labels: &[String], candidates: &[usize], query: &str) -> Option<Vec<usize>> {
    let fzf = FZF.as_ref()?;
    let mut child = Command::new(fzf)
        .args(["--filter", query, "--delimiter", "\t", "--nth", "2.."])
        // The user's defaults could change the output; matching stays fzf's.
        .env("FZF_DEFAULT_OPTS", "")
        .env("FZF_DEFAULT_OPTS_FILE", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let input: String = candidates.iter().map(|&i| format!("{i}\t{}\n", labels[i].replace('\n', " "))).collect();
    let mut stdin = child.stdin.take()?;
    // Written from a thread so a full pipe cannot stall both sides.
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let out = child.wait_with_output().ok()?;
    let _ = writer.join();
    match out.status.code() {
        Some(0) => {}
        Some(1) => return Some(Vec::new()), // no match
        _ => return None,
    }
    Some(String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.split('\t').next()?.parse().ok()).filter(|&i: &usize| i < labels.len()).collect())
}

/// The search without fzf: every word of query must appear in the label,
/// best on the row's name.
pub fn text_rank(labels: &[String], candidates: &[usize], query: &str) -> Vec<usize> {
    let query = query.to_lowercase();
    let terms: Vec<&str> = query.split_whitespace().collect();
    let mut ranked: Vec<(usize, usize)> = candidates.iter().filter_map(|&i| match_rank(&labels[i].to_lowercase(), &terms).map(|r| (r, i))).collect();
    ranked.sort_by_key(|&(r, _)| r);
    ranked.into_iter().map(|(_, i)| i).collect()
}

/// How well s matches every term: 0 when each term is in s's first word (the
/// row's name), 1 when each is somewhere in s, 2 when some term only matches
/// the name loosely, as a subsequence. None is no match.
fn match_rank(s: &str, terms: &[&str]) -> Option<usize> {
    let name = s.split(' ').next().unwrap_or("");
    let mut rank = 0;
    for term in terms {
        if name.contains(term) {
            continue;
        }
        if s.contains(term) {
            rank = rank.max(1);
        } else if subsequence(name, term) {
            rank = 2;
        } else {
            return None;
        }
    }
    Some(rank)
}

fn subsequence(s: &str, term: &str) -> bool {
    let mut rest = s.chars();
    term.chars().all(|c| rest.any(|r| r == c))
}

/* --------------------------------------------------------------- confirm */

/// A yes/no question shown in place of the help line. yes gets the page back
/// so it can act on it.
pub struct Confirm<P> {
    pub question: String,
    pub yes: Yes<P>,
}

pub type Yes<P> = Box<dyn FnOnce(&mut P, &mut Ctx)>;

impl<P> Confirm<P> {
    pub fn new(question: impl Into<String>, yes: impl FnOnce(&mut P, &mut Ctx) + 'static) -> Confirm<P> {
        Confirm { question: question.into(), yes: Box::new(yes) }
    }

    /// Answers the question; any key but y cancels.
    pub fn answer(self, page: &mut P, key: &Key, ctx: &mut Ctx) {
        if key.name.eq_ignore_ascii_case("y") {
            (self.yes)(page, ctx);
        } else {
            ctx.status("cancelled");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_rank_prefers_names() {
        let labels: Vec<String> = ["dracula a vampire", "nord cold", "catppuccin-mocha warm", "vampire-light dracula"].iter().map(|s| s.to_string()).collect();
        assert_eq!(text_rank(&labels, &[0, 1, 2, 3], "dracula"), [0, 3]);
        assert_eq!(text_rank(&labels, &[0, 1, 2, 3], "cmoc"), [2]);
        assert!(text_rank(&labels, &[0, 1, 2, 3], "zzz").is_empty());
    }

    #[test]
    fn clip_keeps_width() {
        let l = Line::from(vec![accent("abc"), raw("défgh")]);
        let c = clip(l, 5);
        assert_eq!(c.width(), 5);
        assert_eq!(c.spans.last().unwrap().content, "…");
        assert_eq!(clip_str("hello", 3), "he…");
    }

    #[test]
    fn list_skips_headers() {
        let mut l = List::new();
        l.set_rows(vec!["Group".into(), "a".into(), "b".into()], vec![true, false, false]);
        assert_eq!(l.selected(), Some(1));
        l.handle(&Key::named("up"));
        assert_eq!(l.selected(), Some(1));
        l.handle(&Key::named("down"));
        assert_eq!(l.selected(), Some(2));
    }
}
