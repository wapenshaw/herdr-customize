package ui

import (
	"context"
	"errors"
	"fmt"
	"os"
	"regexp"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"

	"github.com/wapenshaw/herdr-customize/internal/plugins"
	"github.com/wapenshaw/herdr-customize/internal/theme"
)

type pluginsView int

const (
	viewInstalled pluginsView = iota
	viewMarket
	viewLogs
)

type pluginsMode int

const (
	pluginsBrowse pluginsMode = iota
	pluginsConfirm
	pluginsInstallInput
	pluginsMarketQuery
)

// marketPage is how many results each marketplace request fetches.
const marketPage = 50

type pluginRow struct {
	plugin int
	action int // -1 for the plugin itself
}

type pluginsPage struct {
	sh       *shared
	screen   pluginsView
	mode     pluginsMode
	ask      *confirm
	input    textinput.Model
	plugins  []plugins.Plugin
	loadErr  error
	loaded   bool
	expanded map[string]bool
	rows     []pluginRow
	list     list
	statuses map[string]plugins.Status
	checking bool
	// marketplace
	market      list
	repos       []plugins.Repo
	marketTotal int
	marketQuery string
	marketSort  string
	marketBusy  bool
	marketErr   error
	// logs
	logs     []plugins.Log
	logsList list
	logsFor  string
	logsErr  error
	haveGit  bool
}

type (
	checksMsg struct{ statuses map[string]plugins.Status }
	marketMsg struct {
		query, sort string
		page        int
		results     plugins.Results
		err         error
	}
	pluginOpMsg struct {
		text string
		err  error
	}
	pinMsg struct {
		plugin plugins.Plugin
		sha    string
		err    error
	}
	logsMsg struct {
		id   string
		logs []plugins.Log
		err  error
	}
	registryEditedMsg struct{ err error }
)

func newPluginsPage(sh *shared) page {
	p := &pluginsPage{sh: sh, list: newList(), market: newList(), logsList: newList(), expanded: map[string]bool{}, marketSort: "stars", haveGit: plugins.HaveGit()}
	p.list.header = func(i int) bool { return false }
	return p
}

func (p *pluginsPage) title() string { return "Plugins" }

func (p *pluginsPage) init() tea.Cmd { return nil }

func (p *pluginsPage) capturing() bool {
	if p.mode != pluginsBrowse {
		return true
	}
	switch p.screen {
	case viewMarket:
		return p.market.filtering
	case viewLogs:
		return p.logsList.filtering
	}
	return p.list.filtering
}

// back leaves the marketplace or logs before the popup closes.
func (p *pluginsPage) back() bool {
	switch p.screen {
	case viewMarket, viewLogs:
		p.screen = viewInstalled
		return true
	}
	return p.list.clearFilter()
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
		return "j/k move · r refresh · esc back"
	}
	if _, action := p.selected(); action != nil {
		return "↵ run · b bind a key · esc back"
	}
	return "↵ actions · u/U update · e on/off · x remove · o repo · l logs · m market · i install · c plugins.json"
}

// selected returns the plugin under the cursor and, on an action row, the
// action.
func (p *pluginsPage) selected() (*plugins.Plugin, *plugins.Action) {
	i := p.list.selected()
	if i < 0 || i >= len(p.rows) {
		return nil, nil
	}
	r := p.rows[i]
	pl := &p.plugins[r.plugin]
	if r.action >= 0 {
		return pl, &pl.Actions[r.action]
	}
	return pl, nil
}

