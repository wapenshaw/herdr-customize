package ui

import (
	"fmt"
	"os"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/pelletier/go-toml/v2"

	"github.com/wapenshaw/herdr-customize/internal/config"
	"github.com/wapenshaw/herdr-customize/internal/herdr"
	"github.com/wapenshaw/herdr-customize/internal/schema"
	"github.com/wapenshaw/herdr-customize/internal/theme"
)

type settingRow struct {
	header  string
	key     schema.Key
	unknown []string // a key in config.toml that Herdr does not document
	value   any
	set     bool
}

func (r settingRow) path() []string {
	if r.unknown != nil {
		return r.unknown
	}
	return r.key.Path()
}

func (r settingRow) name() string { return strings.Join(r.path(), ".") }

type settingsMode int

const (
	settingsBrowse settingsMode = iota
	settingsEditing
	settingsConfirm
)

type settingsPage struct {
	sh      *shared
	rows    []settingRow
	list    list
	loadErr error
	issues  []string
	mode    settingsMode
	input   textinput.Model
	// pending is a value picked with ←/→ that is saved once the keys stop.
	pending *pendingValue
	ask     *confirm
	// raw holds an edit of the whole file that failed the check, so the next
	// edit starts from it rather than losing the work.
	raw []byte
	// assumed holds values being saved, shown until their saves report back
	// so a row does not flash its old value.
	assumed map[string]assumedValue
}

type assumedValue struct {
	value    any
	set      bool
	inflight int
}

type pendingValue struct {
	key   string
	path  []string
	value any
	seq   int
}

// pickDelay is how long ←/→ must rest before the picked value is saved, so
// cycling through values writes the config once.
var pickDelay = 400 * time.Millisecond

type (
	pendingSaveMsg struct{ seq int }
	// settingSavedMsg wraps the savedMsg of one key's save.
	settingSavedMsg struct {
		name  string
		saved savedMsg
	}
	issuesMsg    struct{ result herdr.CheckResult }
	rawEditedMsg struct {
		path string
		err  error
	}
)

// rawWhat names saves of the whole file from the editor.
const rawWhat = "config.toml"

func newSettingsPage(sh *shared) page {
	p := &settingsPage{sh: sh, list: newList(), assumed: map[string]assumedValue{}}
	p.list.header = func(i int) bool { return i < len(p.rows) && p.rows[i].header != "" }
	return p
}

func (p *settingsPage) title() string { return "Settings" }

func (p *settingsPage) init() tea.Cmd {
	p.rebuild()
	return p.check
}

func (p *settingsPage) capturing() bool { return p.mode != settingsBrowse || p.list.filtering }

func (p *settingsPage) back() bool { return p.list.clearFilter() }

func (p *settingsPage) help() string {
	switch p.mode {
	case settingsEditing:
		return "↵ save · esc cancel"
	case settingsConfirm:
		return p.ask.question + " [y/N]"
	}
	r := p.current()
	switch {
	case r == nil:
		return "/ search · E edit config.toml"
	case r.unknown != nil:
		return "x remove · / search · E edit config.toml"
	case r.key.Open():
		return "←/→ pick · ↵ type a value · x reset · / search · E edit config.toml"
	case len(options(r.key)) > 0:
		return "←/→ pick · x reset · / search · E edit config.toml"
	}
	return "↵ change · x reset · / search · E edit config.toml"
}

func (p *settingsPage) current() *settingRow {
	if i := p.list.selected(); i >= 0 && i < len(p.rows) && p.rows[i].header == "" {
		return &p.rows[i]
	}
	return nil
}

// check runs herdr config check on the file as it is.
func (p *settingsPage) check() tea.Msg {
	return issuesMsg{result: herdr.CheckConfig(p.sh.store.Path)}
}

func (p *settingsPage) rebuild() {
	doc, err := p.sh.store.Load()
	p.loadErr = err
	if err != nil {
		doc, _ = config.Parse(nil)
	}
	var rows []settingRow
	for _, section := range p.sh.ref.Sections {
		// Keybindings and themes have their own pages.
		if section.ID == "keys" || section.ID == "theme" {
			continue
		}
		rows = append(rows, settingRow{header: section.Title})
		for _, k := range section.Keys {
			value, set := doc.Get(k.Path()...)
			rows = append(rows, settingRow{key: k, value: value, set: set})
		}
	}
	if unknown := unknownKeys(doc.Map(), p.sh.ref, nil); len(unknown) > 0 {
		rows = append(rows, settingRow{header: "Not Herdr settings"})
		for _, path := range unknown {
			value, _ := doc.Get(path...)
			rows = append(rows, settingRow{unknown: path, value: value, set: true})
		}
	}
	p.rows = rows
	p.applyAssumed()
	labels := make([]string, len(rows))
	for i, r := range rows {
		if r.header != "" {
			labels[i] = r.header
		} else {
			labels[i] = r.name() + " " + r.key.Description
		}
	}
	p.list.setRows(labels)
}

