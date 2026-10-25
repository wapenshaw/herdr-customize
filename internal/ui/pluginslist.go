package ui

import (
	"context"
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/wapenshaw/herdr-customize/internal/pluginconfig"
	"github.com/wapenshaw/herdr-customize/internal/plugins"
)

// The installed screen: a list of plugins on the left and, on the right, a
// panel for the selected one whose menu holds everything it can do. → or ↵
// moves into the panel, ← or esc back.

// Rows below the plugins that start something new.
const (
	rowPlugin = iota
	rowGap
	rowInstall
	rowMarket
)

type pluginRow struct {
	kind   int
	plugin int
}

// pluginDetail is what the panel loads lazily for a plugin.
type pluginDetail struct {
	settings string
	last     string
}

type pluginDetailMsg struct {
	id     string
	detail pluginDetail
}

// menuItem is one line of the panel's menu.
type menuItem struct {
	icon, label, info, key string
	run                    func() tea.Cmd
	action                 *plugins.Action
}

func (p *pluginsPage) help() string {
	switch p.mode {
	case pluginsConfirm:
		return p.ask.question + " [y/N]"
	case pluginsInstallInput, pluginsMarketQuery:
		return "↵ confirm · esc cancel"
	}
	switch p.screen {
	case viewMarket:
		return "↵ install · o open repo · / search · s sort · r refresh · esc back"
	case viewLogs:
		return "↑/↓ move · r refresh · esc back"
	case viewConfig:
		return p.config.help()
	}
	if p.panel {
		if item := p.menuItem(); item != nil && item.action != nil {
			return "↵ run · b bind a key · ↑/↓ move · ← back · ? keys"
		}
		return "↵ choose · ↑/↓ move · ← back · ? keys"
	}
	return "↵/→ open · ↑/↓ move · / search · m marketplace · i install · ? keys"
}

func (p *pluginsPage) keys() []keyHint {
	return []keyHint{
		{"↑/↓ j/k", "move"},
		{"↵ →", "open the plugin's panel (or start install / marketplace)"},
		{"← esc", "back to the list"},
		{"/", "search installed plugins"},
		{"", "On a plugin, from the list or the panel"},
		{"s", "settings: its config files, typed"},
		{"l", "recent runs and their output"},
		{"u", "update (moves an exact-commit pin after asking)"},
		{"e", "enable or disable"},
		{"x", "uninstall, or unlink a local checkout"},
		{"o", "open the repository"},
		{"b", "bind the selected action to a key"},
		{"", "Everywhere"},
		{"U", "update every plugin that is behind"},
		{"m", "marketplace of topic:herdr-plugin repositories"},
		{"i", "install owner/repo[/subdir][@ref]"},
		{"c", "edit plugins.json"},
		{"r", "reload"},
	}
}

// selected returns the plugin on the list's row, if it is one.
func (p *pluginsPage) selected() *plugins.Plugin {
	i := p.list.selected()
	if i < 0 || i >= len(p.rows) || p.rows[i].kind != rowPlugin {
		return nil
	}
	return &p.plugins[p.rows[i].plugin]
}

func (p *pluginsPage) selectedKind() int {
	if i := p.list.selected(); i >= 0 && i < len(p.rows) {
		return p.rows[i].kind
	}
	return -1
}

func (p *pluginsPage) rebuild() {
	p.rows = p.rows[:0]
	var labels []string
	for i, pl := range p.plugins {
		p.rows = append(p.rows, pluginRow{kind: rowPlugin, plugin: i})
		labels = append(labels, pl.ID+" "+pl.Name+" "+pl.Description)
	}
	if p.loadErr == nil {
		p.rows = append(p.rows, pluginRow{kind: rowGap}, pluginRow{kind: rowInstall}, pluginRow{kind: rowMarket})
		labels = append(labels, "", "install from github", "marketplace browse")
	}
	p.list.setRows(labels)
	p.list.skipHeaders(1)
}