func (p *pluginsPage) rebuild() {
	p.rows = p.rows[:0]
	var labels []string
	for i, pl := range p.plugins {
		p.rows = append(p.rows, pluginRow{plugin: i, action: -1})
		labels = append(labels, pl.ID+" "+pl.Name+" "+pl.Description)
		if !p.expanded[pl.ID] {
			continue
		}
		for j, a := range pl.Actions {
			if !a.Available() {
				continue
			}
			p.rows = append(p.rows, pluginRow{plugin: i, action: j})
			labels = append(labels, a.QualifiedID(pl.ID)+" "+a.Title)
		}
	}
	p.list.setRows(labels)
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

func (p *pluginsPage) update(msg tea.Msg) tea.Cmd {
	switch msg := msg.(type) {
	case pluginsMsg:
		p.loaded, p.loadErr = true, msg.err
		if msg.err == nil {
			p.plugins = msg.list
		}
		p.rebuild()
		return p.checkUpdates()
	case checksMsg:
		p.checking = false
		p.statuses = msg.statuses
	case marketMsg:
		return p.marketLoaded(msg)
	case logsMsg:
		if msg.id == p.logsFor {
			p.logs, p.logsErr = msg.logs, msg.err
			labels := make([]string, len(msg.logs))
			for i, l := range msg.logs {
				labels[i] = l.Trigger() + " " + l.Status
			}
			p.logsList.setRows(labels)
		}
	case pluginOpMsg:
		if msg.err != nil {
			return tea.Batch(setError(fmt.Errorf("%s: %w", msg.text, msg.err)), loadPlugins)
		}
		return tea.Batch(setStatus(msg.text), loadPlugins)
	case pinMsg:
		return p.confirmPinMove(msg)
	case registryEditedMsg:
		if msg.err != nil {
			return tea.Batch(setError(fmt.Errorf("editor: %w", msg.err)), loadPlugins)
		}
		return loadPlugins
	case tea.KeyMsg:
		return p.handleKey(msg)
	}
	return nil
}

func (p *pluginsPage) handleKey(msg tea.KeyMsg) tea.Cmd {
	switch p.mode {
	case pluginsConfirm:
		p.mode = pluginsBrowse
		ask := p.ask
		p.ask = nil
		return ask.handle(msg)
	case pluginsInstallInput, pluginsMarketQuery:
		return p.handleInput(msg)
	}
	switch p.screen {
	case viewMarket:
		return p.handleMarketKey(msg)
	case viewLogs:
		return p.handleLogsKey(msg)
	}
	if handled, cmd := p.list.handleKey(msg); handled {
		return cmd
	}
	switch msg.String() {
	case "m":
		p.screen = viewMarket
		if p.repos == nil && !p.marketBusy {
			return p.search(1)
		}
		return nil
	case "i":
		p.mode = pluginsInstallInput
		p.input = newInput("install owner/repo[/subdir][@ref]: ", "")
		return textinput.Blink
	case "c":
		return p.editRegistry()
	case "r":
		return tea.Batch(loadPlugins, setStatus("refreshing…"))
	case "U":
		return p.updateAll()
	}
	pl, action := p.selected()
	if pl == nil {
		return nil
	}
	if action != nil {
		switch msg.String() {
		case "enter":
			qualified := action.QualifiedID(pl.ID)
			return quitThen(func() error { return plugins.InvokeAfterClose(qualified) })
		case "b":
			return goTo("keys", bindActionMsg{qualified: action.QualifiedID(pl.ID), title: firstNonEmpty(action.Title, action.ID)})
		}
		return nil
	}
	switch msg.String() {
	case "enter":
		available := 0
		for _, a := range pl.Actions {
			if a.Available() {
				available++
			}
		}
		if available == 0 {
			return setStatus(pl.Name + " has no actions")
		}
		p.expanded[pl.ID] = !p.expanded[pl.ID]
		p.rebuild()
	case "u":
		return p.updateOne(*pl)
	case "e":
		target, verb := *pl, "disabled"
		if !pl.Enabled {
			verb = "enabled"
		}
		return func() tea.Msg {
			return pluginOpMsg{text: verb + " " + target.Name, err: plugins.SetEnabled(target.ID, !target.Enabled)}
		}
	case "x":
		target, verb := *pl, "uninstall"
		if !pl.GitHub() {
			verb = "unlink"
		}
		p.mode = pluginsConfirm
		p.ask = &confirm{question: fmt.Sprintf("%s %s (%s)?", verb, pl.Name, pl.ID), yes: func() tea.Cmd {
			return func() tea.Msg { return pluginOpMsg{text: verb + "ed " + target.Name, err: plugins.Remove(target)} }
		}}
	case "o":
		if !pl.GitHub() {
			return setStatus(pl.Name + " is linked from a local checkout")
		}
		if err := plugins.OpenURL(pl.RepoURL()); err != nil {
			return setError(err)
		}
		return setStatus("opened " + pl.RepoURL())
	case "l":
		p.screen, p.logsFor, p.logs, p.logsErr = viewLogs, pl.ID, nil, nil
		p.logsList.setRows(nil)
		return p.loadLogs()
	}
	return nil
}

func (p *pluginsPage) loadLogs() tea.Cmd {
	id := p.logsFor
	return func() tea.Msg {
		logs, err := plugins.Logs(id, 50)
		return logsMsg{id: id, logs: logs, err: err}
	}
}

// interactive runs herdr on the terminal; install and update show Herdr's
// trust preview there.
func interactive(what string, args ...string) tea.Cmd {
	cmd, err := plugins.Interactive(args...)
	if err != nil {
		return setError(err)
	}
	return tea.ExecProcess(cmd, func(err error) tea.Msg {
		if err != nil {
			return pluginOpMsg{text: what, err: errors.New("cancelled or failed")}
		}
		return pluginOpMsg{text: what + " done"}
	})
}

func (p *pluginsPage) updateOne(pl plugins.Plugin) tea.Cmd {
	if !pl.GitHub() {
		return setStatus(pl.Name + " is linked from a local checkout; update it there")
	}
	if pl.Pinned() {
		return func() tea.Msg {
			sha, err := plugins.ResolveRemote(context.Background(), pl.Slug(), "")
			return pinMsg{plugin: pl, sha: sha, err: err}
		}
	}
	if p.statuses[pl.ID].State == plugins.Current {
		return setStatus(pl.Name + " is up to date")
	}
	return interactive("update "+pl.Name, "plugin", "update", pl.ID)
}

// confirmPinMove offers to move an exact-commit pin to the default branch.
func (p *pluginsPage) confirmPinMove(msg pinMsg) tea.Cmd {
	pl := msg.plugin
	if msg.err != nil {
		return setError(msg.err)
	}
	if strings.EqualFold(msg.sha, pl.Source.Commit) {
		return setStatus(pl.Name + " is pinned to the latest commit")
	}
	p.mode = pluginsConfirm
	p.ask = &confirm{
		question: fmt.Sprintf("%s is pinned to %s; move the pin to %s (default branch)?", pl.Name, short(pl.Source.Commit), short(msg.sha)),
		yes: func() tea.Cmd {
			return interactive("update "+pl.Name, "plugin", "install", pl.Spec(), "--ref", msg.sha)
		},
	}
	return nil
}

func short(sha string) string { return sha[:min(len(sha), 7)] }

// updateAll updates every plugin that is behind. Exact-commit pins are left
// alone: moving one is a decision per plugin.
func (p *pluginsPage) updateAll() tea.Cmd {
	if !p.haveGit {
		return setError(errors.New("install git to check for updates"))
	}
	if p.checking {
		return setStatus("still checking for updates")
	}
	var ids, names []string
	for _, pl := range p.plugins {
		status := p.statuses[pl.ID]
		if !pl.GitHub() || pl.Pinned() || status.State != plugins.Behind {
			continue
		}
		ids = append(ids, pl.ID)
		name := pl.Name
		if status.Version != "" {
			name += " → " + status.Version
		}
		names = append(names, name)
	}
	if len(ids) == 0 {
		return setStatus("nothing to update")
	}
	p.mode = pluginsConfirm
	p.ask = &confirm{
		question: fmt.Sprintf("Update %d plugin(s): %s?", len(ids), strings.Join(names, ", ")),
		yes: func() tea.Cmd {
			return interactive(fmt.Sprintf("update of %d plugin(s)", len(ids)), append([]string{"plugin", "update"}, ids...)...)
		},
	}
	return nil
}

// specPattern matches what herdr plugin install takes, plus an optional @ref.
var specPattern = regexp.MustCompile(`^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+(/[A-Za-z0-9_./-]+)?(@[^\s@]+)?$`)

func installArgs(spec string) ([]string, error) {
	spec = strings.TrimSpace(strings.TrimPrefix(strings.TrimSpace(spec), "https://github.com/"))
	if !specPattern.MatchString(spec) {
		return nil, fmt.Errorf("%q is not owner/repo[/subdir][@ref]", spec)
	}
	args := []string{"plugin", "install"}
	if name, ref, ok := strings.Cut(spec, "@"); ok {
		return append(args, name, "--ref", ref), nil
	}
	return append(args, spec), nil
}

func (p *pluginsPage) handleInput(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "esc":
		p.mode = pluginsBrowse
		return nil
	case "enter":
		text := strings.TrimSpace(p.input.Value())
		mode := p.mode
		p.mode = pluginsBrowse
		if mode == pluginsMarketQuery {
			p.marketQuery = text
			return p.search(1)
		}
		if text == "" {
			return nil
		}
		args, err := installArgs(text)
		if err != nil {
			return setError(err)
		}
		return interactive("install "+args[2], args...)
	}
	var cmd tea.Cmd
	p.input, cmd = p.input.Update(msg)
	return cmd
}

