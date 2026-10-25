package ui

import (
	"errors"
	"fmt"
	"slices"
	"strconv"
	"strings"

	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"

	"github.com/wapenshaw/herdr-customize/internal/config"
	"github.com/wapenshaw/herdr-customize/internal/keys"
	"github.com/wapenshaw/herdr-customize/internal/plugins"
	"github.com/wapenshaw/herdr-customize/internal/schema"
)

var commandPath = []string{"keys", "command"}

// commandTypes are the [[keys.command]] kinds Herdr accepts.
var commandTypes = []string{"plugin_action", "popup", "pane", "shell"}

type keyRowKind int

const (
	rowHeader keyRowKind = iota
	rowBuiltin
	rowCommand
	rowPluginAction
)

type keyRow struct {
	kind  keyRowKind
	label string
	// built-in action
	key     schema.Key
	action  string // name under [keys]
	values  []string
	changed bool
	// custom command
	index   int
	command map[string]any
	missing bool // plugin_action that is not installed
	// plugin action without a key
	qualified, title string
	// shared
	owner    string
	clashes  []string
	problems []string
}

type keysMode int

const (
	keysBrowse keysMode = iota
	keysEditing
	keysForm
	keysConfirm
)

type keysPage struct {
	sh      *shared
	rows    []keyRow
	list    list
	plugins []plugins.Plugin
	loadErr error
	mode    keysMode
	ask     *confirm
	// binding editor
	input     textinput.Model
	editing   *keyRow
	recording bool
	// custom command form
	form commandForm
}

// bindActionMsg asks the Keys page to bind a plugin action.
type bindActionMsg struct{ qualified, title string }

func newKeysPage(sh *shared) page {
	l := newList()
	p := &keysPage{sh: sh, list: l}
	p.list.header = func(i int) bool { return i < len(p.rows) && p.rows[i].kind == rowHeader }
	return p
}

func (p *keysPage) title() string { return "Keys" }

func (p *keysPage) init() tea.Cmd { p.rebuild(); return nil }

func (p *keysPage) capturing() bool { return p.mode != keysBrowse || p.list.filtering }

func (p *keysPage) back() bool { return p.list.clearFilter() }

func (p *keysPage) help() string {
	switch p.mode {
	case keysEditing:
		if p.recording {
			return "press the key to record · esc stop recording"
		}
		return "↵ save · ctrl+r record a key · empty resets to default · esc cancel"
	case keysForm:
		return "tab/shift+tab field · ←/→ change type · ↵ save · esc cancel"
	case keysConfirm:
		return p.ask.question + " [y/N]"
	}
	r := p.current()
	switch {
	case r == nil:
		return "/ search · n new command"
	case r.kind == rowBuiltin:
		return "↵ change · x reset to default · / search · n new command"
	case r.kind == rowCommand:
		return "↵ edit · d delete · / search · n new command"
	case r.kind == rowPluginAction:
		return "↵ bind a key · / search · n new command"
	}
	return ""
}

func (p *keysPage) current() *keyRow {
	if i := p.list.selected(); i >= 0 && i < len(p.rows) && p.rows[i].kind != rowHeader {
		return &p.rows[i]
	}
	return nil
}

