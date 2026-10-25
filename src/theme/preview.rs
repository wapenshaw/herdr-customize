//! A small Herdr workspace drawn in a theme's colors.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use super::{Tokens, parse_rgb, token_origin, token_role};

pub fn rgb(hex: &str) -> Color {
    let (r, g, b) = parse_rgb(hex);
    Color::Rgb(r, g, b)
}

/// text in truecolor on bg; empty colors fall back to black and white.
pub fn paint(bg: &str, fg: &str, text: impl Into<String>) -> Span<'static> {
    let bg = if bg.is_empty() { Color::Black } else { rgb(bg) };
    let fg = if fg.is_empty() { Color::White } else { rgb(fg) };
    Span::styled(text.into(), Style::new().bg(bg).fg(fg))
}

fn fg(color: &str, text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), Style::new().fg(if color.is_empty() { Color::White } else { rgb(color) }))
}

/// Pads or cuts s to n characters.
fn pad(n: usize, s: &str) -> String {
    let mut out: String = s.chars().take(n).collect();
    let len = out.chars().count();
    out.extend(std::iter::repeat_n(' ', n - len));
    out
}

/// Draws the mock, 60 columns wide.
pub fn render_mock(t: &Tokens) -> Vec<Line<'static>> {
    let get = |k: &str| t.get(k).cloned().unwrap_or_default();
    let (panel, text, accent, sidebar) = (get("panel_bg"), get("text"), get("accent"), get("sidebar_bg"));
    let (active_row, selection, blue, green, peach) = (get("active_row_bg"), get("selection_bg"), get("blue"), get("green"), get("peach"));
    let (subtext, surface1, dim, overlay) = (get("subtext0"), get("surface1"), get("surface_dim"), get("overlay0"));
    const SB: usize = 18;
    const PA: usize = 26;
    const PB: usize = 16;
    let sb = |color: &str, s: &str| paint(&sidebar, color, pad(SB, s));
    let sbsel = |color: &str, s: &str| paint(&active_row, color, pad(SB, s));
    let edge = |l: &str, r: &str, w: usize, bg: &str, color: &str| paint(bg, color, format!("{l}{}{r}", "─".repeat(w - 2)));
    let a_row = |color: &str, s: &str| vec![paint(&panel, &accent, "│"), paint(&panel, color, pad(PA - 2, s)), paint(&panel, &accent, "│")];
    let i_row = |color: &str, s: &str| vec![paint(&dim, &overlay, "│"), paint(&dim, color, pad(PB - 2, s)), paint(&dim, &overlay, "│")];
    let line = |parts: Vec<Vec<Span<'static>>>| Line::from(parts.into_iter().flatten().collect::<Vec<_>>());

    let half = SB / 2;
    let blank_panes = paint(&panel, &panel, pad(PA + PB, ""));
    vec![
        line(vec![vec![sb(&subtext, " spaces"), paint(&accent, &panel, " 1 · Shell "), paint(&surface1, &subtext, " 2 · agent "), fg(&overlay, " +")]]),
        line(vec![vec![sbsel(&accent, " ○ space1"), edge("┌", "┐", PA, &panel, &accent), edge("┌", "┐", PB, &dim, &overlay)]]),
        line(vec![vec![sb(&text, "     main")], a_row(&peach, " ~   ✓ 09:31"), i_row(&overlay, "187 command")]),
        line(vec![vec![sb(&subtext, "   space2")], a_row(&text, " 187 command ="), i_row(&subtext, "188 descrip")]),
        line(vec![vec![sb(&overlay, "     main")], a_row(&blue, " 188 command ="), i_row(&subtext, "189 descrip")]),
        line(vec![vec![sb(&sidebar, ""), paint(&panel, &accent, "│"), paint(&selection, &panel, pad(PA - 2, " 229 key = prefix+down")), paint(&panel, &accent, "│")], i_row(&overlay, "")]),
        line(vec![vec![sb(&sidebar, ""), edge("└", "┘", PA, &panel, &accent), edge("└", "┘", PB, &dim, &overlay)]]),
        line(vec![vec![paint(&sidebar, &overlay, pad(half, " new")), paint(&sidebar, &overlay, pad(half, "menu")), blank_panes.clone()]]),
        line(vec![vec![paint(&sidebar, &overlay, pad(half, " agents")), paint(&sidebar, &overlay, pad(half, "priority")), blank_panes]]),
        Line::from(paint(&panel, &subtext, " AGENT ")),
        Line::from(vec![paint(&active_row, &green, " ● "), paint(&active_row, &accent, "claude "), paint(&active_row, &overlay, "herdr-customize")]),
        Line::from(vec![paint(&panel, &overlay, " ○ "), paint(&panel, &subtext, "pi "), paint(&panel, &overlay, "idle")]),
    ]
}