func (p *pluginsPage) editRegistry() tea.Cmd {
	path := plugins.RegistryPath()
	if _, err := os.Stat(path); err != nil {
		return setError(fmt.Errorf("%s not found", path))
	}
	cmd, err := theme.EditorCommand(path)
	if err != nil {
		return setError(err)
	}
	return tea.ExecProcess(cmd, func(err error) tea.Msg { return registryEditedMsg{err: err} })
}

// search fetches one page of marketplace results; page 1 starts over.
func (p *pluginsPage) search(page int) tea.Cmd {
	if page == 1 {
		p.repos, p.marketTotal, p.marketErr = nil, 0, nil
		p.market.setRows(nil)
	}
	p.marketBusy = true
	query, sort := p.marketQuery, p.marketSort
	return func() tea.Msg {
		results, err := plugins.Search(context.Background(), query, sort, page, marketPage)
		return marketMsg{query: query, sort: sort, page: page, results: results, err: err}
	}
}

func (p *pluginsPage) marketLoaded(msg marketMsg) tea.Cmd {
	if msg.query != p.marketQuery || msg.sort != p.marketSort {
		return nil
	}
	p.marketBusy = false
	if msg.err != nil {
		p.marketErr = msg.err
		return nil
	}
	if msg.page == 1 {
		p.repos = nil
	}
	p.repos = append(p.repos, msg.results.Repos...)
	p.marketTotal = msg.results.Total
	labels := make([]string, len(p.repos))
	for i, r := range p.repos {
		labels[i] = r.FullName + " " + r.Description
	}
	p.market.setRows(labels)
	return nil
}

