package ui

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/charmbracelet/bubbles/cursor"
	tea "github.com/charmbracelet/bubbletea"

	"github.com/wapenshaw/herdr-customize/internal/plugins"
	"github.com/wapenshaw/herdr-customize/internal/theme"
	"github.com/wapenshaw/herdr-customize/tests/testutil"
)

func TestMain(m *testing.M) {
	cursorMode = cursor.CursorStatic
	// Keep update checks and marketplace searches off the network.
	plugins.GitBase = filepath.Join(os.TempDir(), "herdr-customize-no-remote")
	plugins.RawBase, plugins.APIBase = "http://127.0.0.1:1", "http://127.0.0.1:1"
	testutil.Main(m)
}

// driver runs an app without a terminal, executing commands synchronously.
type driver struct {
	t   *testing.T
	app *app
}

func newDriver(t *testing.T, opts Options) *driver {
	t.Helper()
	t.Setenv("HERDR_PLUGIN_ROOT", filepath.Join("..", ".."))
	d := &driver{t: t, app: newApp(opts)}
	d.send(tea.WindowSizeMsg{Width: 120, Height: 30})
	d.run(d.app.Init())
	return d
}

func (d *driver) send(msg tea.Msg) {
	d.t.Helper()
	_, cmd := d.app.Update(msg)
	d.run(cmd)
}

// run executes cmd and feeds its messages back. Commands that take longer
// than the timeout are timers and are dropped.
func (d *driver) run(cmd tea.Cmd) {
	d.t.Helper()
	if cmd == nil {
		return
	}
	done := make(chan tea.Msg, 1)
	go func() { done <- cmd() }()
	var msg tea.Msg
	select {
	case msg = <-done:
	case <-time.After(5 * time.Second):
		return
	}
	switch m := msg.(type) {
	case nil:
	case tea.BatchMsg:
		for _, c := range m {
			d.run(c)
		}
	case tea.QuitMsg:
	default:
		// tea.Sequence produces an unexported slice of commands.
		if v := reflect.ValueOf(msg); v.Kind() == reflect.Slice && v.Type().Elem() == reflect.TypeOf(tea.Cmd(nil)) {
			for i := 0; i < v.Len(); i++ {
				d.run(v.Index(i).Interface().(tea.Cmd))
			}
			return
		}
		if strings.Contains(reflect.TypeOf(msg).String(), "blink") || strings.Contains(reflect.TypeOf(msg).String(), "Blink") {
			return
		}
		d.send(msg)
	}
}

func (d *driver) keys(keys ...string) {
	d.t.Helper()
	for _, k := range keys {
		switch k {
		case "enter":
			d.send(tea.KeyMsg{Type: tea.KeyEnter})
		case "esc":
			d.send(tea.KeyMsg{Type: tea.KeyEsc})
		case "tab":
			d.send(tea.KeyMsg{Type: tea.KeyTab})
		case "shift+tab":
			d.send(tea.KeyMsg{Type: tea.KeyShiftTab})
		case "up":
			d.send(tea.KeyMsg{Type: tea.KeyUp})
		case "down":
			d.send(tea.KeyMsg{Type: tea.KeyDown})
		case "left":
			d.send(tea.KeyMsg{Type: tea.KeyLeft})
		case "right":
			d.send(tea.KeyMsg{Type: tea.KeyRight})
		case "ctrl+u":
			d.send(tea.KeyMsg{Type: tea.KeyCtrlU})
		default:
			d.send(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(k)})
		}
	}
}

func (d *driver) view() string { return d.app.View() }

func TestRunesThatArriveTogetherAreSplit(t *testing.T) {
	d := newDriver(t, Options{})
	page := d.app.pages[0].(*themesPage)
	start := page.list.cursor
	d.keys("jj")
	if page.list.cursor != start+2 {
		t.Fatalf("cursor = %d, want %d", page.list.cursor, start+2)
	}
	d.keys("/nord")
	if !strings.Contains(d.view(), "/nord") || page.list.filter.Value() != "nord" {
		t.Fatalf("filter not typed:\n%s", d.view())
	}
}

func TestEscClearsSearchBeforeQuitting(t *testing.T) {
	d := newDriver(t, Options{})
	d.keys("/nord", "enter", "esc")
	page := d.app.pages[0].(*themesPage)
	if page.list.filter.Value() != "" {
		t.Fatal("esc did not clear the search")
	}
	_, cmd := d.app.Update(tea.KeyMsg{Type: tea.KeyEsc})
	if cmd == nil || cmd() != tea.Quit() {
		t.Fatal("second esc did not quit")
	}
}

func TestPageSwitching(t *testing.T) {
	d := newDriver(t, Options{Tab: "keys"})
	if d.app.active != 2 {
		t.Fatalf("started on page %d", d.app.active)
	}
	d.keys("tab")
	if d.app.active != 3 {
		t.Fatalf("tab moved to %d", d.app.active)
	}
	d.keys("1")
	if d.app.active != 0 {
		t.Fatalf("1 moved to %d", d.app.active)
	}
}

