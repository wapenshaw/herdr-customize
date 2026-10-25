// Package ui is the herdr-customize popup: one Bubble Tea program with a page
// per area (themes, plugins, keys, settings).
package ui

import (
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

// Options selects what the popup shows first.
type Options struct {
	// Tab is "themes", "plugins", "keys" or "settings".
	Tab string
	// EditTheme opens the token editor for this user theme.
	EditTheme string
}

// page is one tab of the popup.
type page interface {
	title() string
	init() tea.Cmd
	update(msg tea.Msg) tea.Cmd
	view(width, height int) string
	help() string
	// capturing reports a text field, prompt or sub-view that needs every key,
	// including the ones that otherwise switch pages or quit.
	capturing() bool
}

type app struct {
	shared        *shared
	pages         []page
	active        int
	width, height int
	status        string
	statusErr     bool
	// after runs once the popup has closed, with the terminal restored.
	after func() error
}

// Messages shared by every page.
type (
	statusMsg struct {
		text string
		err  bool
	}
	quitThenMsg struct{ fn func() error }
	// gotoMsg switches to the named page and then delivers msg to it.
	gotoMsg struct {
		page string
		msg  tea.Msg
	}
)

func setStatus(text string) tea.Cmd {
	return func() tea.Msg { return statusMsg{text: text} }
}

func setError(err error) tea.Cmd {
	return func() tea.Msg { return statusMsg{text: err.Error(), err: true} }
}

// quitThen closes the popup and runs fn afterwards.
func quitThen(fn func() error) tea.Cmd {
	return func() tea.Msg { return quitThenMsg{fn: fn} }
}

func goTo(name string, msg tea.Msg) tea.Cmd {
	return func() tea.Msg { return gotoMsg{page: name, msg: msg} }
}

// Run shows the popup until the user leaves it.
func Run(opts Options) error {
	a := newApp(opts)
	final, err := tea.NewProgram(a, tea.WithAltScreen()).Run()
	if err != nil {
		return err
	}
	if after := final.(*app).after; after != nil {
		return after()
	}
	return nil
}

func newApp(opts Options) *app {
	sh := newShared()
	a := &app{shared: sh, pages: []page{
		newThemesPage(opts.EditTheme),
		newPluginsPage(sh),
		newKeysPage(sh),
		newSettingsPage(sh),
	}}
	for i, p := range a.pages {
		if strings.EqualFold(p.title(), opts.Tab) {
			a.active = i
		}
	}
	return a
}

func (a *app) Init() tea.Cmd {
	cmds := []tea.Cmd{loadReference, loadPlugins}
	for _, p := range a.pages {
		cmds = append(cmds, p.init())
	}
	return tea.Batch(cmds...)
}

func (a *app) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		a.width, a.height = msg.Width, msg.Height
		return a, nil
	case statusMsg:
		a.status, a.statusErr = msg.text, msg.err
		return a, nil
	case referenceMsg:
		a.shared.ref = msg.ref
	case savedMsg:
		a.status, a.statusErr = msg.describe()
	case quitThenMsg:
		a.after = msg.fn
		return a, tea.Quit
	case gotoMsg:
		for i, p := range a.pages {
			if strings.EqualFold(p.title(), msg.page) {
				a.active = i
				if msg.msg != nil {
					return a, p.update(msg.msg)
				}
			}
		}
		return a, nil
	case tea.KeyMsg:
		// Keys that arrive together come as one message ("jj"); handle them
		// one at a time so each still reaches its binding. Pastes stay whole.
		if msg.Type == tea.KeyRunes && len(msg.Runes) > 1 && !msg.Paste {
			var cmds []tea.Cmd
			for _, r := range msg.Runes {
				_, cmd := a.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{r}, Alt: msg.Alt})
				cmds = append(cmds, cmd)
			}
			return a, tea.Sequence(cmds...)
		}
		a.status = ""
		if msg.String() == "ctrl+c" {
			return a, tea.Quit
		}
		current := a.pages[a.active]
		// A page can take a key that would otherwise switch pages.
		if c, ok := current.(interface{ claims(tea.KeyMsg) bool }); ok && c.claims(msg) {
			return a, current.update(msg)
		}
		if !current.capturing() {
			switch msg.String() {
			case "esc":
				if b, ok := current.(interface{ back() bool }); ok && b.back() {
					return a, nil
				}
				return a, tea.Quit
			case "q":
				return a, tea.Quit
			// Arrows belong to the page; only Tab and the digits switch.
			case "tab":
				a.active = (a.active + 1) % len(a.pages)
				return a, nil
			case "shift+tab":
				a.active = (a.active + len(a.pages) - 1) % len(a.pages)
				return a, nil
			case "1", "2", "3", "4":
				if n := int(msg.String()[0] - '1'); n < len(a.pages) {
					a.active = n
				}
				return a, nil
			}
		}
		return a, current.update(msg)
	}
	// Results of background work go to every page; each ignores the others'.
	var cmds []tea.Cmd
	for _, p := range a.pages {
		cmds = append(cmds, p.update(msg))
	}
	return a, tea.Batch(cmds...)
}

func (a *app) View() string {
	if a.width == 0 {
		return ""
	}
	var tabs []string
	for i, p := range a.pages {
		label := " " + string(rune('1'+i)) + " " + p.title() + " "
		if i == a.active {
			tabs = append(tabs, styleTabActive.Render(label))
		} else {
			tabs = append(tabs, styleTab.Render(label))
		}
	}
	header := lipgloss.JoinHorizontal(lipgloss.Top, tabs...)
	current := a.pages[a.active]
	help := current.help()
	if !current.capturing() {
		help += " · tab next page · q quit"
	}
	status := ""
	if a.status != "" {
		if a.statusErr {
			status = styleError.Render("✗ " + a.status)
		} else {
			status = styleOK.Render(a.status)
		}
	}
	bodyHeight := max(a.height-4, 1)
	body := current.view(a.width, bodyHeight)
	body = fit(body, a.width, bodyHeight)
	return header + "\n" + body + "\n" + clip(status, a.width) + "\n" + styleHelp.Render(clip(help, a.width))
}