func (p *pluginsPage) installed(fullName string) bool {
	for _, pl := range p.plugins {
		if pl.GitHub() && strings.EqualFold(pl.Slug(), fullName) {
			return true
		}
	}
	return false
}

func (p *pluginsPage) handleMarketKey(msg tea.KeyMsg) tea.Cmd {
	if handled, cmd := p.market.handleKey(msg); handled {
		// Fetch the next page as the cursor nears the end of what is loaded.
		if p.market.filter.Value() == "" && !p.marketBusy && p.marketErr == nil &&
			p.market.cursor >= len(p.repos)-5 && len(p.repos) < p.marketTotal {
			return tea.Batch(cmd, p.search(len(p.repos)/marketPage+1))
		}
		return cmd
	}
	switch msg.String() {
	case "m":
		p.screen = viewInstalled
		return nil
	case "/":
		p.mode = pluginsMarketQuery
		p.input = newInput("search the marketplace: ", p.marketQuery)
		return textinput.Blink
	case "s":
		if p.marketSort == "stars" {
			p.marketSort = "updated"
		} else {
			p.marketSort = "stars"
		}
		return p.search(1)
	case "r":
		return p.search(1)
	}
	i := p.market.selected()
	if i < 0 || i >= len(p.repos) {
		return nil
	}
	repo := p.repos[i]
	switch msg.String() {
	case "enter", "i":
		if p.installed(repo.FullName) {
			return setStatus(repo.FullName + " is already installed; update it from the list")
		}
		p.mode = pluginsConfirm
		p.ask = &confirm{question: fmt.Sprintf("Install %s (★ %d)?", repo.FullName, repo.Stars), yes: func() tea.Cmd {
			return interactive("install "+repo.FullName, "plugin", "install", repo.FullName)
		}}
	case "o":
		url := "https://github.com/" + repo.FullName
		if err := plugins.OpenURL(url); err != nil {
			return setError(err)
		}
		return setStatus("opened " + url)
	}
	return nil
}