func TestApplyClosesThenApplies(t *testing.T) {
	d := newDriver(t, Options{})
	d.keys("/dracula", "enter", "enter")
	if d.app.after == nil {
		t.Fatal("enter did not schedule an apply")
	}
	t.Setenv("HERDR_CONFIG_PATH", filepath.Join(t.TempDir(), "config.toml"))
	t.Setenv("HERDR_PLUGIN_STATE_DIR", t.TempDir())
	quiet(t)
	if err := d.app.after(); err != nil {
		t.Fatal(err)
	}
	if applied, _ := theme.AppliedSlug(); applied != "dracula-default" {
		t.Fatalf("applied %q", applied)
	}
}

func TestNewThemeIsNamedSavedAndApplied(t *testing.T) {
	t.Setenv("HERDR_PLUGIN_STATE_DIR", t.TempDir())
	d := newDriver(t, Options{})
	page := d.app.pages[0].(*themesPage)
	body, err := os.ReadFile(filepath.Join("..", "..", "themes", "nord-default"))
	if err != nil {
		t.Fatal(err)
	}
	d.run(page.askName(string(body)))
	if !page.capturing() {
		t.Fatal("name prompt does not capture keys")
	}
	d.keys("My Theme", "enter")
	if !theme.IsUserTheme("my-theme") || d.app.after == nil {
		t.Fatal("theme not saved or not scheduled to apply")
	}

	// Saving the same name again asks before overwriting.
	d.app.after = nil
	d.run(page.askName(string(body)))
	d.keys("my theme", "enter")
	if page.mode != themesConfirm {
		t.Fatalf("overwrite was not confirmed, mode %d", page.mode)
	}
	d.keys("n")
	if d.app.after != nil {
		t.Fatal("declined overwrite still applied")
	}
}

func TestInvalidPaletteIsRejected(t *testing.T) {
	d := newDriver(t, Options{})
	page := d.app.pages[0].(*themesPage)
	d.run(page.askName("background = #000000\n"))
	if page.mode != themesBrowse || !d.app.statusErr {
		t.Fatal("invalid palette reached the name prompt")
	}
}

func TestTokenEditorSavesOverride(t *testing.T) {
	t.Setenv("HERDR_PLUGIN_STATE_DIR", t.TempDir())
	body, err := os.ReadFile(filepath.Join("..", "..", "themes", "nord-default"))
	if err != nil {
		t.Fatal(err)
	}
	t.Setenv("HERDR_PLUGIN_ROOT", filepath.Join("..", ".."))
	if _, err := theme.SaveUserTheme("mine", string(body), false); err != nil {
		t.Fatal(err)
	}
	d := newDriver(t, Options{EditTheme: "mine"})
	page := d.app.pages[0].(*themesPage)
	if page.mode != themesEditing {
		t.Fatalf("editor not open, mode %d", page.mode)
	}
	for page.editRows[page.editCur].Token != "accent" {
		d.keys("j")
	}
	d.keys("enter", "ctrl+u", "#ff0000", "enter")
	if !strings.Contains(page.editBody, "hpick-override: accent=#ff0000") {
		t.Fatal("override not applied to the scratch body")
	}
	saved, _ := theme.UserThemeBody("mine")
	if strings.Contains(saved, "#ff0000") {
		t.Fatal("unsaved edit reached disk")
	}
	d.keys("s")
	saved, _ = theme.UserThemeBody("mine")
	if !strings.Contains(saved, "hpick-override: accent=#ff0000") || d.app.after == nil {
		t.Fatal("save did not commit and apply")
	}
}

func quiet(t *testing.T) {
	t.Helper()
	null, err := os.Open(os.DevNull)
	if err != nil {
		t.Fatal(err)
	}
	stdout := os.Stdout
	os.Stdout = null
	t.Cleanup(func() { os.Stdout = stdout; null.Close() })
}

func TestThemeFiltersAndGroups(t *testing.T) {
	d := newDriver(t, Options{})
	page := d.app.pages[0].(*themesPage)
	if page.rows[0].header == "" || !strings.Contains(d.view(), "DARK") || !strings.Contains(d.view(), "Light") {
		t.Fatalf("groups not shown:\n%s", d.view())
	}
	if _, ok := page.selectedItem(); !ok {
		t.Fatal("cursor starts on a heading")
	}
	for _, want := range []string{"Dark", "Light"} {
		d.keys("right")
		if themeFilters[page.filter] != want {
			t.Fatalf("filter %s, want %s", themeFilters[page.filter], want)
		}
		if d.app.active != 0 {
			t.Fatal("right left the page")
		}
		for _, r := range page.rows {
			if r.header != "" {
				t.Fatal("a filtered view has headings")
			}
			if dark := page.looks[page.items[r.item].Slug].Dark; dark != (want == "Dark") {
				t.Fatalf("%s in %s", page.items[r.item].Slug, want)
			}
		}
	}
	d.keys("left", "left", "left")
	if themeFilters[page.filter] != "Yours" {
		t.Fatalf("left did not wrap: %s", themeFilters[page.filter])
	}
}

func TestKeySheet(t *testing.T) {
	d := newDriver(t, Options{})
	page := d.app.pages[0].(*themesPage)
	start := page.list.cursor
	d.keys("?")
	if !strings.Contains(d.view(), "Keys · Themes") || !strings.Contains(d.view(), "token table") {
		t.Fatalf("sheet not shown:\n%s", d.view())
	}
	d.keys("j")
	if d.app.sheet || page.list.cursor != start {
		t.Fatal("the key that closed the sheet also moved the list")
	}
}
