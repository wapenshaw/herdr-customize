package theme_test

import (
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	. "github.com/wapenshaw/herdr-customize/internal/theme"
)

func TestImportLegacyThemes(t *testing.T) {
	root := t.TempDir()
	t.Setenv("HERDR_PLUGIN_STATE_DIR", filepath.Join(root, PluginID))
	legacy := filepath.Join(root, LegacyPluginID)
	if LegacyStateDir() != legacy {
		t.Fatalf("LegacyStateDir = %s", LegacyStateDir())
	}
	body := fixturePalette(t)
	writeTestFile(t, filepath.Join(legacy, "themes", "old-one"), body)
	writeTestFile(t, filepath.Join(legacy, "themes", "kept"), "background = #000000\n")
	writeTestFile(t, filepath.Join(legacy, "index.txt"), "old-one\nkept\nmissing\n")
	// A theme already here is never replaced.
	mine := strings.Replace(body, "background", "# mine\nbackground", 1)
	if err := saveFixtureTheme(t, "kept", mine); err != nil {
		t.Fatal(err)
	}

	if got := LegacyThemes(); !slices.Equal(got, []string{"old-one"}) {
		t.Fatalf("LegacyThemes = %v", got)
	}
	imported, err := ImportLegacyThemes()
	if err != nil || !slices.Equal(imported, []string{"old-one"}) {
		t.Fatalf("imported %v, %v", imported, err)
	}
	if got, _ := UserThemeBody("old-one"); got != body {
		t.Fatal("imported body differs")
	}
	if data, _ := os.ReadFile(filepath.Join(UserThemesDir(), "kept")); string(data) != mine {
		t.Fatal("existing theme replaced")
	}
	if len(LegacyThemes()) != 0 {
		t.Fatal("themes offered again after import")
	}
}

func TestLegacyStateDirNeedsKnownLayout(t *testing.T) {
	t.Setenv("HERDR_PLUGIN_STATE_DIR", t.TempDir())
	if LegacyStateDir() != "" || LegacyThemes() != nil {
		t.Fatal("guessed a legacy dir beside an unrelated state dir")
	}
}
