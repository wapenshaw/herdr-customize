package ui

import (
	"errors"
	"fmt"
	"os"
	"strings"

	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"

	"github.com/wapenshaw/herdr-customize/internal/theme"
)

type themesMode int

const (
	themesBrowse themesMode = iota
	themesNaming
	themesConfirm
	themesEditing
	themesEditingColor
)

type themesPage struct {
	items      []theme.Item
	list       list
	mode       themesMode
	input      textinput.Model
	ask        *confirm
	remoteOnly bool
	previews   map[string]string
	// pendingBody is a new palette waiting for a name.
	pendingBody string
	// token editor
	editSlug  string
	editBody  string
	editDirty bool
	editRows  []theme.TokenRow
	editTok   map[string]string
	editCur   int
	startEdit string
	// legacy counts herdr-theme-picker user themes not imported yet.
	legacy int
}

type (
	themesLoadedMsg struct {
		items    []theme.Item
		warnings []string
		legacy   []string
		err      error
	}
	themeEditorDoneMsg struct {
		path string
		err  error
	}
)

func newThemesPage(edit string) *themesPage {
	return &themesPage{list: newList(), previews: map[string]string{}, startEdit: edit}
}

func (p *themesPage) title() string { return "Themes" }

func (p *themesPage) init() tea.Cmd {
	p.remoteOnly = theme.OnlyRemoteClients()
	if p.remoteOnly {
		return nil
	}
	if p.startEdit != "" {
		if err := p.beginEdit(p.startEdit); err != nil {
			return tea.Batch(loadThemes, setError(err))
		}
	}
	return loadThemes
}

func loadThemes() tea.Msg {
	items, warnings, err := theme.Themes()
	return themesLoadedMsg{items: items, warnings: warnings, legacy: theme.LegacyThemes(), err: err}
}

func (p *themesPage) capturing() bool {
	return p.mode != themesBrowse || p.list.filtering
}

func (p *themesPage) back() bool { return p.list.clearFilter() }

func (p *themesPage) help() string {
	switch p.mode {
	case themesNaming:
		return "type a name · ↵ save and apply · esc cancel"
	case themesConfirm:
		return p.ask.question + " [y/N]"
	case themesEditing:
		return "↵ change color · s save & apply · esc discard"
	case themesEditingColor:
		return "#rrggbb · ↵ set · esc cancel"
	}
	if p.remoteOnly {
		return ""
	}
	help := "↵ apply · / search · a add · c add from clipboard · e edit ★ · d delete ★"
	if p.legacy > 0 {
		help += " · I import old themes"
	}
	return help
}

func (p *themesPage) selectedItem() (theme.Item, bool) {
	if i := p.list.selected(); i >= 0 && i < len(p.items) {
		return p.items[i], true
	}
	return theme.Item{}, false
}

func (p *themesPage) update(msg tea.Msg) tea.Cmd {
	switch msg := msg.(type) {
	case themesLoadedMsg:
		if msg.err != nil {
			return setError(msg.err)
		}
		p.items = msg.items
		labels := make([]string, len(msg.items))
		for i, item := range msg.items {
			labels[i] = item.Slug
		}
		p.list.setRows(labels)
		p.previews = map[string]string{}
		p.legacy = len(msg.legacy)
		if len(msg.warnings) > 0 {
			return setStatus(strings.Join(msg.warnings, "; "))
		}
		if p.legacy > 0 {
			return setStatus(fmt.Sprintf("%d user theme(s) from herdr-theme-picker: press I to import them", p.legacy))
		}
		return nil
	case themeEditorDoneMsg:
		defer os.Remove(msg.path)
		if msg.err != nil {
			return setError(fmt.Errorf("editor failed: %w", msg.err))
		}
		data, err := os.ReadFile(msg.path)
		if err != nil {
			return setError(err)
		}
		return p.askName(string(data))
	case tea.KeyMsg:
		return p.handleKey(msg)
	}
	return nil
}

