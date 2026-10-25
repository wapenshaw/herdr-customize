package pluginconfig_test

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/wapenshaw/herdr-customize/internal/pluginconfig"
	"github.com/wapenshaw/herdr-customize/internal/schema"
	"github.com/wapenshaw/herdr-customize/tests/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

func write(t *testing.T, dir, name, text string) string {
	t.Helper()
	path := filepath.Join(dir, name)
	if err := os.WriteFile(path, []byte(text), 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

func read(t *testing.T, path string) string {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}

func load(t *testing.T, dir, root string) *pluginconfig.Config {
	t.Helper()
	c, err := pluginconfig.Load(dir, root)
	if err != nil {
		t.Fatal(err)
	}
	return c
}

func file(t *testing.T, c *pluginconfig.Config, name string) *pluginconfig.File {
	t.Helper()
	for _, f := range c.Files {
		if f.Name == name {
			return f
		}
	}
	t.Fatalf("no file %s", name)
	return nil
}

func entry(t *testing.T, f *pluginconfig.File, key string) *pluginconfig.Entry {
	t.Helper()
	for _, e := range f.Entries {
		if e.Key.Key == key {
			return e
		}
	}
	t.Fatalf("no key %s in %s", key, f.Name)
	return nil
}

func TestTOMLTypesAndCommentedDefaults(t *testing.T) {
	dir := t.TempDir()
	path := write(t, dir, "config.toml", `# Where the panel opens.
placement = "split"
ratio = 0.5
startup = true

[web]
# Port to listen on.
# port = 6363
listen = "127.0.0.1"
colors = { fg = "#fff" }

[[rules]]
name = "a"
`)
	write(t, dir, "hook.cjs", "module.exports = {}\n")
	write(t, dir, "config.toml.bak-20260101", "")
	c := load(t, dir, "")
	if len(c.Files) != 2 || c.Files[0].Name != "config.toml" || c.Files[1].Format != pluginconfig.Other {
		t.Fatalf("files = %+v", c.Files)
	}
	f := c.Files[0]
	var keys []string
	for _, e := range f.Entries {
		keys = append(keys, e.Key.Key+":"+e.Key.Type)
	}
	want := "placement:string ratio:float startup:boolean web.listen:string rules:array web.colors.fg:string web.port:integer"
	if strings.Join(keys, " ") != want {
		t.Fatalf("keys = %s", strings.Join(keys, " "))
	}
	if e := entry(t, f, "placement"); e.Key.Description != "" || e.Value != "split" || !e.Set {
		t.Errorf("placement = %+v", e)
	}
	port := entry(t, f, "web.port")
	if port.Set || port.Key.Default != "6363" || port.Key.Description != "Port to listen on." {
		t.Errorf("port = %+v", port)
	}
	if !entry(t, f, "rules").ReadOnly {
		t.Error("array of tables is editable")
	}

	if err := f.Set("web.port", int64(7000)); err != nil {
		t.Fatal(err)
	}
	if err := f.Set("ratio", 0.75); err != nil {
		t.Fatal(err)
	}
	if err := f.Unset("startup"); err != nil {
		t.Fatal(err)
	}
	got := read(t, path)
	for _, want := range []string{"# Where the panel opens.\nplacement = \"split\"", "ratio = 0.75", "port = 7000", "# port = 6363"} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}
	if strings.Contains(got, "startup") {
		t.Errorf("unset left the key:\n%s", got)
	}
	if _, err := os.Stat(path + ".bak-" + "20260101"); err != nil {
		t.Error("existing backup touched")
	}
	if p := load(t, dir, "").Files[0]; entry(t, p, "web.port").Value != int64(7000) {
		t.Error("reload did not see the change")
	}
}

func TestEnvFiles(t *testing.T) {
	dir := t.TempDir()
	path := write(t, dir, "config.env", `# How often to poll, in ms.
#APP_POLL_MS=500

# Debug logging.
#APP_DEBUG=false
APP_NAME="my app"
APP_MAX=0
`)
	c := load(t, dir, "")
	f := file(t, c, "config.env")
	poll := entry(t, f, "APP_POLL_MS")
	if poll.Set || poll.Key.Type != schema.TypeInteger || poll.Key.Default != "500" || poll.Key.Description != "How often to poll, in ms." {
		t.Errorf("poll = %+v", poll)
	}
	debug := entry(t, f, "APP_DEBUG")
	if def, ok := debug.Key.DefaultValue(); debug.Key.Type != schema.TypeBoolean || !ok || def != false {
		t.Errorf("debug = %+v", debug)
	}
	if e := entry(t, f, "APP_NAME"); e.Value != "my app" || e.Key.Type != schema.TypeString {
		t.Errorf("name = %+v", e)
	}
	if e := entry(t, f, "APP_MAX"); e.Value != int64(0) || e.Key.Type != schema.TypeInteger {
		t.Errorf("max = %+v", e)
	}

	for _, step := range []struct {
		key   string
		value any
	}{{"APP_POLL_MS", int64(250)}, {"APP_DEBUG", true}, {"APP_NAME", "other"}, {"APP_NEW", "x"}} {
		if err := f.Set(step.key, step.value); err != nil {
			t.Fatal(err)
		}
	}
	if err := f.Unset("APP_MAX"); err != nil {
		t.Fatal(err)
	}
	want := `# How often to poll, in ms.
#APP_POLL_MS=500
APP_POLL_MS=250

# Debug logging.
#APP_DEBUG=false
APP_DEBUG=true
APP_NAME="other"
APP_NEW=x
`
	if got := read(t, path); got != want {
		t.Fatalf("got:\n%s\nwant:\n%s", got, want)
	}
	if err := f.Set("APP_NAME", "two\nlines"); err == nil {
		t.Error("multi-line value accepted")
	}
	if err := f.Unset("APP_POLL_MS"); err != nil {
		t.Fatal(err)
	}
	if strings.Contains(read(t, path), "APP_POLL_MS=250") || !strings.Contains(read(t, path), "#APP_POLL_MS=500") {
		t.Errorf("unset:\n%s", read(t, path))
	}
}

func TestEnvKeepsCRLF(t *testing.T) {
	dir := t.TempDir()
	path := write(t, dir, "x.env", "#A=1\r\nB=2\r\n")
	f := file(t, load(t, dir, ""), "x.env")
	if err := f.Set("A", int64(3)); err != nil {
		t.Fatal(err)
	}
	if got := read(t, path); got != "#A=1\r\nA=3\r\nB=2\r\n" {
		t.Fatalf("got %q", got)
	}
}

func TestSchemaDescribesSettings(t *testing.T) {
	root, dir := t.TempDir(), t.TempDir()
	write(t, root, pluginconfig.SchemaFile, `{"file": "settings.toml", "sections": [{"id": "general", "title": "General", "keys": [
		{"key": "placement", "type": "enum", "default": "\"split\"", "description": "Where it opens.", "values": ["split", "popup", "tab"]},
		{"key": "panel.width", "type": "integer", "default": "40", "description": "Columns."}
	]}]}`)
	c := load(t, dir, root)
	if c.SchemaErr != nil {
		t.Fatal(c.SchemaErr)
	}
	f := file(t, c, "settings.toml")
	if !f.Missing || len(f.Entries) != 2 || !f.Entries[0].Documented || f.Entries[0].Key.Values[1] != "popup" {
		t.Fatalf("file = %+v", f)
	}
	if err := f.Set("panel.width", int64(50)); err != nil {
		t.Fatal(err)
	}
	if got := read(t, filepath.Join(dir, "settings.toml")); got != "[panel]\nwidth = 50\n" {
		t.Fatalf("got:\n%s", got)
	}
	write(t, dir, "settings.toml", "placement = \"tab\"\nextra = 1\n")
	f = file(t, load(t, dir, root), "settings.toml")
	if f.Missing || len(f.Entries) != 3 {
		t.Fatalf("entries = %d", len(f.Entries))
	}
	if e := entry(t, f, "placement"); !e.Documented || e.Value != "tab" {
		t.Errorf("placement = %+v", e)
	}
	if e := entry(t, f, "extra"); e.Documented || e.Key.Type != schema.TypeInteger {
		t.Errorf("extra = %+v", e)
	}

	write(t, root, pluginconfig.SchemaFile, "{not json")
	if c := load(t, dir, root); c.SchemaErr == nil || len(c.Files) != 1 {
		t.Errorf("broken schema: %+v", c)
	}
}

func TestSchemaTypesEnvValues(t *testing.T) {
	root, dir := t.TempDir(), t.TempDir()
	write(t, root, pluginconfig.SchemaFile, `{"file": "config.env", "sections": [{"id": "a", "keys": [
		{"key": "NAME", "type": "string", "default": "\"x\""}, {"key": "ON", "type": "boolean", "default": "false"}
	]}]}`)
	write(t, dir, "config.env", "NAME=123\nON=true\n")
	f := file(t, load(t, dir, root), "config.env")
	if e := entry(t, f, "NAME"); e.Value != "123" {
		t.Errorf("documented string read as %T", e.Value)
	}
	if e := entry(t, f, "ON"); e.Value != true {
		t.Errorf("on = %+v", e)
	}
}

func TestDir(t *testing.T) {
	testutil.FakeHerdr(t, testutil.HerdrResponse{Args: "plugin config-dir a.b", Stdout: "/some/dir\n"})
	if dir, err := pluginconfig.Dir("a.b"); err != nil || dir != "/some/dir" {
		t.Fatalf("Dir = %q, %v", dir, err)
	}
}

func TestEmptyDir(t *testing.T) {
	c := load(t, filepath.Join(t.TempDir(), "missing"), "")
	if len(c.Files) != 0 {
		t.Fatalf("files = %+v", c.Files)
	}
}