func (p *pluginsPage) handleLogsKey(msg tea.KeyMsg) tea.Cmd {
	if handled, cmd := p.logsList.handleKey(msg); handled {
		return cmd
	}
	switch msg.String() {
	case "r":
		return p.loadLogs()
	case "l":
		p.screen = viewInstalled
	}
	return nil
}

func (p *pluginsPage) view(width, height int) string {
	var lines []string
	if p.mode == pluginsInstallInput || p.mode == pluginsMarketQuery {
		lines = append(lines, p.input.View())
	}
	switch p.screen {
	case viewMarket:
		return p.marketView(width, height, lines)
	case viewLogs:
		return p.logsView(width, height, lines)
	}
	if !p.loaded {
		return "loading plugins…"
	}
	if p.loadErr != nil {
		return styleError.Render("herdr plugin list: "+p.loadErr.Error()) + "\n" + styleDim.Render("i installs a plugin by owner/repo · m opens the marketplace")
	}
	if len(p.plugins) == 0 {
		return "No plugins installed.\n" + styleDim.Render("i installs a plugin by owner/repo · m opens the marketplace")
	}
	footer := p.footer()
	listHeight := max(height-len(lines)-strings.Count(footer, "\n")-2, 3)
	nameWidth := min(max(width/4, 18), 32)
	body := p.list.view(listHeight, func(i int, selected bool) string {
		r := p.rows[i]
		pl := p.plugins[r.plugin]
		if r.action >= 0 {
			a := pl.Actions[r.action]
			return row("    ▸ "+firstNonEmpty(a.Title, a.ID)+styleDim.Render("  "+a.QualifiedID(pl.ID)), selected)
		}
		return row(p.pluginLine(pl, nameWidth), selected)
	})
	lines = append(lines, fit(body, width, listHeight), "", footer)
	return strings.Join(lines, "\n")
}

func (p *pluginsPage) pluginLine(pl plugins.Plugin, nameWidth int) string {
	status := p.statuses[pl.ID]
	dot := styleDim.Render("·")
	switch {
	case !pl.GitHub():
		dot = styleDim.Render("⌂")
	case status.State == plugins.Current:
		dot = styleOK.Render("●")
	case status.State == plugins.Behind:
		dot = styleWarn.Render("↑")
	case status.State == plugins.Failed:
		dot = styleError.Render("?")
	}
	marker := " "
	if p.expanded[pl.ID] {
		marker = "▾"
	} else if len(pl.Actions) > 0 {
		marker = "▸"
	}
	version := pl.Version
	if status.State == plugins.Behind && status.Version != "" && status.Version != pl.Version {
		version += styleWarn.Render(" → " + status.Version)
	} else if status.State == plugins.Behind {
		version += styleWarn.Render(" → " + short(status.Remote))
	}
	source := "linked " + pl.Root
	if pl.GitHub() {
		source = pl.Spec()
		if ref := pl.Ref(); ref != "" {
			if pl.Pinned() {
				ref = short(ref) + " (pinned)"
			}
			source += "@" + ref
		}
	}
	name := padRight(pl.Name, nameWidth)
	if !pl.Enabled {
		name = styleDim.Render(name) + styleDim.Render(" (disabled)")
	}
	return fmt.Sprintf("%s %s %s %s  %s", dot, marker, name, padRight(version, 8), styleDim.Render(source))
}