// rebuild reads the config and recomputes rows, conflicts and problems.
func (p *keysPage) rebuild() {
	doc, err := p.sh.store.Load()
	p.loadErr = err
	if err != nil {
		doc, _ = config.Parse(nil)
	}
	var rows []keyRow
	rows = append(rows, keyRow{kind: rowHeader, label: "Herdr actions"})
	if section, ok := p.sh.ref.Section("keys"); ok {
		for _, k := range section.Keys {
			path := k.Path()
			current, set := doc.Get(path...)
			values := keys.Values(current)
			if !set {
				if def, ok := k.DefaultValue(); ok {
					values = keys.Values(def)
				}
			}
			action := strings.TrimPrefix(k.Key, "keys.")
			if strings.HasPrefix(action, "indexed.") {
				// Indexed jumps take a modifier combo for 1..9.
				for i, v := range values {
					values[i] = v + "+1..9"
				}
			}
			rows = append(rows, keyRow{kind: rowBuiltin, label: action + " " + strings.Join(values, " "), key: k, action: action, values: values, changed: set, owner: k.Key})
		}
	}
	installed := map[string]bool{}
	var unbound []keyRow
	for _, pl := range p.plugins {
		for _, a := range pl.Actions {
			installed[a.QualifiedID(pl.ID)] = true
		}
	}
	bound := map[string]bool{}
	rows = append(rows, keyRow{kind: rowHeader, label: "Custom commands"})
	for i, entry := range doc.ArrayTable(commandPath) {
		key, _ := entry["key"].(string)
		kind, _ := entry["type"].(string)
		command, _ := entry["command"].(string)
		desc, _ := entry["description"].(string)
		r := keyRow{kind: rowCommand, label: strings.Join([]string{key, kind, command, desc}, " "), index: i, command: entry, values: keys.Values(key), owner: fmt.Sprintf("command %d (%s)", i+1, firstNonEmpty(desc, command))}
		if kind == "plugin_action" {
			bound[command] = true
			r.missing = p.plugins != nil && !installed[command] && !installedSuffix(installed, command)
		}
		rows = append(rows, r)
	}
	for _, pl := range p.plugins {
		if !pl.Enabled {
			continue
		}
		for _, a := range pl.Actions {
			q := a.QualifiedID(pl.ID)
			if bound[q] || !a.Available() {
				continue
			}
			title := firstNonEmpty(a.Title, a.ID)
			unbound = append(unbound, keyRow{kind: rowPluginAction, label: q + " " + title, qualified: q, title: title})
		}
	}
	if len(unbound) > 0 {
		rows = append(rows, keyRow{kind: rowHeader, label: "Plugin actions without a key"})
		rows = append(rows, unbound...)
	}
	p.rows = rows
	p.annotate()
	labels := make([]string, len(rows))
	for i, r := range rows {
		labels[i] = r.label
	}
	p.list.setRows(labels)
}

// installedSuffix accepts an unqualified action id that matches exactly one
// installed action, which Herdr resolves too.
func installedSuffix(installed map[string]bool, command string) bool {
	if strings.Contains(command, ".") {
		return false
	}
	n := 0
	for q := range installed {
		if strings.HasSuffix(q, "."+command) {
			n++
		}
	}
	return n == 1
}

func (p *keysPage) bindings() []keys.Binding {
	var out []keys.Binding
	for _, r := range p.rows {
		if r.kind == rowBuiltin || r.kind == rowCommand {
			out = append(out, keys.Binding{Owner: r.owner, Keys: r.values, Navigate: r.kind == rowBuiltin && keys.Navigate(r.action)})
		}
	}
	return out
}

func (p *keysPage) annotate() {
	conflicts := keys.Conflicts(p.bindings())
	for i := range p.rows {
		r := &p.rows[i]
		if r.kind != rowBuiltin && r.kind != rowCommand {
			continue
		}
		r.clashes = keys.Clashes(conflicts, keys.Binding{Owner: r.owner, Keys: r.values, Navigate: r.kind == rowBuiltin && keys.Navigate(r.action)})
		r.problems = nil
		action := r.action
		if r.kind == rowCommand {
			action = "command"
		}
		if r.kind == rowBuiltin && r.key.Type != schema.TypeKeybinding {
			continue
		}
		for _, v := range r.values {
			r.problems = append(r.problems, keys.Problems(action, v)...)
		}
	}
}