// loadDetail fetches the panel's settings and last-run lines for the
// selected plugin once.
func (p *pluginsPage) loadDetail() tea.Cmd {
	pl := p.selected()
	if pl == nil {
		return nil
	}
	if _, ok := p.details[pl.ID]; ok {
		return nil
	}
	p.details[pl.ID] = pluginDetail{settings: "…", last: "…"}
	target := *pl
	return func() tea.Msg {
		var d pluginDetail
		if dir, err := pluginconfig.Dir(target.ID); err != nil {
			d.settings = "unavailable"
		} else if cfg, err := pluginconfig.Load(dir, target.Root); err != nil {
			d.settings = "unreadable: " + err.Error()
		} else {
			d.settings = settingsSummary(cfg)
		}
		if logs, err := plugins.Logs(target.ID, 1); err == nil && len(logs) > 0 {
			l := logs[0]
			mark := "✓"
			if l.Status != "succeeded" {
				mark = "✗"
			}
			d.last = fmt.Sprintf("%s %s %s", l.Trigger(), mark, ago(l.StartedAt()))
		} else {
			d.last = "none yet"
		}
		return pluginDetailMsg{id: target.ID, detail: d}
	}
}

func settingsSummary(cfg *pluginconfig.Config) string {
	entries, files := 0, 0
	var names []string
	for _, f := range cfg.Files {
		if f.Format == pluginconfig.Other {
			continue
		}
		files++
		entries += len(f.Entries)
		names = append(names, f.Name)
	}
	switch {
	case files == 0 && len(cfg.Files) == 0:
		return "none yet"
	case files == 0:
		return fmt.Sprintf("%d file(s) to edit", len(cfg.Files))
	case files == 1:
		return fmt.Sprintf("%d in %s", entries, names[0])
	}
	return fmt.Sprintf("%d in %d files", entries, files)
}

// ago is a short relative time.
func ago(t time.Time) string {
	d := time.Since(t)
	switch {
	case d < time.Minute:
		return "just now"
	case d < time.Hour:
		return fmt.Sprintf("%dm ago", int(d.Minutes()))
	case d < 48*time.Hour:
		return fmt.Sprintf("%dh ago", int(d.Hours()))
	}
	return fmt.Sprintf("%dd ago", int(d.Hours()/24))
}

// menu lists what the panel offers for pl.
func (p *pluginsPage) menu(pl *plugins.Plugin) []menuItem {
	var items []menuItem
	for i := range pl.Actions {
		a := &pl.Actions[i]
		if !a.Available() {
			continue
		}
		qualified := a.QualifiedID(pl.ID)
		items = append(items, menuItem{icon: "▶", label: firstNonEmpty(a.Title, a.ID), info: qualified, action: a,
			run: func() tea.Cmd { return quitThen(func() error { return plugins.InvokeAfterClose(qualified) }) }})
	}
	target := *pl
	detail := p.details[pl.ID]
	items = append(items,
		menuItem{icon: "⚙", label: "Settings", info: detail.settings, key: "s", run: func() tea.Cmd { return p.openConfig(target) }},
		menuItem{icon: "≡", label: "Recent runs", info: detail.last, key: "l", run: func() tea.Cmd { return p.openLogs(target) }},
	)
	if pl.GitHub() {
		items = append(items, menuItem{icon: "↑", label: "Update", info: p.updateInfo(*pl), key: "u", run: func() tea.Cmd { return p.updateOne(target) }})
	}
	toggle := "Disable"
	if !pl.Enabled {
		toggle = "Enable"
	}
	items = append(items, menuItem{icon: "⏻", label: toggle, key: "e", run: func() tea.Cmd { return p.toggle(target) }})
	remove := "Uninstall"
	if !pl.GitHub() {
		remove = "Unlink"
	}
	items = append(items, menuItem{icon: "✕", label: remove, key: "x", run: func() tea.Cmd { return p.remove(target) }})
	if pl.GitHub() {
		items = append(items, menuItem{icon: "↗", label: "Open repository", info: "github.com/" + pl.Slug(), key: "o", run: func() tea.Cmd { return p.openRepo(target) }})
	}
	return items
}