// unknownKeys finds leaf keys that no reference key covers. Keybindings are
// left to the Keys page and plugin tables are left alone.
func unknownKeys(table map[string]any, ref *schema.Reference, prefix []string) [][]string {
	var out [][]string
	names := make([]string, 0, len(table))
	for name := range table {
		names = append(names, name)
	}
	slices.Sort(names)
	for _, name := range names {
		path := append(slices.Clone(prefix), name)
		dotted := strings.Join(path, ".")
		if dotted == "keys" {
			continue
		}
		if _, ok := ref.Lookup(dotted); ok {
			continue
		}
		if sub, ok := table[name].(map[string]any); ok && coversPrefix(ref, dotted) {
			out = append(out, unknownKeys(sub, ref, path)...)
			continue
		}
		out = append(out, path)
	}
	return out
}

func coversPrefix(ref *schema.Reference, prefix string) bool {
	for _, s := range ref.Sections {
		for _, k := range s.Keys {
			if strings.HasPrefix(k.Key, prefix+".") {
				return true
			}
		}
	}
	return false
}

// write saves one key, or removes it when set is false. The row shows the
// new value at once.
func (p *settingsPage) write(what string, path []string, value any, set bool) tea.Cmd {
	name := strings.Join(path, ".")
	a := p.assumed[name]
	p.assumed[name] = assumedValue{value: value, set: set, inflight: a.inflight + 1}
	p.applyAssumed()
	save := p.sh.save(what, func(doc *config.Doc) error {
		if !set {
			_, err := doc.Unset(path)
			return err
		}
		return doc.Set(path, value)
	})
	return func() tea.Msg { return settingSavedMsg{name: name, saved: save().(savedMsg)} }
}

func (p *settingsPage) applyAssumed() {
	for i := range p.rows {
		if a, ok := p.assumed[p.rows[i].name()]; ok && p.rows[i].header == "" {
			p.rows[i].value, p.rows[i].set = a.value, a.set
		}
	}
}

func (p *settingsPage) update(msg tea.Msg) tea.Cmd {
	switch msg := msg.(type) {
	case settingSavedMsg:
		if a := p.assumed[msg.name]; a.inflight > 1 {
			a.inflight--
			p.assumed[msg.name] = a
		} else {
			delete(p.assumed, msg.name)
		}
		// Every page, and the status line, hears about the save.
		return func() tea.Msg { return msg.saved }
	case referenceMsg:
		p.rebuild()
	case savedMsg:
		p.rebuild()
		if msg.what == rawWhat && msg.err == nil {
			p.raw = nil
		}
		if msg.err == nil && msg.result.Changed {
			return p.check
		}
	case issuesMsg:
		p.issues = msg.result.Issues
	case pendingSaveMsg:
		if p.pending != nil && p.pending.seq == msg.seq {
			return p.flush()
		}
	case rawEditedMsg:
		return p.rawEdited(msg)
	case tea.KeyMsg:
		return p.handleKey(msg)
	}
	return nil
}

func (p *settingsPage) handleKey(msg tea.KeyMsg) tea.Cmd {
	switch p.mode {
	case settingsConfirm:
		p.mode = settingsBrowse
		ask := p.ask
		p.ask = nil
		return ask.handle(msg)
	case settingsEditing:
		return p.handleEditKey(msg)
	}
	if handled, cmd := p.list.handleKey(msg); handled {
		// Moving away saves a value still waiting on its delay.
		return tea.Batch(cmd, p.flush())
	}
	if msg.String() == "E" {
		return p.editRaw()
	}
	r := p.current()
	if r == nil {
		return nil
	}
	switch msg.String() {
	case "left", "right":
		if r.unknown == nil {
			return p.cycle(r, msg.String())
		}
	case "enter", " ":
		if r.unknown != nil {
			return nil
		}
		// Space steps through the listed values of a setting that also
		// takes others; Enter types one.
		if msg.String() == " " && r.key.Open() {
			return p.cycle(r, "right")
		}
		return p.edit(r)
	case "x":
		if !r.set {
			return setStatus(r.name() + " already uses the default")
		}
		path, name := r.path(), r.name()
		if r.unknown != nil {
			p.mode = settingsConfirm
			p.ask = &confirm{question: "Remove " + name + " from config.toml?", yes: func() tea.Cmd { return p.unset(name, path) }}
			return nil
		}
		return p.unset(name+" default", path)
	}
	return nil
}

