package ui

import (
	"errors"
	"fmt"
	"os"
	"strings"

	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

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

// themeFilters are the views ←/→ move between.
var themeFilters = []string{"All", "Dark", "Light", "Yours"}

// themeRow is a group heading or one theme.
type themeRow struct {
	header string
	item   int
}

type themesPage struct {
	items      []theme.Item
	looks      map[string]theme.Look
	rows       []themeRow
	filter     int
	showTokens bool
	list       list
	mode       themesMode
	input      textinput.Model
	ask        *confirm
	remoteOnly bool
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
		looks    map[string]theme.Look
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
	p := &themesPage{list: newList(), looks: map[string]theme.Look{}, startEdit: edit}
	p.list.header = func(i int) bool { return i < len(p.rows) && p.rows[i].header != "" }
	return p
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
	looks := map[string]theme.Look{}
	for _, item := range items {
		if look, err := theme.LoadLook(item.Slug); err == nil {
			looks[item.Slug] = look
		}
	}
	return themesLoadedMsg{items: items, looks: looks, warnings: warnings, legacy: theme.LegacyThemes(), err: err}
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
	help := "↵ apply · ←/→ " + strings.ToLower(strings.Join(themeFilters, "/")) + " · / search · t tokens"
	if item, ok := p.selectedItem(); ok && item.User {
		help += " · e edit · d delete"
	}
	if p.legacy > 0 {
		help += " · I import old themes"
	}
	return help + " · ? keys"
}

func (p *themesPage) keys() []keyHint {
	return []keyHint{
		{"↑/↓ j/k", "move; the preview follows"},
		{"←/→ h/l", "show all, dark, light or your themes"},
		{"/", "search by name"},
		{"↵", "apply the theme and close"},
		{"t", "token table instead of the preview"},
		{"", "Your themes"},
		{"a", "write a new palette in your editor"},
		{"c", "add a palette from the clipboard (terminalcolors.com)"},
		{"e", "edit a theme's token colors"},
		{"d", "delete a theme"},
		{"I", "import themes from herdr-theme-picker"},
		{"r", "reload the list"},
	}
}

func (p *themesPage) selectedItem() (theme.Item, bool) {
	if i := p.list.selected(); i >= 0 && i < len(p.rows) && p.rows[i].header == "" {
		return p.items[p.rows[i].item], true
	}
	return theme.Item{}, false
}

// rebuild lists the themes the filter shows, keeping the selection.
func (p *themesPage) rebuild() {
	keep, _ := p.selectedItem()
	p.rows = p.rows[:0]
	var groups [4][]int // in use, yours, dark, light
	for i, item := range p.items {
		dark := p.looks[item.Slug].Dark || p.looks[item.Slug].Tokens == nil
		switch themeFilters[p.filter] {
		case "Dark":
			if dark {
				groups[2] = append(groups[2], i)
			}
		case "Light":
			if !dark {
				groups[3] = append(groups[3], i)
			}
		case "Yours":
			if item.User {
				groups[1] = append(groups[1], i)
			}
		default:
			switch {
			case item.Applied:
				groups[0] = append(groups[0], i)
			case item.User:
				groups[1] = append(groups[1], i)
			case dark:
				groups[2] = append(groups[2], i)
			default:
				groups[3] = append(groups[3], i)
			}
		}
	}
	var labels []string
	for g, name := range []string{"In use", "Yours", "Dark", "Light"} {
		if len(groups[g]) == 0 {
			continue
		}
		if p.filter == 0 {
			p.rows = append(p.rows, themeRow{header: name})
			labels = append(labels, name)
		}
		for _, i := range groups[g] {
			p.rows = append(p.rows, themeRow{item: i})
			labels = append(labels, p.items[i].Slug)
		}
	}
	p.list.setRows(labels)
	for i, r := range p.rows {
		if r.header == "" && p.items[r.item].Slug == keep.Slug {
			p.list.selectRow(i)
		}
	}
	p.list.skipHeaders(1)
}

// count reports how many themes filter f shows.
func (p *themesPage) count(f int) int {
	n := 0
	for _, item := range p.items {
		dark := p.looks[item.Slug].Dark || p.looks[item.Slug].Tokens == nil
		switch themeFilters[f] {
		case "Dark":
			if dark {
				n++
			}
		case "Light":
			if !dark {
				n++
			}
		case "Yours":
			if item.User {
				n++
			}
		default:
			n++
		}
	}
	return n
}

func (p *themesPage) update(msg tea.Msg) tea.Cmd {
	switch msg := msg.(type) {
	case themesLoadedMsg:
		if msg.err != nil {
			return setError(msg.err)
		}
		p.items, p.looks = msg.items, msg.looks
		p.rebuild()
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
	case "left", "h":
		p.filter = (p.filter + len(themeFilters) - 1) % len(themeFilters)
		p.rebuild()
	case "right", "l":
		p.filter = (p.filter + 1) % len(themeFilters)
		p.rebuild()
	case "t":
		p.showTokens = !p.showTokens
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

func (p *themesPage) view(width, height int) string {
	if p.remoteOnly {
		return "\n" + theme.ClientSetup
	}
	if p.mode == themesEditing || p.mode == themesEditingColor {
		return p.editorView(width, height)
	}
	leftWidth := min(38, width)
	if width < 90 {
		leftWidth = width
	}
	var lines []string
	lines = append(lines, p.chips(), "")
	listHeight := height - 4
	if p.mode == themesNaming {
		listHeight -= 2
	}
	nameWidth := leftWidth - 14
	lines = append(lines, fit(p.list.view(listHeight, func(i int, selected bool) string {
		r := p.rows[i]
		if r.header != "" {
			return "  " + styleDim.Render(strings.ToUpper(r.header))
		}
		item := p.items[r.item]
		marker := "  "
		switch {
		case item.Applied:
			marker = styleOK.Render("● ")
		case item.User:
			marker = styleWarn.Render("★ ")
		}
		name := clip(item.Slug, nameWidth)
		if selected {
			name = styleSelected.Render(name)
		}
		return row(marker+padRight(name, nameWidth)+" "+swatch(p.looks[item.Slug]), selected)
	}), leftWidth, listHeight), "")
	if p.mode == themesNaming {
		lines = append(lines, p.input.View(), "")
	}
	lines = append(lines, styleDim.Render("  c pastes one from terminalcolors.com"))
	left := strings.Join(lines, "\n")
	if width < 90 {
		return left
	}
	item, ok := p.selectedItem()
	if !ok {
		return left
	}
	return columns(left, leftWidth+1, p.card(item), height)
}

// chips shows the filters with the current one highlighted.
func (p *themesPage) chips() string {
	var parts []string
	for i, name := range themeFilters {
		label := fmt.Sprintf(" %s %d ", name, p.count(i))
		if i == p.filter {
			parts = append(parts, styleTabActive.Render(label))
		} else {
			parts = append(parts, styleDim.Render(label))
		}
	}
	return " " + strings.Join(parts, "")
}

// swatch draws a palette's 16 colors in 8 cells: normal over bright.
func swatch(look theme.Look) string {
	if look.Tokens == nil {
		return styleDim.Render("unreadable")
	}
	var b strings.Builder
	for i := 0; i < 8; i++ {
		b.WriteString(theme.Paint(look.ANSI[i+8], look.ANSI[i], "▀"))
	}
	return b.String()
}

// card frames the preview of item in its own accent color.
func (p *themesPage) card(item theme.Item) string {
	look, ok := p.looks[item.Slug]
	if !ok {
		return styleError.Render("  cannot read the palette for " + item.Slug)
	}
	title := styleHeading.Render(item.Slug)
	kind := "light"
	if look.Dark {
		kind = "dark"
	}
	title += styleDim.Render(" · " + kind)
	if item.Applied {
		title += styleOK.Render(" · ● in use")
	}
	if item.User {
		title += styleWarn.Render(" · ★ yours")
	}
	return framed(title, look, p.showTokens)
}

func framed(title string, look theme.Look, tokens bool) string {
	body := theme.RenderMock(look.Tokens)
	if tokens {
		body = theme.RenderTokenTable(look.Tokens)
	}
	var palette [2]strings.Builder
	for i := 0; i < 8; i++ {
		palette[0].WriteString(theme.Paint(look.ANSI[i], look.ANSI[i], "    "))
		palette[1].WriteString(theme.Paint(look.ANSI[i+8], look.ANSI[i+8], "    "))
	}
	t := look.Tokens
	sample := theme.Paint(look.Background, look.Foreground, " $ git push ") +
		theme.Paint(look.Background, t["green"], "✓ done ") +
		theme.Paint(look.Background, t["red"], "✗ 2 errors ") +
		theme.Paint(look.Background, t["yellow"], "⚠ 1 warning ") +
		theme.Paint(look.Background, t["blue"], "→ main ") +
		theme.Paint(t["selection_bg"], look.Foreground, " selected ")
	content := strings.Join([]string{
		title, "",
		strings.TrimRight(body, "\n"), "",
		sample, "",
		palette[0].String(), palette[1].String(),
	}, "\n")
	return lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color(t["accent"])).
		Padding(0, 1).
		Render(content)
}

// editorView lists a user theme's tokens beside a live preview.
func (p *themesPage) editorView(width, height int) string {
	var lines []string
	lines = append(lines, styleHeading.Render("Editing "+p.editSlug), "")
	for i, r := range p.editRows {
		text := fmt.Sprintf("%s %-14s %s", theme.Paint(r.Color, r.Color, "  "), r.Token, r.Color)
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
	left := strings.Join(lines, "\n")
	if width < 90 {
		return left
	}
	look := theme.Look{Tokens: p.editTok, Background: p.editTok["panel_bg"], Foreground: p.editTok["text"], Dark: theme.IsDark(p.editTok["panel_bg"])}
	for i, token := range []string{"surface0", "red", "green", "yellow", "blue", "mauve", "teal", "text"} {
		look.ANSI[i], look.ANSI[i+8] = p.editTok[token], p.editTok[token]
	}
	title := styleHeading.Render(p.editSlug) + styleDim.Render(" · editing")
	if p.editDirty {
		title += styleWarn.Render(" · unsaved")
	}
	return columns(left, 38, framed(title, look, false), height)
}