func (p *pluginsPage) updateInfo(pl plugins.Plugin) string {
	status := p.statuses[pl.ID]
	switch {
	case !p.haveGit:
		return "install git to check"
	case p.checking:
		return "checking…"
	case pl.Pinned():
		return "pinned to " + short(pl.Ref())
	case status.State == plugins.Behind && status.Version != "" && status.Version != pl.Version:
		return pl.Version + " → " + status.Version
	case status.State == plugins.Behind:
		return "→ " + short(status.Remote)
	case status.State == plugins.Current:
		return "up to date"
	case status.State == plugins.Failed:
		return "could not check"
	}
	return ""
}

// menuItem returns the panel item under the cursor.
func (p *pluginsPage) menuItem() *menuItem {
	pl := p.selected()
	if pl == nil {
		return nil
	}
	items := p.menu(pl)
	if p.menuCursor < 0 || p.menuCursor >= len(items) {
		return nil
	}
	return &items[p.menuCursor]
}

func (p *pluginsPage) handleInstalledKey(msg tea.KeyMsg) tea.Cmd {
	key := msg.String()
	if p.panel {
		if cmd, ok := p.handlePanelKey(msg); ok {
			return cmd
		}
	} else if handled, cmd := p.list.handleKey(msg); handled {
		return tea.Batch(cmd, p.loadDetail())
	}
	switch key {
	case "m":
		return p.openMarket()
	case "i":
		return p.startInstall()
	case "c":
		return p.editRegistry()
	case "r":
		p.details = map[string]pluginDetail{}
		return tea.Batch(loadPlugins, setStatus("refreshing…"))
	case "U":
		return p.updateAll()
	}
	pl := p.selected()
	if pl == nil {
		if key == "enter" || key == "right" || key == "l" {
			switch p.selectedKind() {
			case rowInstall:
				return p.startInstall()
			case rowMarket:
				return p.openMarket()
			}
		}
		return nil
	}
	switch key {
	case "enter", "right":
		p.panel, p.menuCursor = true, 0
		return p.loadDetail()
	case "s":
		return p.openConfig(*pl)
	case "l":
		return p.openLogs(*pl)
	case "u":
		return p.updateOne(*pl)
	case "e":
		return p.toggle(*pl)
	case "x":
		return p.remove(*pl)
	case "o":
		return p.openRepo(*pl)
	}
	return nil
}

// handlePanelKey moves in and runs the panel's menu. Keys it does not use
// fall through to the plugin's letter keys.
func (p *pluginsPage) handlePanelKey(msg tea.KeyMsg) (tea.Cmd, bool) {
	pl := p.selected()
	if pl == nil {
		p.panel = false
		return nil, false
	}
	items := p.menu(pl)
	switch msg.String() {
	case "up", "k":
		p.menuCursor = max(p.menuCursor-1, 0)
	case "down", "j":
		p.menuCursor = min(p.menuCursor+1, len(items)-1)
	case "home", "g":
		p.menuCursor = 0
	case "end", "G":
		p.menuCursor = len(items) - 1
	case "left", "h", "esc":
		p.panel = false
	case "enter", " ":
		if item := p.menuItem(); item != nil {
			return item.run(), true
		}
	case "b":
		item := p.menuItem()
		if item == nil || item.action == nil {
			return setStatus("select an action to bind it"), true
		}
		return goTo("keys", bindActionMsg{qualified: item.action.QualifiedID(pl.ID), title: item.label}), true
	default:
		return nil, false
	}
	return nil, true
}

func (p *pluginsPage) openMarket() tea.Cmd {
	p.screen = viewMarket
	if p.repos == nil && !p.marketBusy {
		return p.search(1)
	}
	return nil
}

func (p *pluginsPage) startInstall() tea.Cmd {
	p.mode = pluginsInstallInput
	p.input = newInput("install owner/repo[/subdir][@ref]: ", "")
	return textinput.Blink
}

func (p *pluginsPage) openConfig(pl plugins.Plugin) tea.Cmd {
	p.screen, p.config = viewConfig, newPluginConfig(pl)
	// The summary is read again on the way back.
	delete(p.details, pl.ID)
	return loadPluginConfig(pl)
}