func (p *keysPage) update(msg tea.Msg) tea.Cmd {
	switch msg := msg.(type) {
	case pluginsMsg:
		if msg.err == nil {
			p.plugins = msg.list
		}
		p.rebuild()
	case referenceMsg, savedMsg:
		p.rebuild()
	case bindActionMsg:
		p.mode = keysForm
		p.form = newCommandForm(map[string]any{"type": "plugin_action", "command": msg.qualified, "description": msg.title}, -1)
		return textinput.Blink
	case tea.KeyMsg:
		return p.handleKey(msg)
	}
	return nil
}

func (p *keysPage) handleKey(msg tea.KeyMsg) tea.Cmd {
	switch p.mode {
	case keysConfirm:
		p.mode = keysBrowse
		ask := p.ask
		p.ask = nil
		return ask.handle(msg)
	case keysEditing:
		return p.handleEditKey(msg)
	case keysForm:
		return p.handleFormKey(msg)
	}
	if handled, cmd := p.list.handleKey(msg); handled {
		return cmd
	}
	r := p.current()
	switch msg.String() {
	case "n":
		p.mode = keysForm
		p.form = newCommandForm(map[string]any{"type": "popup"}, -1)
		return textinput.Blink
	case "r":
		p.rebuild()
		return loadPlugins
	}
	if r == nil {
		return nil
	}
	switch {
	case r.kind == rowBuiltin && msg.String() == "enter":
		p.editing = r
		p.mode = keysEditing
		p.recording = false
		value := strings.Join(r.values, ", ")
		if strings.HasPrefix(r.action, "indexed.") {
			value = strings.ReplaceAll(value, "+1..9", "")
		}
		p.input = newInput(r.action+" = ", value)
		return textinput.Blink
	case r.kind == rowBuiltin && msg.String() == "x":
		if !r.changed {
			return setStatus(r.action + " already uses the default")
		}
		path := r.key.Path()
		return p.sh.save(r.action+" default", func(doc *config.Doc) error {
			_, err := doc.Unset(path)
			return err
		})
	case r.kind == rowCommand && msg.String() == "enter":
		p.mode = keysForm
		p.form = newCommandForm(r.command, r.index)
		return textinput.Blink
	case r.kind == rowCommand && msg.String() == "d":
		index := r.index
		p.mode = keysConfirm
		p.ask = &confirm{question: fmt.Sprintf("Delete %s?", r.owner), yes: func() tea.Cmd {
			return p.sh.save("command removal", func(doc *config.Doc) error { return doc.DeleteArrayTable(commandPath, index) })
		}}
	case r.kind == rowPluginAction && msg.String() == "enter":
		return p.update(bindActionMsg{qualified: r.qualified, title: r.title})
	}
	return nil
}

func (p *keysPage) handleEditKey(msg tea.KeyMsg) tea.Cmd {
	if p.recording {
		if msg.String() == "esc" {
			p.recording = false
			return nil
		}
		if key, ok := recordKey(msg); ok {
			p.input.SetValue(key)
			p.input.CursorEnd()
			p.recording = false
		}
		return nil
	}
	switch msg.String() {
	case "esc":
		p.mode, p.editing = keysBrowse, nil
		return nil
	case "ctrl+r":
		p.recording = true
		return nil
	case "enter":
		r := p.editing
		p.mode, p.editing = keysBrowse, nil
		values := splitValues(p.input.Value())
		path := r.key.Path()
		if len(values) == 0 {
			return p.sh.save(r.action+" default", func(doc *config.Doc) error {
				_, err := doc.Unset(path)
				return err
			})
		}
		var value any = values[0]
		if len(values) > 1 {
			value = values
		}
		return p.sh.save(r.action, func(doc *config.Doc) error { return doc.Set(path, value) })
	}
	var cmd tea.Cmd
	p.input, cmd = p.input.Update(msg)
	return cmd
}

func splitValues(text string) []string {
	var out []string
	for _, v := range strings.Split(text, ",") {
		if v = strings.TrimSpace(v); v != "" {
			out = append(out, v)
		}
	}
	return out
}