func (p *themesPage) handleKey(msg tea.KeyMsg) tea.Cmd {
	switch p.mode {
	case themesConfirm:
		p.mode = themesBrowse
		ask := p.ask
		p.ask = nil
		return ask.handle(msg)
	case themesNaming:
		switch msg.String() {
		case "esc":
			p.mode, p.pendingBody = themesBrowse, ""
			return setStatus("not saved")
		case "enter":
			return p.saveNew(false)
		}
		var cmd tea.Cmd
		p.input, cmd = p.input.Update(msg)
		return cmd
	case themesEditing:
		return p.handleEditorKey(msg)
	case themesEditingColor:
		switch msg.String() {
		case "esc":
			p.mode = themesEditing
			return nil
		case "enter":
			row := p.editRows[p.editCur]
			body, err := theme.SetOverride(p.editBody, row.Token, p.input.Value())
			if err != nil {
				return setError(err)
			}
			p.mode = themesEditing
			return p.setEditBody(body, true)
		}
		var cmd tea.Cmd
		p.input, cmd = p.input.Update(msg)
		return cmd
	}
	if p.remoteOnly {
		return nil
	}
	if handled, cmd := p.list.handleKey(msg); handled {
		return cmd
	}
	item, ok := p.selectedItem()
	switch msg.String() {
	case "enter":
		if !ok {
			return nil
		}
		slug := item.Slug
		return quitThen(func() error { return theme.ApplyTheme(slug) })
	case "a":
		path, err := theme.NewThemeFile()
		if err != nil {
			return setError(err)
		}
		cmd, err := theme.EditorCommand(path)
		if err != nil {
			os.Remove(path)
			return setError(err)
		}
		return tea.ExecProcess(cmd, func(err error) tea.Msg { return themeEditorDoneMsg{path: path, err: err} })
	case "c":
		clip, err := theme.ReadClipboard()
		if err != nil {
			return setError(err)
		}
		if strings.TrimSpace(clip) == "" {
			return setError(errors.New("clipboard is empty (copy a Ghostty theme from https://terminalcolors.com first)"))
		}
		return p.askName(clip)
	case "e":
		if !ok || !item.User {
			return setError(errors.New("only ★ user-added themes can be edited"))
		}
		if err := p.beginEdit(item.Slug); err != nil {
			return setError(err)
		}
	case "d":
		if !ok || !item.User {
			return setError(errors.New("only ★ user-added themes can be deleted"))
		}
		slug := item.Slug
		p.mode = themesConfirm
		p.ask = &confirm{question: fmt.Sprintf("Delete %q?", slug), yes: func() tea.Cmd {
			if err := theme.RemoveUserTheme(slug); err != nil {
				return setError(err)
			}
			return tea.Batch(loadThemes, setStatus(fmt.Sprintf("deleted %q", slug)))
		}}
	case "r":
		return loadThemes
	case "I":
		if p.legacy == 0 {
			return setStatus("no herdr-theme-picker themes to import")
		}
		imported, err := theme.ImportLegacyThemes()
		if err != nil {
			return tea.Batch(loadThemes, setError(fmt.Errorf("imported %d: %w", len(imported), err)))
		}
		return tea.Batch(loadThemes, setStatus(fmt.Sprintf("imported %d theme(s): %s", len(imported), strings.Join(imported, ", "))))
	}
	return nil
}

// askName validates a new palette, then prompts for its name.
func (p *themesPage) askName(body string) tea.Cmd {
	if err := theme.ValidateThemeBody(body); err != nil {
		return setError(err)
	}
	p.pendingBody = body
	p.mode = themesNaming
	p.input = newInput("Theme name: ", "")
	return textinput.Blink
}