func (p *pluginsPage) openLogs(pl plugins.Plugin) tea.Cmd {
	p.screen, p.logsFor, p.logs, p.logsErr = viewLogs, pl.ID, nil, nil
	p.logsList.setRows(nil)
	return p.loadLogs()
}

func (p *pluginsPage) toggle(pl plugins.Plugin) tea.Cmd {
	verb := "disabled"
	if !pl.Enabled {
		verb = "enabled"
	}
	return func() tea.Msg {
		return pluginOpMsg{text: verb + " " + pl.Name, err: plugins.SetEnabled(pl.ID, !pl.Enabled)}
	}
}

func (p *pluginsPage) remove(pl plugins.Plugin) tea.Cmd {
	verb := "uninstall"
	if !pl.GitHub() {
		verb = "unlink"
	}
	p.mode = pluginsConfirm
	p.ask = &confirm{question: fmt.Sprintf("%s %s (%s)?", verb, pl.Name, pl.ID), yes: func() tea.Cmd {
		p.panel = false
		return func() tea.Msg { return pluginOpMsg{text: verb + "ed " + pl.Name, err: plugins.Remove(pl)} }
	}}
	return nil
}

func (p *pluginsPage) openRepo(pl plugins.Plugin) tea.Cmd {
	if !pl.GitHub() {
		return setStatus(pl.Name + " is linked from a local checkout")
	}
	if err := plugins.OpenURL(pl.RepoURL()); err != nil {
		return setError(err)
	}
	return setStatus("opened " + pl.RepoURL())
}

func (p *pluginsPage) checkUpdates() tea.Cmd {
	if !p.haveGit {
		return nil
	}
	p.checking = true
	list := p.plugins
	return func() tea.Msg {
		return checksMsg{statuses: plugins.CheckAll(context.Background(), list)}
	}
}

func (p *pluginsPage) installedView(width, height int, lines []string) string {
	if !p.loaded {
		return "loading plugins…"
	}
	if p.loadErr != nil {
		return styleError.Render("herdr plugin list: "+p.loadErr.Error()) + "\n" + styleDim.Render("i installs a plugin by owner/repo · m opens the marketplace")
	}
	leftWidth := min(max(width/3, 28), 36)
	narrow := width < 84
	if narrow {
		leftWidth = width
	}
	listHeight := max(height-len(lines)-2, 3)
	nameWidth := leftWidth - 14
	body := p.list.view(listHeight, func(i int, selected bool) string {
		r := p.rows[i]
		switch r.kind {
		case rowGap:
			return ""
		case rowInstall:
			return row(styleAccent.Render("＋ Install from GitHub…"), selected)
		case rowMarket:
			return row(styleAccent.Render("◆ Browse the marketplace…"), selected)
		}
		pl := p.plugins[r.plugin]
		name := clip(pl.Name, nameWidth)
		switch {
		case !pl.Enabled:
			name = styleDim.Render(padRight(name, nameWidth))
		case selected && p.panel:
			name = styleHeading.Render(padRight(name, nameWidth))
		case selected:
			name = styleSelected.Render(padRight(name, nameWidth))
		default:
			name = padRight(name, nameWidth)
		}
		version := pl.Version
		if !pl.Enabled {
			version = "off"
		}
		return row(p.statusGlyph(pl)+" "+name+" "+styleDim.Render(clip(version, 9)), selected)
	})
	left := strings.Join(append(lines, fit(body, leftWidth, listHeight), "", p.summary()), "\n")
	if narrow && !p.panel {
		return left
	}
	panel := p.panelView(width-leftWidth-1, height)
	if narrow {
		return panel
	}
	return columns(left, leftWidth+1, panel, height)
}

func (p *pluginsPage) statusGlyph(pl plugins.Plugin) string {
	status := p.statuses[pl.ID]
	switch {
	case !pl.GitHub():
		return styleDim.Render("⌂")
	case status.State == plugins.Current:
		return styleOK.Render("●")
	case status.State == plugins.Behind:
		return styleWarn.Render("↑")
	case status.State == plugins.Failed:
		return styleError.Render("?")
	}
	return styleDim.Render("·")
}