func (p *settingsPage) unset(what string, path []string) tea.Cmd {
	return p.write(what, path, nil, false)
}

// edit starts changing r: booleans toggle, enums move to the next value and
// everything else gets a text field.
func (p *settingsPage) edit(r *settingRow) tea.Cmd {
	k := r.key
	if k.Type == schema.TypeBoolean {
		current, _ := r.value.(bool)
		if !r.set {
			def, _ := k.DefaultValue()
			current, _ = def.(bool)
		}
		return p.write(k.Key, k.Path(), !current, true)
	}
	if len(options(k)) > 0 && !k.Open() {
		return p.cycle(r, "right")
	}
	return p.editText(r)
}

// editText opens the text field for r.
func (p *settingsPage) editText(r *settingRow) tea.Cmd {
	k := r.key
	text := ""
	if r.set {
		text = editText(k, r.value)
	} else if def, ok := k.DefaultValue(); ok {
		text = editText(k, def)
	}
	p.input = newInput(k.Key+" = ", text)
	p.mode = settingsEditing
	return textinput.Blink
}

// structured types are edited as TOML values; the rest as plain text.
func structured(k schema.Key) bool {
	switch k.Type {
	case schema.TypeArray, schema.TypeStringList, schema.TypeTokenRows, schema.TypeTokenTables:
		return true
	}
	return false
}

func editText(k schema.Key, value any) string {
	if s, ok := value.(string); ok && !structured(k) {
		return s
	}
	if value == nil {
		return ""
	}
	if encoded, err := config.Encode(value); err == nil {
		return encoded
	}
	return fmt.Sprint(value)
}

var hexColor = regexp.MustCompile(`^#[0-9a-fA-F]{6}$`)

// parseValue turns the text field into the value to store.
func parseValue(k schema.Key, text string) (any, error) {
	switch {
	case k.Type == schema.TypeInteger:
		n, err := strconv.ParseInt(strings.TrimSpace(text), 10, 64)
		if err != nil {
			return nil, fmt.Errorf("%s needs a whole number", k.Key)
		}
		return n, nil
	case k.Type == schema.TypeFloat:
		n, err := strconv.ParseFloat(strings.TrimSpace(text), 64)
		if err != nil {
			return nil, fmt.Errorf("%s needs a number", k.Key)
		}
		return n, nil
	case structured(k):
		var document map[string]any
		if err := toml.Unmarshal([]byte("v = "+text), &document); err != nil {
			return nil, fmt.Errorf("%s needs a TOML value such as %s", k.Key, k.Default)
		}
		return document["v"], nil
	}
	return text, nil
}

func (p *settingsPage) handleEditKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "esc":
		p.mode = settingsBrowse
		return nil
	case "enter":
		r := p.current()
		if r == nil {
			p.mode = settingsBrowse
			return nil
		}
		value, err := parseValue(r.key, p.input.Value())
		if err != nil {
			return setError(err)
		}
		p.mode = settingsBrowse
		return p.write(r.key.Key, r.key.Path(), value, true)
	}
	var cmd tea.Cmd
	p.input, cmd = p.input.Update(msg)
	return cmd
}

// cycle moves r to the previous or next listed value, wrapping around. The
// save waits for pickDelay so stepping through several values writes once.
func (p *settingsPage) cycle(r *settingRow, dir string) tea.Cmd {
	var opts []option
	for _, o := range options(r.key) {
		if !o.custom {
			opts = append(opts, o)
		}
	}
	if len(opts) == 0 {
		return nil
	}
	value := effective(*r)
	if p.pending != nil && p.pending.key == r.key.Key {
		value = p.pending.value
	}
	i := step(selectedOption(opts, value), len(opts), dir)
	var flush tea.Cmd
	if p.pending != nil && p.pending.key != r.key.Key {
		flush = p.flush()
	}
	seq := 1
	if p.pending != nil {
		seq = p.pending.seq + 1
	}
	p.pending = &pendingValue{key: r.key.Key, path: r.key.Path(), value: opts[i].value, seq: seq}
	return tea.Batch(flush, tea.Tick(pickDelay, func(time.Time) tea.Msg { return pendingSaveMsg{seq: seq} }))
}

