package ui

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/wapenshaw/herdr-customize/tests/testutil"
)

func settingsDriver(t *testing.T, config string, responses ...testutil.HerdrResponse) (*driver, *settingsPage, string) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "config.toml")
	if err := os.WriteFile(path, []byte(config), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("HERDR_CONFIG_PATH", path)
	t.Setenv("HERDR_PLUGIN_STATE_DIR", t.TempDir())
	testutil.FakeHerdr(t, responses...)
	old := pickDelay
	pickDelay = time.Millisecond
	t.Cleanup(func() { pickDelay = old })
	d := newDriver(t, Options{Tab: "settings"})
	return d, d.app.pages[3].(*settingsPage), path
}

func (p *settingsPage) selectKey(t *testing.T, name string) {
	t.Helper()
	for i, r := range p.rows {
		if r.header == "" && r.name() == name {
			p.list.selectRow(i)
			return
		}
	}
	t.Fatalf("no row %s", name)
}

func TestSettingsToggleUsesDefault(t *testing.T) {
	d, page, path := settingsDriver(t, "")
	page.selectKey(t, "terminal.kitty_graphics") // defaults to true
	d.keys("enter")
	if got := readConfig(t, path); !strings.Contains(got, "kitty_graphics = false") {
		t.Fatalf("toggle did not flip the default:\n%s", got)
	}
	d.keys("enter")
	if got := readConfig(t, path); !strings.Contains(got, "kitty_graphics = true") {
		t.Fatalf("second toggle:\n%s", got)
	}
	d.keys("x")
	if got := readConfig(t, path); strings.Contains(got, "kitty_graphics") {
		t.Fatalf("reset left the key:\n%s", got)
	}
}

func TestSettingsEnterCyclesEnums(t *testing.T) {
	d, page, path := settingsDriver(t, "[terminal]\nnew_cwd = \"home\"\n")
	page.selectKey(t, "terminal.new_cwd")
	if !strings.Contains(d.view(), "[home]") || !strings.Contains(d.view(), "follow") {
		t.Fatalf("values not shown on the row:\n%s", d.view())
	}
	d.keys("enter")
	if got := readConfig(t, path); got != "[terminal]\nnew_cwd = \"current\"\n" {
		t.Fatalf("got:\n%s", got)
	}
	// Space steps too, and the last value wraps to the first.
	d.keys(" ", " ")
	if got := readConfig(t, path); got != "[terminal]\nnew_cwd = \"follow\"\n" {
		t.Fatalf("got:\n%s", got)
	}
	// A default value steps from where it is.
	page.selectKey(t, "ui.tab_bar_position")
	d.keys("enter")
	if got := readConfig(t, path); !strings.Contains(got, "tab_bar_position = \"bottom\"") {
		t.Fatalf("got:\n%s", got)
	}
}

func TestSettingsArrowsSwitchPages(t *testing.T) {
	d, page, path := settingsDriver(t, "")
	for _, key := range []string{"terminal.new_cwd", "terminal.kitty_graphics", "ui.sidebar_width"} {
		page.selectKey(t, key)
		d.keys("right")
		if d.app.active != 0 {
			t.Fatalf("right on %s stayed on page %d", key, d.app.active)
		}
		d.keys("left")
		if d.app.active != 3 {
			t.Fatalf("left did not come back to Settings")
		}
	}
	if got := readConfig(t, path); got != "" {
		t.Fatalf("arrows changed the config:\n%s", got)
	}
}

func TestSettingsCyclingSavesOnce(t *testing.T) {
	_, page, path := settingsDriver(t, "")
	page.selectKey(t, "ui.host_cursor") // auto, native, drawn; default auto
	r := page.current()
	page.cycle(r)
	page.cycle(r)
	if page.pending == nil || page.pending.value != "drawn" {
		t.Fatalf("pending = %+v", page.pending)
	}
	if cmd := page.update(pendingSaveMsg{seq: 1}); cmd != nil {
		t.Fatal("an earlier press saved")
	}
	save := page.update(pendingSaveMsg{seq: page.pending.seq})
	// Until the save reports back, even a reload from disk shows the new
	// value rather than flashing the old one.
	page.update(savedMsg{what: "another key"})
	if r := page.current(); r.value != "drawn" || !r.set {
		t.Fatalf("row shows %v while saving", r.value)
	}
	saved := save()
	if msg := saved.(settingSavedMsg); msg.saved.err != nil {
		t.Fatal(msg.saved.err)
	}
	page.update(saved)
	if len(page.assumed) != 0 {
		t.Fatalf("assumed values left: %v", page.assumed)
	}
	if got := readConfig(t, path); got != "[ui]\nhost_cursor = \"drawn\"\n" {
		t.Fatalf("got:\n%s", got)
	}
}