// recordKey turns a key press into Herdr notation. A plain printable key
// becomes a prefix binding, since a direct one would intercept typing.
func recordKey(msg tea.KeyMsg) (string, bool) {
	name := msg.String()
	replacer := strings.NewReplacer("pgup", "pageup", "pgdown", "pagedown")
	name = replacer.Replace(name)
	if name == " " {
		name = "space"
	}
	c, err := keys.Parse(name)
	if err != nil {
		return "", false
	}
	if c.Plain() {
		c.Prefix = true
	}
	return c.String(), true
}

// editHints lists problems and clashes for the value being typed.
func (p *keysPage) editHints() []string {
	r := p.editing
	values := splitValues(p.input.Value())
	if strings.HasPrefix(r.action, "indexed.") {
		for i, v := range values {
			values[i] = v + "+1..9"
		}
	}
	var hints []string
	if r.key.Type == schema.TypeKeybinding {
		for _, v := range values {
			hints = append(hints, keys.Problems(r.action, v)...)
		}
	}
	var others []keys.Binding
	for _, b := range p.bindings() {
		if b.Owner != r.owner {
			others = append(others, b)
		}
	}
	self := keys.Binding{Owner: r.owner, Keys: values, Navigate: keys.Navigate(r.action)}
	if clashes := keys.Clashes(keys.Conflicts(append(others, self)), self); len(clashes) > 0 {
		hints = append(hints, "also used by "+strings.Join(clashes, ", "))
	}
	if len(values) == 0 {
		hints = append(hints, "empty: reset to the default ("+r.key.Default+")")
	}
	return hints
}

func (p *keysPage) view(width, height int) string {
	if p.mode == keysForm {
		return p.form.view(p)
	}
	detail := p.detail()
	listHeight := max(height-len(detail)-1, 3)
	conflicts := 0
	for _, r := range p.rows {
		if len(r.clashes) > 0 {
			conflicts++
		}
	}
	body := p.list.view(listHeight, func(i int, selected bool) string {
		r := p.rows[i]
		switch r.kind {
		case rowHeader:
			return styleHeading.Render(r.label)
		case rowBuiltin:
			text := fmt.Sprintf("%-28s %-24s", r.action, strings.Join(r.values, ", "))
			if r.changed {
				text = fmt.Sprintf("%-28s %-24s", r.action, styleAccent.Render(padRight(strings.Join(r.values, ", "), 24)))
			}
			return row(text+p.marks(r), selected)
		case rowCommand:
			key, _ := r.command["key"].(string)
			kind, _ := r.command["type"].(string)
			command, _ := r.command["command"].(string)
			desc, _ := r.command["description"].(string)
			text := fmt.Sprintf("%-18s %-14s %s", key, kind, command)
			if desc != "" {
				text += styleDim.Render("  " + desc)
			}
			return row(text+p.marks(r), selected)
		case rowPluginAction:
			return row(fmt.Sprintf("%-40s %s", r.qualified, styleDim.Render(r.title)), selected)
		}
		return ""
	})
	if conflicts > 0 && p.mode == keysBrowse {
		body = styleWarn.Render(fmt.Sprintf("%d bindings share a key with another binding (⚠)", conflicts)) + "\n" + body
	} else {
		body = "\n" + body
	}
	return fit(body, width, listHeight+1) + "\n" + strings.Join(detail, "\n")
}

func (p *keysPage) marks(r keyRow) string {
	var marks []string
	if len(r.clashes) > 0 {
		marks = append(marks, styleWarn.Render("⚠ conflict"))
	}
	if len(r.problems) > 0 {
		marks = append(marks, styleError.Render("✗ "+r.problems[0]))
	}
	if r.missing {
		marks = append(marks, styleError.Render("✗ action not installed"))
	}
	if len(marks) == 0 {
		return ""
	}
	return " " + strings.Join(marks, " ")
}