// step moves from option i (-1 for none) to the previous or next one.
func step(i, n int, dir string) int {
	switch {
	case i < 0 && dir == "left":
		return n - 1
	case dir == "left":
		return (i + n - 1) % n
	}
	return (i + 1) % n
}

// flush saves the pending value now.
func (p *settingsPage) flush() tea.Cmd {
	pending := p.pending
	if pending == nil {
		return nil
	}
	p.pending = nil
	return p.write(pending.key, pending.path, pending.value, true)
}

// option is one value offered in place on a setting's row.
type option struct {
	label  string
	value  any
	custom bool // opens the text field for a value not listed
}

// options lists the values offered for k: on/off for booleans, the known
// values otherwise, and "other…" when other values are valid too.
func options(k schema.Key) []option {
	if k.Type == schema.TypeBoolean {
		return []option{{label: "off", value: false}, {label: "on", value: true}}
	}
	choices := k.Choices()
	if len(choices) == 0 {
		return nil
	}
	out := make([]option, 0, len(choices)+1)
	for _, c := range choices {
		label := c
		if c == "" {
			label = "off"
		}
		out = append(out, option{label: label, value: c})
	}
	if k.Open() {
		out = append(out, option{label: "other…", custom: true})
	}
	return out
}

// effective is the value in force for r: its own, or the default.
func effective(r settingRow) any {
	if r.set {
		return r.value
	}
	if def, ok := r.key.DefaultValue(); ok {
		return def
	}
	// Some defaults are prose, such as compact or "stable" (with a note).
	text := r.key.Default
	if i := strings.Index(text, `"`); i >= 0 {
		if j := strings.Index(text[i+1:], `"`); j >= 0 {
			return text[i+1 : i+1+j]
		}
	}
	return text
}

func selectedOption(opts []option, value any) int {
	for i, o := range opts {
		if !o.custom && o.value == value {
			return i
		}
	}
	return -1
}

// strip renders a row's values with the one in force marked, or the one
// about to be saved highlighted.
func strip(r settingRow, opts []option, pending *pendingValue) string {
	value := effective(r)
	current := selectedOption(opts, value)
	picked := -1
	if pending != nil && pending.key == r.key.Key {
		picked = selectedOption(opts, pending.value)
	}
	parts := make([]string, len(opts))
	for i, o := range opts {
		label := o.label
		other := o.custom && current < 0 && r.set
		if other {
			label = formatValue(value)
		}
		switch {
		case i == picked:
			parts[i] = styleTabActive.Render(" " + label + " ")
		case picked < 0 && (i == current || other) && r.set:
			parts[i] = styleAccent.Bold(true).Render("[" + label + "]")
		case picked < 0 && (i == current || other):
			parts[i] = styleHeading.Render("[" + label + "]")
		default:
			parts[i] = styleDim.Render(" " + label + " ")
		}
	}
	text := strings.Join(parts, "")
	if r.set {
		text += " *"
	}
	return text
}

// editRaw opens config.toml in the user's editor through a scratch copy, so
// the edit is checked like any other before it replaces the file.
func (p *settingsPage) editRaw() tea.Cmd {
	data := p.raw
	if data == nil {
		doc, err := p.sh.store.Load()
		if err != nil {
			current, readErr := os.ReadFile(p.sh.store.Path)
			if readErr != nil {
				return setError(err)
			}
			data = current
		} else {
			data = doc.Bytes()
		}
	}
	file, err := os.CreateTemp("", "herdr-config-*.toml")
	if err != nil {
		return setError(err)
	}
	path := file.Name()
	_, err = file.Write(data)
	if closeErr := file.Close(); err == nil {
		err = closeErr
	}
	if err != nil {
		os.Remove(path)
		return setError(err)
	}
	cmd, err := theme.EditorCommand(path)
	if err != nil {
		os.Remove(path)
		return setError(err)
	}
	return tea.ExecProcess(cmd, func(err error) tea.Msg { return rawEditedMsg{path: path, err: err} })
}