func (p *themesPage) saveNew(overwrite bool) tea.Cmd {
	name := p.input.Value()
	slug, err := theme.SaveUserTheme(name, p.pendingBody, overwrite)
	if errors.Is(err, theme.ErrThemeExists) {
		p.mode = themesConfirm
		p.ask = &confirm{question: fmt.Sprintf("%q already exists. Overwrite?", slug), yes: func() tea.Cmd {
			p.mode = themesNaming
			return p.saveNew(true)
		}}
		return nil
	}
	if err != nil {
		return setError(err)
	}
	p.mode, p.pendingBody = themesBrowse, ""
	return quitThen(func() error { return theme.ApplyTheme(slug) })
}

func (p *themesPage) beginEdit(slug string) error {
	body, err := theme.UserThemeBody(slug)
	if err != nil {
		return err
	}
	p.editSlug, p.editCur = slug, 0
	p.mode = themesEditing
	p.setEditBody(body, false)
	return nil
}

func (p *themesPage) setEditBody(body string, dirty bool) tea.Cmd {
	rows, tokens, err := theme.TokenRows(body)
	if err != nil {
		return setError(err)
	}
	p.editBody, p.editRows, p.editTok = body, rows, tokens
	p.editDirty = p.editDirty || dirty
	return nil
}

func (p *themesPage) handleEditorKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "esc", "q":
		p.mode = themesBrowse
		if p.editDirty {
			p.editDirty = false
			return setStatus("edits discarded")
		}
	case "up", "k":
		p.editCur = max(p.editCur-1, 0)
	case "down", "j":
		p.editCur = min(p.editCur+1, len(p.editRows)-1)
	case "enter":
		p.mode = themesEditingColor
		p.input = newInput(p.editRows[p.editCur].Token+": ", p.editRows[p.editCur].Color)
		return textinput.Blink
	case "s":
		if err := theme.CommitUserTheme(p.editSlug, p.editBody); err != nil {
			return setError(err)
		}
		slug := p.editSlug
		return quitThen(func() error { return theme.ApplyTheme(slug) })
	}
	return nil
}

func (p *themesPage) preview(slug string) string {
	if cached, ok := p.previews[slug]; ok {
		return cached
	}
	rendered := theme.RenderSwatch(slug)
	p.previews[slug] = rendered
	return rendered
}

func (p *themesPage) view(width, height int) string {
	if p.remoteOnly {
		return "\n" + theme.ClientSetup
	}
	leftWidth := min(36, width/3)
	showPreview := width >= 80
	if !showPreview {
		leftWidth = width
	}
	var left, right string
	switch p.mode {
	case themesEditing, themesEditingColor:
		var lines []string
		lines = append(lines, styleHeading.Render("Editing "+p.editSlug))
		for i, r := range p.editRows {
			text := fmt.Sprintf("%-14s %s", r.Token, r.Color)
			if r.Overridden {
				text += styleAccent.Render(" *")
			}
			lines = append(lines, row(text, i == p.editCur))
		}
		if p.mode == themesEditingColor {
			lines = append(lines, "", p.input.View())
		} else if len(p.editRows) > 0 {
			r := p.editRows[p.editCur]
			lines = append(lines, "", styleDim.Render(r.Origin+" · "+r.Role))
		}
		left = strings.Join(lines, "\n")
		right = theme.RenderSwatchFromTokens(p.editTok, p.editSlug+" (editing)")
	default:
		left = p.list.view(height-1, func(i int, selected bool) string {
			item := p.items[i]
			marker := "  "
			switch {
			case item.Applied:
				marker = styleOK.Render("✓ ")
			case item.User:
				marker = styleWarn.Render("★ ")
			}
			return row(marker+item.Slug, selected)
		})
		left += "\n" + styleDim.Render("  more at terminalcolors.com")
		if item, ok := p.selectedItem(); ok {
			right = p.preview(item.Slug)
		}
		if p.mode == themesNaming {
			left += "\n\n" + p.input.View()
		}
	}
	if !showPreview {
		return left
	}
	return columns(left, leftWidth, " "+strings.ReplaceAll(right, "\n", "\n "), height)
}