func TestSettingsOpenChoicesAcceptOtherValues(t *testing.T) {
	d, page, path := settingsDriver(t, "")
	page.selectKey(t, "ui.right_click_passthrough_modifier")
	if !strings.Contains(d.view(), "other…") {
		t.Fatal("open choice has no way to type a value")
	}
	d.keys(" ", " ")
	if got := readConfig(t, path); !strings.Contains(got, `right_click_passthrough_modifier = "alt"`) {
		t.Fatalf("got:\n%s", got)
	}
	d.keys("enter", "ctrl+u", "ctrl+alt", "enter")
	if got := readConfig(t, path); !strings.Contains(got, `right_click_passthrough_modifier = "ctrl+alt"`) {
		t.Fatalf("got:\n%s", got)
	}
	if !strings.Contains(d.view(), "[ctrl+alt]") {
		t.Fatalf("custom value not shown:\n%s", d.view())
	}
}

func TestSettingsLeavesThemesToThemesPage(t *testing.T) {
	_, page, _ := settingsDriver(t, "[theme]\nname = \"nord\"\n")
	for _, r := range page.rows {
		if r.header == "Theme" || strings.HasPrefix(r.name(), "theme.") && r.header == "" {
			t.Fatalf("theme row %q on the Settings page", r.name())
		}
	}
}

func TestSettingsTextFields(t *testing.T) {
	d, page, path := settingsDriver(t, "# keep me\n[ui]\nsidebar_width = 30\n")
	page.selectKey(t, "ui.sidebar_width")
	d.keys("enter", "ctrl+u", "wide", "enter")
	if !d.app.statusErr || page.mode != settingsEditing {
		t.Fatal("non-number accepted for an integer")
	}
	d.keys("ctrl+u", "32", "enter")
	if got := readConfig(t, path); got != "# keep me\n[ui]\nsidebar_width = 32\n" {
		t.Fatalf("got:\n%s", got)
	}

	page.selectKey(t, "experimental.cjk_ime_agents")
	d.keys("enter", "ctrl+u", `["codex", "claude"]`, "enter")
	if got := readConfig(t, path); !strings.Contains(got, "[experimental]\ncjk_ime_agents = [\"codex\", \"claude\"]") {
		t.Fatalf("list not saved:\n%s", got)
	}
}

func TestSettingsRejectedSaveKeepsFile(t *testing.T) {
	original := "[ui]\nsidebar_width = 30\n"
	d, page, path := settingsDriver(t, original, testutil.HerdrResponse{
		Args: "config check", ConfigContains: "sidebar_width = 0", Exit: 1,
		Stdout: "config: issues found\nui.sidebar_width must be greater than zero\n",
	})
	page.selectKey(t, "ui.sidebar_width")
	d.keys("enter", "ctrl+u", "0", "enter")
	if got := readConfig(t, path); got != original || !d.app.statusErr {
		t.Fatalf("rejected value reached disk:\n%s", got)
	}
	if !strings.Contains(d.app.status, "greater than zero") {
		t.Fatalf("status %q", d.app.status)
	}
}

func TestSettingsUnknownKeys(t *testing.T) {
	d, page, path := settingsDriver(t, "[ui]\nsidebar_width = 30\ntypo = 1\n\n[plugins.thing]\nx = 1\n")
	var unknown []string
	for _, r := range page.rows {
		if r.unknown != nil {
			unknown = append(unknown, r.name())
		}
	}
	if strings.Join(unknown, ",") != "plugins,ui.typo" {
		t.Fatalf("unknown = %v", unknown)
	}
	page.selectKey(t, "ui.typo")
	d.keys("x", "y")
	if got := readConfig(t, path); strings.Contains(got, "typo") || !strings.Contains(got, "plugins.thing") {
		t.Fatalf("got:\n%s", got)
	}
}

func TestSettingsRawEditResumesAfterRejection(t *testing.T) {
	d, page, path := settingsDriver(t, "[ui]\nsidebar_width = 30\n")
	scratch := filepath.Join(t.TempDir(), "edit.toml")
	os.WriteFile(scratch, []byte("[ui]\nsidebar_width = \n"), 0o600)
	d.send(rawEditedMsg{path: scratch})
	if page.raw == nil || !d.app.statusErr {
		t.Fatal("broken edit was not kept")
	}
	os.WriteFile(scratch, []byte("[ui]\nsidebar_width = 40\n"), 0o600)
	d.send(rawEditedMsg{path: scratch})
	if got := readConfig(t, path); got != "[ui]\nsidebar_width = 40\n" || page.raw != nil {
		t.Fatalf("fixed edit not saved:\n%s", got)
	}
	if _, err := os.Stat(scratch); !os.IsNotExist(err) {
		t.Fatal("scratch file left behind")
	}
}
