package ui

import (
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"

	"github.com/wapenshaw/herdr-customize/internal/pluginconfig"
	"github.com/wapenshaw/herdr-customize/internal/plugins"
	"github.com/wapenshaw/herdr-customize/internal/schema"
	"github.com/wapenshaw/herdr-customize/internal/theme"
)

// configRow is a file heading (entry -1) or one of its settings.
type configRow struct {
	file  int
	entry int
}

// pluginConfig is the Plugins page screen for one plugin's own settings.
type pluginConfig struct {
	plugin  plugins.Plugin
	cfg     *pluginconfig.Config
	err     error
	rows    []configRow
	list    list
	editing bool
	input   textinput.Model
	// pending is a value picked with ←/→ in pendingFile, saved after
	// pickDelay like on the Settings page.
	pending     *pendingValue
	pendingFile string
}

type (
	pluginConfigMsg struct {
		id  string
		cfg *pluginconfig.Config
		err error
	}
	pluginConfigSavedMsg struct {
		id, what string
		err      error
	}
	pluginConfigPickMsg   struct{ seq int }
	pluginConfigReloadMsg struct{ id string }
)

func newPluginConfig(pl plugins.Plugin) *pluginConfig {
	c := &pluginConfig{plugin: pl, list: newList()}
	c.list.header = func(i int) bool { return i < len(c.rows) && c.rows[i].entry < 0 }
	return c
}

func loadPluginConfig(pl plugins.Plugin) tea.Cmd {
	return func() tea.Msg {
		dir, err := pluginconfig.Dir(pl.ID)
		if err != nil {
			return pluginConfigMsg{id: pl.ID, err: err}
		}
		cfg, err := pluginconfig.Load(dir, pl.Root)
		return pluginConfigMsg{id: pl.ID, cfg: cfg, err: err}
	}
}

func (c *pluginConfig) loaded(msg pluginConfigMsg) {
	if msg.id != c.plugin.ID {
		return
	}
	c.cfg, c.err = msg.cfg, msg.err
	c.rows = c.rows[:0]
	var labels []string
	if c.cfg != nil {
		for i, f := range c.cfg.Files {
			c.rows = append(c.rows, configRow{file: i, entry: -1})
			labels = append(labels, f.Name)
			for j, e := range f.Entries {
				c.rows = append(c.rows, configRow{file: i, entry: j})
				labels = append(labels, e.Key.Key+" "+e.Key.Description)
			}
		}
	}
	c.list.setRows(labels)
}

// current returns the file under the cursor and, on a setting, the setting.
func (c *pluginConfig) current() (*pluginconfig.File, *pluginconfig.Entry) {
	i := c.list.selected()
	if c.cfg == nil || i < 0 || i >= len(c.rows) {
		return nil, nil
	}
	r := c.rows[i]
	f := c.cfg.Files[r.file]
	if r.entry < 0 {
		return f, nil
	}
	return f, f.Entries[r.entry]
}

func asRow(e *pluginconfig.Entry) settingRow {
	return settingRow{key: e.Key, value: e.Value, set: e.Set}
}

func (c *pluginConfig) help() string {
	if c.editing {
		return "↵ save · esc cancel"
	}
	f, e := c.current()
	switch {
	case f == nil:
		return "r reload · esc back"
	case e == nil || e.ReadOnly:
		return "E edit file · / search · r reload · esc back"
	case e.Key.Open():
		return "←/→ pick · ↵ type a value · x reset · E edit file · esc back"
	case len(options(e.Key)) > 0:
		return "←/→ pick · x reset · E edit file · / search · esc back"
	}
	return "↵ change · x reset · E edit file · / search · esc back"
}

// claims takes ←/→ on settings with values to pick, and esc so a picked
// value is saved before going back.
func (c *pluginConfig) claims(msg tea.KeyMsg) bool {
	if c.editing || c.list.filtering {
		return false
	}
	switch msg.String() {
	case "esc":
		return true
	case "left", "right":
	default:
		return false
	}
	_, e := c.current()
	return e != nil && !e.ReadOnly && len(options(e.Key)) > 0
}