func (p *pluginsPage) footer() string {
	pl, action := p.selected()
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
		text = styleWarn.Render(fmt.Sprintf("↑ %d update(s) available: u this row · U all", behind))
	}
	if failed > 0 {
		text += styleError.Render(fmt.Sprintf(" · %d could not be checked", failed))
	}
	if pl != nil {
		if s := p.statuses[pl.ID]; s.State == plugins.Failed && s.Err != nil {
			text += "\n" + styleError.Render(s.Err.Error())
		} else if action == nil && pl.Description != "" {
			text += "\n" + styleDim.Render(pl.Description)
		}
	}
	return text
}

func (p *pluginsPage) marketView(width, height int, lines []string) string {
	sort := "most stars"
	if p.marketSort == "updated" {
		sort = "recently updated"
	}
	heading := fmt.Sprintf("Marketplace · topic:%s · %s", plugins.MarketTopic, sort)
	if p.marketQuery != "" {
		heading += fmt.Sprintf(" · %q", p.marketQuery)
	}
	if p.marketTotal > 0 {
		heading += fmt.Sprintf(" · %d of %d loaded", len(p.repos), p.marketTotal)
	}
	lines = append(lines, styleHeading.Render(heading))
	switch {
	case p.marketErr != nil:
		lines = append(lines, styleError.Render(p.marketErr.Error()), styleDim.Render("r retries"))
	case p.repos == nil && p.marketBusy:
		lines = append(lines, styleDim.Render("searching GitHub…"))
	case len(p.repos) == 0:
		lines = append(lines, styleDim.Render("no plugins found"))
	default:
		listHeight := max(height-len(lines)-2, 3)
		body := p.market.view(listHeight, func(i int, selected bool) string {
			r := p.repos[i]
			mark := "  "
			if p.installed(r.FullName) {
				mark = styleOK.Render("✓ ")
			}
			return row(fmt.Sprintf("%s★ %-6d %-36s %s", mark, r.Stars, r.FullName, styleDim.Render(r.Description)), selected)
		})
		lines = append(lines, fit(body, width, listHeight))
		if p.marketBusy {
			lines = append(lines, styleDim.Render("loading more…"))
		}
	}
	return strings.Join(lines, "\n")
}

func (p *pluginsPage) logsView(width, height int, lines []string) string {
	lines = append(lines, styleHeading.Render("Recent commands · "+p.logsFor))
	if p.logsErr != nil {
		return strings.Join(append(lines, styleError.Render(p.logsErr.Error())), "\n")
	}
	if len(p.logs) == 0 {
		return strings.Join(append(lines, styleDim.Render("no commands recorded")), "\n")
	}
	detail := p.logDetail(width)
	listHeight := max(height-len(lines)-len(detail)-1, 3)
	body := p.logsList.view(listHeight, func(i int, selected bool) string {
		l := p.logs[i]
		status := styleOK.Render(padRight(l.Status, 9))
		if l.Status != "succeeded" {
			status = styleError.Render(padRight(l.Status, 9))
		}
		return row(fmt.Sprintf("%s %s %s", styleDim.Render(l.StartedAt().Format(time.DateTime)), status, l.Trigger()), selected)
	})
	lines = append(lines, fit(body, width, listHeight), "")
	return strings.Join(append(lines, detail...), "\n")
}

func (p *pluginsPage) logDetail(width int) []string {
	i := p.logsList.selected()
	if i < 0 || i >= len(p.logs) {
		return nil
	}
	l := p.logs[i]
	lines := []string{styleDim.Render("$ " + strings.Join(l.Command, " "))}
	if l.ExitCode != nil {
		lines[0] += styleDim.Render(fmt.Sprintf("  exit %d · %dms", *l.ExitCode, l.Finished-l.Started))
	}
	tail := func(label, text string, style func(...string) string) {
		text = strings.TrimSpace(text)
		if text == "" {
			return
		}
		out := strings.Split(text, "\n")
		out = out[max(len(out)-4, 0):]
		lines = append(lines, styleDim.Render(label))
		for _, line := range out {
			lines = append(lines, style("  "+clip(line, width-2)))
		}
	}
	tail("stdout", l.Stdout, styleDim.Render)
	tail("stderr", l.Stderr, styleError.Render)
	return lines
}
