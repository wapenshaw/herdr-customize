package config_test

import (
	"errors"
	"strings"
	"testing"

	"github.com/wapenshaw/herdr-customize/internal/config"
	"github.com/wapenshaw/herdr-customize/tests/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

func parse(t *testing.T, body string) *config.Doc {
	t.Helper()
	doc, err := config.Parse([]byte(body))
	if err != nil {
		t.Fatal(err)
	}
	return doc
}

func expect(t *testing.T, doc *config.Doc, want string) {
	t.Helper()
	if got := string(doc.Bytes()); got != want {
		t.Fatalf("unexpected document:\n--- got ---\n%s\n--- want ---\n%s", got, want)
	}
}

func TestSetReplacesValueOnly(t *testing.T) {
	doc := parse(t, "# top\n[ui]\n  # gaps\n  pane_gaps = false   # trailing\nmouse_capture = true\n")
	if err := doc.Set([]string{"ui", "pane_gaps"}, true); err != nil {
		t.Fatal(err)
	}
	expect(t, doc, "# top\n[ui]\n  # gaps\n  pane_gaps = true   # trailing\nmouse_capture = true\n")
}

func TestSetReplacesMultilineArray(t *testing.T) {
	doc := parse(t, "[ui]\ntab_bar_right = [\n  \"zoom\", # z\n  \"hostname\",\n]\nmouse_capture = true\n")
	if err := doc.Set([]string{"ui", "tab_bar_right"}, []string{"datetime"}); err != nil {
		t.Fatal(err)
	}
	expect(t, doc, "[ui]\ntab_bar_right = [\"datetime\"]\nmouse_capture = true\n")
}

func TestSetInsertsIntoExistingTable(t *testing.T) {
	doc := parse(t, "[ui]\n    mouse_capture = true\n\n# keys follow\n[keys]\nzoom = \"prefix+z\"\n")
	if err := doc.Set([]string{"ui", "pane_gaps"}, false); err != nil {
		t.Fatal(err)
	}
	expect(t, doc, "[ui]\n    mouse_capture = true\n    pane_gaps = false\n\n# keys follow\n[keys]\nzoom = \"prefix+z\"\n")
}

func TestSetUsesDeepestTable(t *testing.T) {
	doc := parse(t, "[ui]\nmouse_capture = true\n\n[ui.toast]\ndelivery = \"herdr\"\n")
	if err := doc.Set([]string{"ui", "toast", "herdr", "position"}, "top-left"); err != nil {
		t.Fatal(err)
	}
	expect(t, doc, "[ui]\nmouse_capture = true\n\n[ui.toast]\ndelivery = \"herdr\"\nherdr.position = \"top-left\"\n")
}

func TestSetCreatesTableBesideRelative(t *testing.T) {
	doc := parse(t, "[ui.toast]\ndelivery = \"herdr\"\n\n[keys]\nzoom = \"prefix+z\"\n")
	if err := doc.Set([]string{"ui", "sound", "enabled"}, true); err != nil {
		t.Fatal(err)
	}
	expect(t, doc, "[ui.toast]\ndelivery = \"herdr\"\n\n[ui.sound]\nenabled = true\n\n[keys]\nzoom = \"prefix+z\"\n")
}

func TestSetCreatesTableInEmptyAndUnterminatedFiles(t *testing.T) {
	doc := parse(t, "")
	if err := doc.Set([]string{"theme", "name"}, "nord"); err != nil {
		t.Fatal(err)
	}
	expect(t, doc, "[theme]\nname = \"nord\"\n")

	doc = parse(t, "onboarding = false")
	if err := doc.Set([]string{"ui", "accent"}, "#89b4fa"); err != nil {
		t.Fatal(err)
	}
	expect(t, doc, "onboarding = false\n\n[ui]\naccent = \"#89b4fa\"\n")
}

func TestSetFallsBackToRootDottedKeys(t *testing.T) {
	// [ui] cannot be declared after ui was defined with dotted keys.
	doc := parse(t, "ui.mouse_capture = true\n\n[keys]\nzoom = \"prefix+z\"\n")
	if err := doc.Set([]string{"ui", "pane_gaps"}, true); err != nil {
		t.Fatal(err)
	}
	expect(t, doc, "ui.mouse_capture = true\nui.pane_gaps = true\n\n[keys]\nzoom = \"prefix+z\"\n")
}

func TestSetRefusesInlineTables(t *testing.T) {
	doc := parse(t, "theme = { name = \"nord\" }\n")
	if err := doc.Set([]string{"theme", "auto_switch"}, true); !errors.Is(err, config.ErrInline) {
		t.Fatalf("expected ErrInline, got %v", err)
	}
	if _, err := doc.Unset([]string{"theme", "name"}); !errors.Is(err, config.ErrInline) {
		t.Fatalf("expected ErrInline, got %v", err)
	}
}

func TestSetQuotesKeysAndStrings(t *testing.T) {
	doc := parse(t, "")
	if err := doc.Set([]string{"ui", "window_title"}, "say \"hi\"\t{tab}"); err != nil {
		t.Fatal(err)
	}
	if err := doc.Set([]string{"odd key", "x"}, int64(3)); err != nil {
		t.Fatal(err)
	}
	if got, _ := doc.Get("ui", "window_title"); got != "say \"hi\"\t{tab}" {
		t.Fatalf("string round trip: %q", got)
	}
	if !strings.Contains(string(doc.Bytes()), "[\"odd key\"]") {
		t.Fatalf("key not quoted:\n%s", doc.Bytes())
	}
}

func TestCRLFIsPreserved(t *testing.T) {
	doc := parse(t, "[ui]\r\nmouse_capture = true\r\n")
	if err := doc.Set([]string{"ui", "pane_gaps"}, false); err != nil {
		t.Fatal(err)
	}
	if err := doc.Set([]string{"keys", "zoom"}, "prefix+z"); err != nil {
		t.Fatal(err)
	}
	expect(t, doc, "[ui]\r\nmouse_capture = true\r\npane_gaps = false\r\n\r\n[keys]\r\nzoom = \"prefix+z\"\r\n")
}

func TestUnsetRemovesOnlyThatKey(t *testing.T) {
	doc := parse(t, "[ui]\n# keep me\nmouse_capture = true # gone\npane_gaps = false\n")
	removed, err := doc.Unset([]string{"ui", "mouse_capture"})
	if err != nil || !removed {
		t.Fatalf("removed=%v err=%v", removed, err)
	}
	expect(t, doc, "[ui]\n# keep me\npane_gaps = false\n")
	if removed, err := doc.Unset([]string{"ui", "missing"}); err != nil || removed {
		t.Fatalf("missing key: removed=%v err=%v", removed, err)
	}
}

const commands = `[keys]
zoom = "prefix+z"

# my bindings
[[keys.command]]
key = "prefix+t"
type = "plugin_action"
command = "herdr-theme-picker.open" # old plugin
description = "Theme picker"

[[keys.command]]
key = "prefix+g"
type = "popup"
command = "lazygit"
`

func TestArrayTableAppend(t *testing.T) {
	doc := parse(t, commands)
	err := doc.AppendArrayTable([]string{"keys", "command"}, []config.Field{
		{Key: "key", Value: "prefix+alt+c"}, {Key: "type", Value: "plugin_action"}, {Key: "command", Value: "herdr-customize.open"},
	})
	if err != nil {
		t.Fatal(err)
	}
	expect(t, doc, commands+"\n[[keys.command]]\nkey = \"prefix+alt+c\"\ntype = \"plugin_action\"\ncommand = \"herdr-customize.open\"\n")

	doc = parse(t, "[ui]\nmouse_capture = true")
	if err := doc.AppendArrayTable([]string{"keys", "command"}, []config.Field{{Key: "key", Value: "f5"}, {Key: "type", Value: "shell"}, {Key: "command", Value: "true"}}); err != nil {
		t.Fatal(err)
	}
	expect(t, doc, "[ui]\nmouse_capture = true\n\n[[keys.command]]\nkey = \"f5\"\ntype = \"shell\"\ncommand = \"true\"\n")
}

func TestArrayTableUpdateKeepsComments(t *testing.T) {
	doc := parse(t, commands)
	err := doc.UpdateArrayTable([]string{"keys", "command"}, 0, []config.Field{
		{Key: "key", Value: "prefix+t"}, {Key: "type", Value: "plugin_action"}, {Key: "command", Value: "herdr-customize.open"}, {Key: "width", Value: "80%"},
	})
	if err != nil {
		t.Fatal(err)
	}
	want := strings.Replace(commands, "command = \"herdr-theme-picker.open\" # old plugin\ndescription = \"Theme picker\"\n", "command = \"herdr-customize.open\" # old plugin\nwidth = \"80%\"\n", 1)
	expect(t, doc, want)
}

func TestArrayTableDelete(t *testing.T) {
	doc := parse(t, commands)
	if err := doc.DeleteArrayTable([]string{"keys", "command"}, 0); err != nil {
		t.Fatal(err)
	}
	expect(t, doc, "[keys]\nzoom = \"prefix+z\"\n\n# my bindings\n[[keys.command]]\nkey = \"prefix+g\"\ntype = \"popup\"\ncommand = \"lazygit\"\n")
	if err := doc.DeleteArrayTable([]string{"keys", "command"}, 0); err != nil {
		t.Fatal(err)
	}
	expect(t, doc, "[keys]\nzoom = \"prefix+z\"\n\n# my bindings\n")
	if err := doc.DeleteArrayTable([]string{"keys", "command"}, 0); err == nil {
		t.Fatal("deleted a missing entry")
	}
}

func TestInlineArrayTableIsReadOnly(t *testing.T) {
	doc := parse(t, "[keys]\ncommand = [{ key = \"f5\", type = \"shell\", command = \"true\" }]\n")
	if got := doc.ArrayTable([]string{"keys", "command"}); len(got) != 1 || got[0]["key"] != "f5" {
		t.Fatalf("inline entries not decoded: %#v", got)
	}
	err := doc.AppendArrayTable([]string{"keys", "command"}, []config.Field{{Key: "key", Value: "f6"}})
	if !errors.Is(err, config.ErrInline) {
		t.Fatalf("expected ErrInline, got %v", err)
	}
}