func (c *pluginConfig) update(msg tea.Msg) tea.Cmd {
	switch msg := msg.(type) {
	case pluginConfigMsg:
		c.loaded(msg)
	case pluginConfigSavedMsg:
		if msg.id != c.plugin.ID {
			return nil
		}
		reload := loadPluginConfig(c.plugin)
		if msg.err != nil {
			return tea.Batch(reload, setError(fmt.Errorf("%s: %w", msg.what, msg.err)))
		}
		return tea.Batch(reload, setStatus("saved "+msg.what+"; "+c.plugin.Name+" sees it the next time it reads its config"))
	case pluginConfigReloadMsg:
		if msg.id == c.plugin.ID {
			return loadPluginConfig(c.plugin)
		}
	case pluginConfigPickMsg:
		if c.pending != nil && c.pending.seq == msg.seq {
			return c.flush()
		}
	case tea.KeyMsg:
		return c.handleKey(msg)
	}
	return nil
}

func (c *pluginConfig) handleKey(msg tea.KeyMsg) tea.Cmd {
	if c.editing {
		return c.handleEditKey(msg)
	}
	if handled, cmd := c.list.handleKey(msg); handled {
		return tea.Batch(cmd, c.flush())
	}
	if msg.String() == "r" {
		return tea.Batch(c.flush(), loadPluginConfig(c.plugin))
	}
	f, e := c.current()
	if f == nil {
		return nil
	}
	if msg.String() == "E" {
		return tea.Batch(c.flush(), c.editFile(f))
	}
	if e == nil {
		if msg.String() == "enter" {
			return c.editFile(f)
		}
		return nil
	}
	if e.ReadOnly {
		return setStatus(e.Key.Key + " is a list of tables; press E to edit the file")
	}
	switch msg.String() {
	case "left", "right":
		return c.cycle(f, e, msg.String())
	case "enter", " ":
		if e.Key.Type == schema.TypeBoolean {
			current, _ := effective(asRow(e)).(bool)
			return c.save(f, e.Key.Key, !current)
		}
		if len(options(e.Key)) > 0 && !e.Key.Open() {
			return c.cycle(f, e, "right")
		}
		text := ""
		if e.Set {
			text = editText(e.Key, e.Value)
		} else if def, ok := e.Key.DefaultValue(); ok {
			text = editText(e.Key, def)
		}
		c.input = newInput(e.Key.Key+" = ", text)
		c.editing = true
		return textinput.Blink
	case "x":
		if !e.Set {
			return setStatus(e.Key.Key + " already uses the default")
		}
		key := e.Key.Key
		return c.run(f, key+" default", func() error { return f.Unset(key) })
	}
	return nil
}

func (c *pluginConfig) handleEditKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "esc":
		c.editing = false
		return nil
	case "enter":
		f, e := c.current()
		if e == nil {
			c.editing = false
			return nil
		}
		value, err := parseValue(e.Key, c.input.Value())
		if err != nil {
			return setError(err)
		}
		c.editing = false
		return c.save(f, e.Key.Key, value)
	}
	var cmd tea.Cmd
	c.input, cmd = c.input.Update(msg)
	return cmd
}

func (c *pluginConfig) save(f *pluginconfig.File, key string, value any) tea.Cmd {
	return c.run(f, key, func() error { return f.Set(key, value) })
}

func (c *pluginConfig) run(f *pluginconfig.File, what string, write func() error) tea.Cmd {
	id := c.plugin.ID
	return func() tea.Msg { return pluginConfigSavedMsg{id: id, what: what, err: write()} }
}

// cycle moves e to the previous or next listed value, saving after a pause.
func (c *pluginConfig) cycle(f *pluginconfig.File, e *pluginconfig.Entry, dir string) tea.Cmd {
	var opts []option
	for _, o := range options(e.Key) {
		if !o.custom {
			opts = append(opts, o)
		}
	}
	if len(opts) == 0 {
		return nil
	}
	same := c.pending != nil && c.pending.key == e.Key.Key && c.pendingFile == f.Name
	value := effective(asRow(e))
	if same {
		value = c.pending.value
	}
	i := selectedOption(opts, value)
	switch {
	case i < 0 && dir == "left":
		i = len(opts) - 1
	case i < 0:
		i = 0
	case dir == "left":
		i = (i + len(opts) - 1) % len(opts)
	default:
		i = (i + 1) % len(opts)
	}
	var flush tea.Cmd
	if c.pending != nil && !same {
		flush = c.flush()
	}
	seq := 1
	if c.pending != nil {
		seq = c.pending.seq + 1
	}
	c.pending, c.pendingFile = &pendingValue{key: e.Key.Key, value: opts[i].value, seq: seq}, f.Name
	return tea.Batch(flush, tea.Tick(pickDelay, func(time.Time) tea.Msg { return pluginConfigPickMsg{seq: seq} }))
}