/// The tokens the table explains, most visible first.
const LEGEND: [&str; 15] = ["accent", "active_row_bg", "selection_bg", "overlay0", "surface_dim", "red", "green", "yellow", "blue", "teal", "mauve", "peach", "text", "panel_bg", "sidebar_bg"];

/// The main tokens with their color, source and role.
pub fn render_token_table(t: &Tokens) -> Vec<Line<'static>> {
    let get = |k: &str| t.get(k).cloned().unwrap_or_default();
    let (panel, overlay, subtext) = (get("panel_bg"), get("overlay0"), get("subtext0"));
    let mut lines = vec![Line::from(fg(&subtext, "    token         source          role"))];
    for token in LEGEND {
        let value = t.get(token).filter(|v| !v.is_empty()).cloned().unwrap_or_else(|| panel.clone());
        lines.push(Line::from(vec![
            Span::raw(" "),
            paint(&value, &panel, "  "),
            Span::raw(format!(" {}", pad(14, token))),
            fg(&overlay, pad(16, token_origin(token))),
            fg(&subtext, token_role(token)),
        ]));
    }
    lines
}

/// The CLI preview: label, mock, accent strip and token table.
pub fn render_swatch(tokens: &Tokens, label: &str) -> String {
    let mut lines = vec![Line::from(format!("  {label}")), Line::default()];
    lines.extend(render_mock(tokens));
    lines.push(Line::default());
    let panel = tokens.get("panel_bg").cloned().unwrap_or_default();
    let mut strip = vec![Span::raw(" ")];
    strip.extend(["red", "green", "yellow", "blue", "mauve", "teal", "peach", "accent"].iter().map(|t| paint(tokens.get(t).map(String::as_str).unwrap_or(""), &panel, "  ")));
    lines.push(Line::from(strip));
    lines.push(Line::default());
    lines.extend(render_token_table(tokens));
    to_ansi(&lines)
}

/// Renders styled lines as text with truecolor escapes.
pub fn to_ansi(lines: &[Line]) -> String {
    let mut out = String::new();
    for line in lines {
        for span in &line.spans {
            let mut codes = Vec::new();
            if let Some(Color::Rgb(r, g, b)) = span.style.bg {
                codes.push(format!("48;2;{r};{g};{b}"));
            }
            if let Some(Color::Rgb(r, g, b)) = span.style.fg {
                codes.push(format!("38;2;{r};{g};{b}"));
            }
            if codes.is_empty() {
                out.push_str(&span.content);
            } else {
                out.push_str(&format!("\x1b[{}m{}\x1b[0m", codes.join(";"), span.content));
            }
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::parse_palette;
    use crate::theme::tests::BODY;

    #[test]
    fn mock_is_sixty_columns() {
        let lines = render_mock(&parse_palette(BODY).unwrap().tokens());
        assert_eq!(lines.len(), 12);
        for line in &lines[1..9] {
            assert_eq!(line.width(), 60, "{line:?}");
        }
        assert!(lines.iter().any(|l| l.to_string().contains(" AGENT ")));
    }
}