func (p *settingsPage) rawEdited(msg rawEditedMsg) tea.Cmd {
	data, readErr := os.ReadFile(msg.path)
	os.Remove(msg.path)
	if msg.err != nil {
		return setError(fmt.Errorf("editor: %w", msg.err))
	}
	if readErr != nil {
		return setError(readErr)
	}
	doc, err := config.Parse(data)
	if err != nil {
		p.raw = data
		return setError(fmt.Errorf("not saved, press E to fix: %w", err))
	}
	// Kept until the save succeeds, so a rejected edit can be resumed.
	p.raw = data
	sh := p.sh
	return func() tea.Msg {
		sh.saving.Lock()
		defer sh.saving.Unlock()
		result, err := sh.store.Save(doc)
		if err != nil {
			err = fmt.Errorf("%w (press E to keep editing)", err)
		}
		return savedMsg{what: rawWhat, result: result, err: err}
	}
}

func (p *settingsPage) view(width, height int) string {
	var top []string
	if p.loadErr != nil {
		top = append(top, styleError.Render("config.toml: "+p.loadErr.Error()))
	}
	if len(p.issues) > 0 {
		top = append(top, styleWarn.Render(fmt.Sprintf("herdr config check: %d issue(s): %s", len(p.issues), p.issues[0])))
	}
	detail := p.detail(width)
	listHeight := max(height-len(top)-len(detail)-1, 3)
	keyWidth := min(max(width/3, 24), 40)
	body := p.list.view(listHeight, func(i int, selected bool) string {
		r := p.rows[i]
		if r.header != "" {
			return styleHeading.Render(r.header)
		}
		name := padRight(r.name(), keyWidth)
		if r.unknown == nil {
			if opts := options(r.key); len(opts) > 0 {
				return row(name+" "+strip(r, opts, p.pending), selected)
			}
		}
		return row(name+" "+p.display(r), selected)
	})
	lines := append(top, fit(body, width, listHeight), "")
	return strings.Join(append(lines, detail...), "\n")
}

// display renders the value column: set values stand out, defaults are dim.
func (p *settingsPage) display(r settingRow) string {
	if r.unknown != nil {
		return styleWarn.Render(formatValue(r.value)) + styleDim.Render("  not a Herdr setting")
	}
	if !r.set {
		return styleDim.Render(r.key.Default)
	}
	text := formatValue(r.value)
	if s, ok := r.value.(string); ok && hexColor.MatchString(s) {
		text = lipgloss.NewStyle().Foreground(lipgloss.Color(s)).Render("██") + " " + text
	}
	return styleAccent.Render(text) + " *"
}

func formatValue(value any) string {
	if s, ok := value.(string); ok {
		if s == "" {
			return `""`
		}
		return s
	}
	if encoded, err := config.Encode(value); err == nil {
		return encoded
	}
	return fmt.Sprint(value)
}

func (p *settingsPage) detail(width int) []string {
	if p.mode == settingsEditing {
		return []string{p.input.View()}
	}
	r := p.current()
	if r == nil {
		if p.mode == settingsBrowse && p.list.filter.Value() == "" {
			return []string{styleDim.Render("Keybindings are on the Keys page and themes on the Themes page.")}
		}
		return nil
	}
	if r.unknown != nil {
		return []string{styleWarn.Render(r.name() + " is not in Herdr's config reference " + p.sh.ref.Version + "; it may be a typo or belong to an older release.")}
	}
	k := r.key
	about := k.Type
	if len(k.Values) > 0 {
		about += ": " + strings.Join(k.Values, ", ")
	}
	lines := wrap(k.Description, width)
	lines = append(lines, styleDim.Render(about+" · default "+k.Default))
	lower := strings.ToLower(k.Description)
	if strings.Contains(lower, "server startup") || strings.Contains(lower, "restart") {
		lines = append(lines, styleWarn.Render("Takes effect when the Herdr server restarts."))
	}
	return lines
}

// wrap splits text into lines of at most width cells.
func wrap(text string, width int) []string {
	width = max(width, 20)
	var lines []string
	line := ""
	for _, word := range strings.Fields(text) {
		if line != "" && len(line)+1+len(word) > width {
			lines = append(lines, line)
			line = word
			continue
		}
		if line != "" {
			line += " "
		}
		line += word
	}
	if line != "" {
		lines = append(lines, line)
	}
	return lines
}