// summary is the update state of every plugin, under the list.
func (p *pluginsPage) summary() string {
	switch {
	case !p.haveGit:
		return styleDim.Render("install git to check for updates")
	case p.checking:
		return styleDim.Render("checking for updates…")
	}
	behind, failed := 0, 0
	for _, s := range p.statuses {
		switch s.State {
		case plugins.Behind:
			behind++
		case plugins.Failed:
			failed++
		}
	}
	text := styleDim.Render("all plugins up to date")
	if behind > 0 {
		text = styleWarn.Render(fmt.Sprintf("↑ %d update(s) · U updates all", behind))
	}
	if failed > 0 {
		text += styleError.Render(fmt.Sprintf(" · %d not checked", failed))
	}
	return text
}

// panelView describes the selected row; for a plugin, with its menu.
func (p *pluginsPage) panelView(width, height int) string {
	inner := min(max(width-4, 20), 72)
	var lines []string
	switch p.selectedKind() {
	case rowInstall:
		lines = append(lines, styleHeading.Render("Install from GitHub"), "")
		lines = append(lines, wrap("Type owner/repo, optionally with /subdir and @ref (a branch, tag or commit). Herdr shows what the plugin will run and asks before anything is installed.", inner)...)
		lines = append(lines, "", styleDim.Render("↵ start"))
	case rowMarket:
		lines = append(lines, styleHeading.Render("Marketplace"), "")
		lines = append(lines, wrap("GitHub repositories tagged "+plugins.MarketTopic+", by stars or recent activity, searchable. Installing one goes through Herdr's trust prompt.", inner)...)
		lines = append(lines, "", styleDim.Render("↵ open"))
	case rowPlugin:
		lines = p.pluginPanel(p.selected(), inner)
	}
	border := lipgloss.Color("8")
	if p.panel {
		border = lipgloss.Color("4")
	}
	return lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(border).
		Padding(0, 1).
		Width(inner + 2).
		Render(fit(strings.Join(lines, "\n"), inner, min(len(lines), max(height-2, 3))))
}

func (p *pluginsPage) pluginPanel(pl *plugins.Plugin, width int) []string {
	title := styleHeading.Render(pl.Name) + styleDim.Render("  "+pl.Version)
	state := p.updateInfo(*pl)
	switch {
	case !pl.Enabled:
		state = "disabled"
	case !pl.GitHub():
		state = "linked"
	}
	title = padRight(title, width-lipgloss.Width(state)-2) + p.statusGlyph(*pl) + " " + styleDim.Render(state)
	source := "linked from " + pl.Root
	if pl.GitHub() {
		source = pl.Spec()
		if ref := pl.Ref(); ref != "" {
			source += "@" + ref
		}
	}
	lines := []string{clip(title, width), styleDim.Render(clip(source, width)), ""}
	if pl.Description != "" {
		lines = append(lines, wrap(pl.Description, width)...)
		lines = append(lines, "")
	}
	if s := p.statuses[pl.ID]; s.State == plugins.Failed && s.Err != nil {
		lines = append(lines, styleError.Render(clip(s.Err.Error(), width)), "")
	}
	items := p.menu(pl)
	labelWidth := 0
	for _, item := range items {
		labelWidth = max(labelWidth, lipgloss.Width(item.label))
	}
	labelWidth = min(labelWidth, width/2)
	for i, item := range items {
		selected := p.panel && i == p.menuCursor
		label := padRight(clip(item.label, labelWidth), labelWidth)
		if selected {
			label = styleSelected.Render(label)
		}
		key := ""
		if item.key != "" {
			key = styleAccent.Render(item.key)
		}
		info := clip(item.info, max(width-labelWidth-10, 0))
		text := styleDim.Render(item.icon) + " " + label + "  " + styleDim.Render(info)
		text = padRight(text, width-3) + key
		lines = append(lines, row(text, selected))
	}
	return lines
}
