package ui

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/wapenshaw/herdr-customize/tests/testutil"
)

const pluginListJSON = `{"result":{"plugins":[{"plugin_id":"demo","name":"Demo","enabled":true,` +
	`"source":{"kind":"github","owner":"me","repo":"demo"},` +
	`"actions":[{"id":"run","title":"Run demo"},{"id":"bound","title":"Already bound"}]}]}}`

// keysDriver opens the Keys page over config with a fake herdr that lists
// the demo plugin.
func keysDriver(t *testing.T, config string) (*driver, *keysPage, string) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "config.toml")
	if err := os.WriteFile(path, []byte(config), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("HERDR_CONFIG_PATH", path)
	t.Setenv("HERDR_PLUGIN_STATE_DIR", t.TempDir())
	testutil.FakeHerdr(t, testutil.HerdrResponse{Args: "plugin list --json", Stdout: pluginListJSON})
	d := newDriver(t, Options{Tab: "keys"})
	return d, d.app.pages[2].(*keysPage), path
}

func (p *keysPage) find(t *testing.T, match func(keyRow) bool) *keyRow {
	t.Helper()
	for i := range p.rows {
		if match(p.rows[i]) {
			return &p.rows[i]
		}
	}
	t.Fatal("row not found")
	return nil
}

func readConfig(t *testing.T, path string) string {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}

func TestKeysEditAndResetBuiltin(t *testing.T) {
	d, page, path := keysDriver(t, "[keys]\nprefix = \"ctrl+a\"\n")
	d.keys("/", "zoom", "enter")
	if r := page.current(); r == nil || r.action != "zoom" {
		t.Fatalf("selected %+v", page.current())
	}
	d.keys("enter", "ctrl+u", "prefix+shift+z", "enter")
	if got := readConfig(t, path); !strings.Contains(got, "zoom = \"prefix+shift+z\"") {
		t.Fatalf("binding not saved:\n%s", got)
	}
	if r := page.current(); r == nil || !r.changed {
		t.Fatal("row not marked changed after save")
	}
	d.keys("x")
	if got := readConfig(t, path); strings.Contains(got, "zoom") || !strings.Contains(got, "ctrl+a") {
		t.Fatalf("reset did not remove only zoom:\n%s", got)
	}
}

func TestKeysFlagsConflictsAndMissingActions(t *testing.T) {
	_, page, _ := keysDriver(t, `[[keys.command]]
key = "prefix+p"
type = "plugin_action"
command = "herdr-theme-picker.open"

[[keys.command]]
key = "prefix+t"
type = "plugin_action"
command = "demo.bound"
`)
	stale := page.find(t, func(r keyRow) bool { return r.kind == rowCommand && r.index == 0 })
	if !stale.missing {
		t.Fatal("uninstalled plugin action not flagged")
	}
	if len(stale.clashes) != 1 || stale.clashes[0] != "keys.previous_tab" {
		t.Fatalf("clashes = %v", stale.clashes)
	}
	if page.find(t, func(r keyRow) bool { return r.kind == rowCommand && r.index == 1 }).missing {
		t.Fatal("installed action flagged as missing")
	}
	for _, r := range page.rows {
		if r.kind == rowPluginAction && r.qualified == "demo.bound" {
			t.Fatal("bound action listed as unbound")
		}
	}
	page.find(t, func(r keyRow) bool { return r.kind == rowPluginAction && r.qualified == "demo.run" })
}

func TestKeysBindPluginAction(t *testing.T) {
	d, page, path := keysDriver(t, "theme = \"x\"\n")
	page.list.selectRow(slicesIndex(page.rows, func(r keyRow) bool { return r.qualified == "demo.run" }))
	d.keys("enter")
	if page.mode != keysForm || page.form.value("command") != "demo.run" {
		t.Fatalf("form not prefilled, mode %d", page.mode)
	}
	d.keys("prefix+alt+d", "enter")
	got := readConfig(t, path)
	for _, want := range []string{"[[keys.command]]", `key = "prefix+alt+d"`, `type = "plugin_action"`, `command = "demo.run"`, `description = "Run demo"`} {
		if !strings.Contains(got, want) {
			t.Fatalf("missing %q in:\n%s", want, got)
		}
	}
	for _, r := range page.rows {
		if r.kind == rowPluginAction && r.qualified == "demo.run" {
			t.Fatal("action still listed as unbound")
		}
	}
}

func TestKeysFormRejectsPlainKey(t *testing.T) {
	d, page, path := keysDriver(t, "")
	d.keys("n", "x", "tab", "echo hi", "enter")
	if page.mode != keysForm || !d.app.statusErr {
		t.Fatal("plain key was accepted")
	}
	if got := readConfig(t, path); got != "" {
		t.Fatalf("config written:\n%s", got)
	}
}

func TestKeysDeleteCommand(t *testing.T) {
	d, page, path := keysDriver(t, "[[keys.command]]\nkey = \"prefix+t\"\ntype = \"shell\"\ncommand = \"true\"\n")
	page.list.selectRow(slicesIndex(page.rows, func(r keyRow) bool { return r.kind == rowCommand }))
	d.keys("d", "y")
	if got := readConfig(t, path); strings.Contains(got, "keys.command") {
		t.Fatalf("command not deleted:\n%s", got)
	}
}

func TestRecordKey(t *testing.T) {
	for _, tc := range []struct {
		msg  tea.KeyMsg
		want string
	}{
		{tea.KeyMsg{Type: tea.KeyCtrlT}, "ctrl+t"},
		{tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("x")}, "prefix+x"},
		{tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("x"), Alt: true}, "alt+x"},
		{tea.KeyMsg{Type: tea.KeyPgUp}, "pageup"},
		{tea.KeyMsg{Type: tea.KeyF5}, "f5"},
	} {
		if got, ok := recordKey(tc.msg); !ok || got != tc.want {
			t.Errorf("recordKey(%s) = %q, %v; want %q", tc.msg, got, ok, tc.want)
		}
	}
}

func slicesIndex(rows []keyRow, match func(keyRow) bool) int {
	for i, r := range rows {
		if match(r) {
			return i
		}
	}
	return -1
}
