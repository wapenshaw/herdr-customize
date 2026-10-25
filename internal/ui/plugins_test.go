package ui

import (
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/wapenshaw/herdr-customize/internal/plugins"
	"github.com/wapenshaw/herdr-customize/tests/testutil"
)

const managedPluginsJSON = `{"result":{"plugins":[
{"plugin_id":"demo","name":"Demo","version":"1.0.0","enabled":true,"source":{"kind":"github","owner":"me","repo":"demo","resolved_commit":"aaaaaaaaaa"},
 "actions":[{"id":"run","title":"Run demo"}]},
{"plugin_id":"pinned","name":"Pinned","version":"0.1.0","enabled":true,"source":{"kind":"github","owner":"me","repo":"pinned","requested_ref":"bbbbbbb","resolved_commit":"bbbbbbbbbb"}},
{"plugin_id":"local","name":"Local","enabled":false,"source":{"kind":"local"}}]}}`

func pluginsDriver(t *testing.T) (*driver, *pluginsPage, string) {
	t.Helper()
	t.Setenv("HERDR_CONFIG_PATH", filepath.Join(t.TempDir(), "config.toml"))
	t.Setenv("HERDR_PLUGIN_STATE_DIR", t.TempDir())
	log := testutil.FakeHerdr(t, testutil.HerdrResponse{Args: "plugin list --json", Stdout: managedPluginsJSON})
	d := newDriver(t, Options{Tab: "plugins"})
	page := d.app.pages[1].(*pluginsPage)
	if len(page.plugins) != 3 {
		t.Fatalf("plugins = %d", len(page.plugins))
	}
	return d, page, log
}

func TestPluginsExpandRunAndBind(t *testing.T) {
	d, page, _ := pluginsDriver(t)
	if !strings.Contains(d.view(), "Demo") || !strings.Contains(d.view(), "(disabled)") {
		t.Fatalf("view:\n%s", d.view())
	}
	d.keys("enter")
	if len(page.rows) != 4 {
		t.Fatalf("actions not expanded: %d rows", len(page.rows))
	}
	d.keys("j", "enter")
	if d.app.after == nil {
		t.Fatal("running an action did not close the popup first")
	}
	d.app.after = nil
	d.keys("b")
	keys := d.app.pages[2].(*keysPage)
	if d.app.active != 2 || keys.mode != keysForm || keys.form.value("command") != "demo.run" {
		t.Fatalf("bind did not open the Keys form (page %d)", d.app.active)
	}
}

func TestPluginsToggleAndRemove(t *testing.T) {
	d, page, log := pluginsDriver(t)
	d.keys("e")
	page.list.selectRow(slices.IndexFunc(page.rows, func(r pluginRow) bool { return page.plugins[r.plugin].ID == "local" }))
	d.keys("e", "x")
	if page.mode != pluginsConfirm || !strings.HasPrefix(page.ask.question, "unlink Local") {
		t.Fatalf("remove did not confirm an unlink: %+v", page.ask)
	}
	d.keys("y")
	calls := strings.Join(testutil.HerdrCalls(t, log), "\n")
	for _, want := range []string{"plugin disable demo", "plugin enable local", "plugin unlink local"} {
		if !strings.Contains(calls, want) {
			t.Errorf("missing %q in:\n%s", want, calls)
		}
	}
}

func TestPluginsUpdateAllSkipsPins(t *testing.T) {
	d, page, _ := pluginsDriver(t)
	d.send(checksMsg{statuses: map[string]plugins.Status{
		"demo":   {State: plugins.Behind, Version: "1.1.0"},
		"pinned": {State: plugins.Behind},
	}})
	if !strings.Contains(d.view(), "1.0.0 → 1.1.0") || !strings.Contains(d.view(), "2 update(s)") {
		t.Fatalf("view:\n%s", d.view())
	}
	d.keys("U")
	if page.mode != pluginsConfirm || page.ask.question != "Update 1 plugin(s): Demo → 1.1.0?" {
		t.Fatalf("question = %+v", page.ask)
	}
	d.keys("n")
	d.send(checksMsg{statuses: map[string]plugins.Status{"demo": {State: plugins.Current}}})
	d.keys("u")
	if !strings.Contains(d.app.status, "up to date") {
		t.Fatalf("status %q", d.app.status)
	}
}

