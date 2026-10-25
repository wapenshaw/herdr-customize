package ui

import (
	"strings"
	"unicode"

	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// Styles use the terminal's own ANSI colors so the popup follows the theme.
var (
	styleTab       = lipgloss.NewStyle().Foreground(lipgloss.Color("8"))
	styleTabActive = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("0")).Background(lipgloss.Color("4"))
	styleSelected  = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("4"))
	styleDim       = lipgloss.NewStyle().Foreground(lipgloss.Color("8"))
	styleHelp      = lipgloss.NewStyle().Foreground(lipgloss.Color("8"))
	styleError     = lipgloss.NewStyle().Foreground(lipgloss.Color("1"))
	styleOK        = lipgloss.NewStyle().Foreground(lipgloss.Color("2"))
	styleWarn      = lipgloss.NewStyle().Foreground(lipgloss.Color("3"))
	styleHeading   = lipgloss.NewStyle().Bold(true)
	styleAccent    = lipgloss.NewStyle().Foreground(lipgloss.Color("6"))
)

// clip truncates one line to width cells, keeping ANSI styling intact.
func clip(line string, width int) string {
	return ansi.Truncate(line, width, "…")
}

// fit clips text to width and pads or cuts it to exactly height lines.
func fit(text string, width, height int) string {
	lines := strings.Split(strings.TrimRight(text, "\n"), "\n")
	if len(lines) > height {
		lines = lines[:height]
	}
	for i, line := range lines {
		lines[i] = clip(line, width)
	}
	for len(lines) < height {
		lines = append(lines, "")
	}
	return strings.Join(lines, "\n")
}

// columns joins two blocks side by side; the left one is padded to leftWidth.
func columns(left string, leftWidth int, right string, height int) string {
	l := strings.Split(fit(left, leftWidth, height), "\n")
	r := strings.Split(right, "\n")
	out := make([]string, height)
	for i := range out {
		line := l[i] + strings.Repeat(" ", max(leftWidth-ansi.StringWidth(l[i]), 0))
		if i < len(r) {
			line += r[i]
		}
		out[i] = line
	}
	return strings.Join(out, "\n")
}

func newInput(prompt, value string) textinput.Model {
	in := textinput.New()
	in.Prompt = prompt
	in.SetValue(value)
	in.CursorEnd()
	in.Focus()
	return in
}

// list is a scrolling, filterable selection over n rows.
type list struct {
	labels    []string
	matches   []int
	cursor    int
	offset    int
	filter    textinput.Model
	filtering bool
}

func newList() list {
	in := textinput.New()
	in.Prompt = "/"
	return list{filter: in}
}

// setRows replaces the rows, keeping the selected label when it still exists.
func (l *list) setRows(labels []string) {
	keep := ""
	if i := l.selected(); i >= 0 {
		keep = l.labels[i]
	}
	l.labels = labels
	l.refilter()
	for c, i := range l.matches {
		if labels[i] == keep {
			l.cursor = c
		}
	}
}

func (l *list) refilter() {
	terms := strings.Fields(strings.ToLower(l.filter.Value()))
	l.matches = l.matches[:0]
	for i, label := range l.labels {
		if matchesAll(strings.ToLower(label), terms) {
			l.matches = append(l.matches, i)
		}
	}
	l.cursor = min(l.cursor, max(len(l.matches)-1, 0))
}

// matchesAll reports whether every term appears in s as a subsequence.
func matchesAll(s string, terms []string) bool {
	for _, term := range terms {
		rest := s
		for _, r := range term {
			i := strings.IndexRune(rest, r)
			if i < 0 {
				return false
			}
			rest = rest[i+len(string(r)):]
		}
	}
	return true
}

// clearFilter drops an applied filter and reports whether there was one.
func (l *list) clearFilter() bool {
	if l.filter.Value() == "" {
		return false
	}
	l.filter.SetValue("")
	l.refilter()
	return true
}

// selected returns the selected row index, or -1.
func (l *list) selected() int {
	if l.cursor < len(l.matches) {
		return l.matches[l.cursor]
	}
	return -1
}

// selectRow moves the cursor to row i when it is visible.
func (l *list) selectRow(i int) {
	for c, m := range l.matches {
		if m == i {
			l.cursor = c
		}
	}
}

// handleKey moves the cursor or edits the filter. It reports whether the key
// was used.
func (l *list) handleKey(msg tea.KeyMsg) (bool, tea.Cmd) {
	if l.filtering {
		switch msg.String() {
		case "enter":
			l.filtering = false
			l.filter.Blur()
			return true, nil
		case "esc":
			l.filtering = false
			l.filter.Blur()
			l.filter.SetValue("")
			l.refilter()
			return true, nil
		case "up", "down", "ctrl+p", "ctrl+n":
		default:
			var cmd tea.Cmd
			l.filter, cmd = l.filter.Update(msg)
			l.refilter()
			return true, cmd
		}
	}
	switch msg.String() {
	case "/":
		l.filtering = true
		return true, l.filter.Focus()
	case "up", "k", "ctrl+p":
		l.cursor = max(l.cursor-1, 0)
	case "down", "j", "ctrl+n":
		l.cursor = min(l.cursor+1, max(len(l.matches)-1, 0))
	case "pgup", "ctrl+u":
		l.cursor = max(l.cursor-10, 0)
	case "pgdown", "ctrl+d":
		l.cursor = min(l.cursor+10, max(len(l.matches)-1, 0))
	case "home", "g":
		l.cursor = 0
	case "end", "G":
		l.cursor = max(len(l.matches)-1, 0)
	default:
		return false, nil
	}
	return true, nil
}

// view renders the visible rows plus the filter line when one is active.
func (l *list) view(height int, render func(i int, selected bool) string) string {
	var lines []string
	if l.filtering || l.filter.Value() != "" {
		lines = append(lines, l.filter.View())
		height--
	}
	height = max(height, 1)
	if l.cursor < l.offset {
		l.offset = l.cursor
	}
	if l.cursor >= l.offset+height {
		l.offset = l.cursor - height + 1
	}
	l.offset = min(l.offset, max(len(l.matches)-height, 0))
	for c := l.offset; c < len(l.matches) && c < l.offset+height; c++ {
		lines = append(lines, render(l.matches[c], c == l.cursor))
	}
	if len(l.matches) == 0 {
		lines = append(lines, styleDim.Render("  no matches"))
	}
	return strings.Join(lines, "\n")
}

// row renders a list row with the selection marker.
func row(text string, selected bool) string {
	if selected {
		return styleSelected.Render("› ") + text
	}
	return "  " + text
}

// confirm is a yes/no question shown in place of the help line.
type confirm struct {
	question string
	yes      func() tea.Cmd
}

// handle answers the question; any key but y cancels.
func (c *confirm) handle(msg tea.KeyMsg) tea.Cmd {
	if strings.EqualFold(msg.String(), "y") {
		return c.yes()
	}
	return setStatus("cancelled")
}

func printable(s string) bool {
	for _, r := range s {
		if !unicode.IsPrint(r) {
			return false
		}
	}
	return true
}