// detail describes the selected row, or the binding being edited.
func (p *keysPage) detail() []string {
	if p.loadErr != nil {
		return []string{styleError.Render("config: " + p.loadErr.Error())}
	}
	if p.mode == keysEditing {
		lines := []string{p.input.View()}
		if p.recording {
			lines = append(lines, styleAccent.Render("recording: press the key combination"))
		}
		for _, h := range p.editHints() {
			lines = append(lines, styleWarn.Render("  "+h))
		}
		return lines
	}
	r := p.current()
	if r == nil {
		return nil
	}
	var lines []string
	switch r.kind {
	case rowBuiltin:
		lines = append(lines, styleDim.Render(r.key.Description+" Default: "+r.key.Default))
	case rowCommand:
		if r.missing {
			lines = append(lines, styleError.Render("No installed plugin has this action. Edit it to point at an installed one, or delete it."))
		}
	case rowPluginAction:
		lines = append(lines, styleDim.Render("Not bound to any key. Press enter to add a binding."))
	}
	if len(r.clashes) > 0 {
		lines = append(lines, styleWarn.Render("Same key as: "+strings.Join(r.clashes, ", ")))
	}
	for _, problem := range r.problems {
		lines = append(lines, styleError.Render(problem))
	}
	return lines
}

func padRight(s string, n int) string {
	if len(s) >= n {
		return s
	}
	return s + strings.Repeat(" ", n-len(s))
}

func firstNonEmpty(values ...string) string {
	for _, v := range values {
		if v != "" {
			return v
		}
	}
	return ""
}

// commandForm edits one [[keys.command]] entry.
type commandForm struct {
	index    int // -1 for a new entry
	typ      int
	fields   []textinput.Model
	names    []string
	focus    int
	original map[string]any
}

var formFields = []string{"key", "command", "description", "width", "height"}

func newCommandForm(entry map[string]any, index int) commandForm {
	f := commandForm{index: index, names: formFields, original: entry}
	kind, _ := entry["type"].(string)
	f.typ = max(slices.Index(commandTypes, kind), 0)
	for _, name := range formFields {
		value := ""
		switch v := entry[name].(type) {
		case string:
			value = v
		case int64:
			value = strconv.FormatInt(v, 10)
		}
		in := newInput(fmt.Sprintf("%-12s", name), value)
		in.Blur()
		f.fields = append(f.fields, in)
	}
	f.fields[f.focus].Focus()
	return f
}

func (f *commandForm) value(name string) string {
	return strings.TrimSpace(f.fields[slices.Index(f.names, name)].Value())
}

func (f *commandForm) entry() ([]config.Field, error) {
	key := f.value("key")
	if key == "" {
		return nil, errors.New("a key is required")
	}
	if problems := keys.Problems("command", key); len(problems) > 0 {
		return nil, errors.New(problems[0])
	}
	command := f.value("command")
	if command == "" {
		return nil, errors.New("a command is required")
	}
	fields := []config.Field{{Key: "key", Value: key}, {Key: "type", Value: commandTypes[f.typ]}, {Key: "command", Value: command}}
	if desc := f.value("description"); desc != "" {
		fields = append(fields, config.Field{Key: "description", Value: desc})
	}
	for _, name := range []string{"width", "height"} {
		value := f.value(name)
		if value == "" || commandTypes[f.typ] != "popup" {
			continue
		}
		if n, err := strconv.ParseInt(value, 10, 64); err == nil {
			fields = append(fields, config.Field{Key: name, Value: n})
		} else if strings.HasSuffix(value, "%") {
			fields = append(fields, config.Field{Key: name, Value: value})
		} else {
			return nil, fmt.Errorf("%s must be a number of cells or a percentage", name)
		}
	}
	// Keep keys this form does not know about.
	for key, value := range f.original {
		if !slices.Contains(append([]string{"type"}, formFields...), key) {
			fields = append(fields, config.Field{Key: key, Value: value})
		}
	}
	return fields, nil
}