func TestInstallArgs(t *testing.T) {
	for in, want := range map[string]string{
		"a/b":                     "plugin install a/b",
		"https://github.com/a/b":  "plugin install a/b",
		"a/b/plugins/x@v1.2":      "plugin install a/b/plugins/x --ref v1.2",
		" a/b@0123456789abcdef  ": "plugin install a/b --ref 0123456789abcdef",
		"a":                       "",
		"a/b; rm -rf /":           "",
		"--ref=x/y":               "",
	} {
		args, err := installArgs(in)
		if got := strings.Join(args, " "); got != want || (want == "") != (err != nil) {
			t.Errorf("installArgs(%q) = %q, %v; want %q", in, got, err, want)
		}
	}
}

func TestPluginsRegistryMissing(t *testing.T) {
	d, _, _ := pluginsDriver(t)
	if filepath.Dir(plugins.RegistryPath()) != filepath.Dir(os.Getenv("HERDR_CONFIG_PATH")) {
		t.Fatalf("registry %s is outside the test config dir", plugins.RegistryPath())
	}
	d.keys("c")
	if !d.app.statusErr || !strings.Contains(d.app.status, "plugins.json") {
		t.Fatalf("status %q", d.app.status)
	}
}

func TestPluginSettings(t *testing.T) {
	t.Setenv("HERDR_CONFIG_PATH", filepath.Join(t.TempDir(), "config.toml"))
	t.Setenv("HERDR_PLUGIN_STATE_DIR", t.TempDir())
	dir := t.TempDir()
	env := filepath.Join(dir, "config.env")
	os.WriteFile(env, []byte("# Where the panel opens.\n#DEMO_MODE=split\n# Debug logging.\n#DEMO_DEBUG=false\n#DEMO_POLL=500\n"), 0o644)
	root := t.TempDir()
	os.WriteFile(filepath.Join(root, "config-schema.json"), []byte(`{"file": "config.env", "sections": [{"id": "a", "keys": [
		{"key": "DEMO_MODE", "type": "enum", "default": "\"split\"", "values": ["split", "popup", "tab"]}]}]}`), 0o644)
	list := strings.Replace(managedPluginsJSON, `"plugin_id":"demo",`, `"plugin_id":"demo","plugin_root":`+strconv.Quote(root)+`,`, 1)
	testutil.FakeHerdr(t,
		testutil.HerdrResponse{Args: "plugin list --json", Stdout: list},
		testutil.HerdrResponse{Args: "plugin config-dir demo", Stdout: dir + "\n"},
	)
	old := pickDelay
	pickDelay = time.Millisecond
	t.Cleanup(func() { pickDelay = old })
	d := newDriver(t, Options{Tab: "plugins"})
	page := d.app.pages[1].(*pluginsPage)
	d.keys("s")
	if page.screen != viewConfig || page.config.cfg == nil {
		t.Fatalf("settings did not load: %v", page.config.err)
	}
	view := d.view()
	for _, want := range []string{"Settings · Demo", "DEMO_MODE", "[split]", "popup", "DEMO_DEBUG", "[off]", "DEMO_POLL", "500"} {
		if !strings.Contains(view, want) {
			t.Fatalf("missing %q in:\n%s", want, view)
		}
	}
	// Arrows switch pages even on a setting with values.
	d.keys("right")
	if d.app.active != 2 {
		t.Fatal("right did not switch pages")
	}
	// The first row is the file heading; the cursor starts on a setting.
	d.keys("left", "enter", "enter", "j", "enter", "j", "enter", "ctrl+u", "250", "enter")
	want := "# Where the panel opens.\n#DEMO_MODE=split\nDEMO_MODE=tab\n# Debug logging.\n#DEMO_DEBUG=false\nDEMO_DEBUG=true\n#DEMO_POLL=500\nDEMO_POLL=250\n"
	if got := readConfig(t, env); got != want {
		t.Fatalf("got:\n%s", got)
	}
	d.keys("enter", "ctrl+u", "soon", "enter")
	if !d.app.statusErr || !page.config.editing {
		t.Fatal("text accepted for a number")
	}
	d.keys("esc", "x")
	if got := readConfig(t, env); strings.Contains(got, "DEMO_POLL=250") {
		t.Fatalf("reset left the value:\n%s", got)
	}
	d.keys("esc")
	if page.screen != viewInstalled {
		t.Fatal("esc did not go back")
	}
}