// flush saves the pending value now.
func (c *pluginConfig) flush() tea.Cmd {
	pending := c.pending
	if pending == nil || c.cfg == nil {
		return nil
	}
	c.pending = nil
	for _, f := range c.cfg.Files {
		if f.Name == c.pendingFile {
			return c.save(f, pending.key, pending.value)
		}
	}
	return nil
}

// editFile opens a config file in the user's editor; the plugin validates
// it, not Herdr, so it is edited in place.
func (c *pluginConfig) editFile(f *pluginconfig.File) tea.Cmd {
	cmd, err := theme.EditorCommand(f.Path)
	if err != nil {
		return setError(err)
	}
	id, name := c.plugin.ID, f.Name
	return tea.ExecProcess(cmd, func(err error) tea.Msg {
		if err != nil {
			return pluginConfigSavedMsg{id: id, what: name, err: fmt.Errorf("editor: %w", err)}
		}
		return pluginConfigReloadMsg{id: id}
	})
}

func (c *pluginConfig) view(width, height int, lines []string) string {
	heading := "Settings · " + c.plugin.Name
	if c.cfg != nil {
		heading += styleDim.Render(" · " + c.cfg.Dir)
	}
	lines = append(lines, styleHeading.Render(heading))
	switch {
	case c.err != nil:
		return strings.Join(append(lines, styleError.Render(c.err.Error()), styleDim.Render("r retries")), "\n")
	case c.cfg == nil:
		return strings.Join(append(lines, styleDim.Render("loading…")), "\n")
	}
	if c.cfg.SchemaErr != nil {
		lines = append(lines, styleWarn.Render(pluginconfig.SchemaFile+": "+c.cfg.SchemaErr.Error()))
	}
	if len(c.cfg.Files) == 0 {
		return strings.Join(append(lines,
			c.plugin.Name+" has no config files yet.",
			styleDim.Render("Plugins create them on first run, or ship "+pluginconfig.SchemaFile+" to describe their settings here."),
		), "\n")
	}
	detail := c.detail(width)
	listHeight := max(height-len(lines)-len(detail)-1, 3)
	keyWidth := min(max(width/3, 24), 40)
	body := c.list.view(listHeight, func(i int, selected bool) string {
		r := c.rows[i]
		f := c.cfg.Files[r.file]
		if r.entry < 0 {
			return styleHeading.Render(f.Name) + styleDim.Render(fileNote(f))
		}
		e := f.Entries[r.entry]
		name := padRight(e.Key.Key, keyWidth)
		if opts := options(e.Key); len(opts) > 0 && !e.ReadOnly {
			var pending *pendingValue
			if c.pendingFile == f.Name {
				pending = c.pending
			}
			return row(name+" "+strip(asRow(e), opts, pending), selected)
		}
		value := styleDim.Render(firstNonEmpty(e.Key.Default, "unset"))
		if e.Set {
			value = styleAccent.Render(formatValue(e.Value)) + " *"
		}
		return row(name+" "+value, selected)
	})
	lines = append(lines, fit(body, width, listHeight), "")
	return strings.Join(append(lines, detail...), "\n")
}

func fileNote(f *pluginconfig.File) string {
	switch {
	case f.Missing:
		return "  not created yet; the first change creates it"
	case f.Format == pluginconfig.Other:
		return "  ↵ opens it in your editor"
	case len(f.Entries) == 0:
		return "  no settings found; E opens it"
	}
	return ""
}

func (c *pluginConfig) detail(width int) []string {
	if c.editing {
		return []string{c.input.View()}
	}
	_, e := c.current()
	if e == nil {
		return nil
	}
	lines := wrap(e.Key.Description, width)
	about := e.Key.Type
	if len(e.Key.Values) > 0 {
		about += ": " + strings.Join(e.Key.Values, ", ")
	}
	if e.Key.Default != "" {
		about += " · default " + e.Key.Default
	}
	if !e.Documented {
		about += " · type inferred from the file"
	}
	return append(lines, styleDim.Render(about))
}