func (p *keysPage) handleFormKey(msg tea.KeyMsg) tea.Cmd {
	f := &p.form
	switch msg.String() {
	case "esc":
		p.mode = keysBrowse
		return nil
	case "tab", "down":
		f.fields[f.focus].Blur()
		f.focus = (f.focus + 1) % len(f.fields)
		return f.fields[f.focus].Focus()
	case "shift+tab", "up":
		f.fields[f.focus].Blur()
		f.focus = (f.focus + len(f.fields) - 1) % len(f.fields)
		return f.fields[f.focus].Focus()
	case "ctrl+right", "ctrl+n":
		f.typ = (f.typ + 1) % len(commandTypes)
		return nil
	case "ctrl+left", "ctrl+p":
		f.typ = (f.typ + len(commandTypes) - 1) % len(commandTypes)
		return nil
	case "enter":
		fields, err := f.entry()
		if err != nil {
			return setError(err)
		}
		p.mode = keysBrowse
		if f.index < 0 {
			return p.sh.save("new command", func(doc *config.Doc) error { return doc.AppendArrayTable(commandPath, fields) })
		}
		index := f.index
		return p.sh.save("command", func(doc *config.Doc) error { return doc.UpdateArrayTable(commandPath, index, fields) })
	}
	var cmd tea.Cmd
	f.fields[f.focus], cmd = f.fields[f.focus].Update(msg)
	return cmd
}

func (f *commandForm) view(p *keysPage) string {
	title := "New custom command"
	if f.index >= 0 {
		title = fmt.Sprintf("Edit custom command %d", f.index+1)
	}
	lines := []string{styleHeading.Render(title), ""}
	var types []string
	for i, t := range commandTypes {
		if i == f.typ {
			types = append(types, styleTabActive.Render(" "+t+" "))
		} else {
			types = append(types, styleDim.Render(" "+t+" "))
		}
	}
	lines = append(lines, fmt.Sprintf("%-12s", "type")+strings.Join(types, " ")+styleDim.Render("   ctrl+←/→ change"))
	for i, in := range f.fields {
		if (f.names[i] == "width" || f.names[i] == "height") && commandTypes[f.typ] != "popup" {
			continue
		}
		lines = append(lines, in.View())
	}
	lines = append(lines, "")
	switch commandTypes[f.typ] {
	case "plugin_action":
		lines = append(lines, styleDim.Render("command is a plugin action id such as herdr-customize.open"))
		if q := f.value("command"); q != "" && p.plugins != nil {
			found := false
			for _, pl := range p.plugins {
				for _, a := range pl.Actions {
					found = found || a.QualifiedID(pl.ID) == q
				}
			}
			if !found {
				lines = append(lines, styleWarn.Render("no installed plugin has the action "+q))
			}
		}
	case "popup":
		lines = append(lines, styleDim.Render("opens a popup running command; width/height are cells or a percentage"))
	case "pane":
		lines = append(lines, styleDim.Render("opens a temporary zoomed pane running command"))
	case "shell":
		lines = append(lines, styleDim.Render("runs command detached in the background"))
	}
	if key := f.value("key"); key != "" {
		for _, problem := range keys.Problems("command", key) {
			lines = append(lines, styleWarn.Render(problem))
		}
		var others []keys.Binding
		owner := "editing"
		for _, b := range p.bindings() {
			if f.index < 0 || b.Owner != p.commandOwner(f.index) {
				others = append(others, b)
			}
		}
		self := keys.Binding{Owner: owner, Keys: []string{key}}
		if clashes := keys.Clashes(keys.Conflicts(append(others, self)), self); len(clashes) > 0 {
			lines = append(lines, styleWarn.Render("also used by "+strings.Join(clashes, ", ")))
		}
	}
	return strings.Join(lines, "\n")
}

func (p *keysPage) commandOwner(index int) string {
	for _, r := range p.rows {
		if r.kind == rowCommand && r.index == index {
			return r.owner
		}
	}
	return ""
}
