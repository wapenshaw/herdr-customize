package theme_test

import (
	"path/filepath"
	"runtime"
	"testing"

	"github.com/wapenshaw/herdr-customize/internal/theme"
)

func TestStandaloneUsesInstalledPluginState(t *testing.T) {
	root := t.TempDir()
	t.Setenv("HERDR_PLUGIN_STATE_DIR", "")
	t.Setenv("XDG_STATE_HOME", root)
	managed := filepath.Join(root, "herdr", "plugins", theme.PluginID)
	if got := theme.StateDir(); got != managed {
		t.Fatalf("standalone sync uses %q instead of plugin state %q", got, managed)
	}
	explicit := t.TempDir()
	t.Setenv("HERDR_PLUGIN_STATE_DIR", explicit)
	if got := theme.StateDir(); got != explicit {
		t.Fatalf("explicit state directory ignored: %q", got)
	}
}

func TestStandaloneStateWindows(t *testing.T) {
	if runtime.GOOS != "windows" {
		t.Skip("Windows state location")
	}
	root := t.TempDir()
	t.Setenv("HERDR_PLUGIN_STATE_DIR", "")
	t.Setenv("XDG_STATE_HOME", "")
	t.Setenv("LOCALAPPDATA", root)
	managed := filepath.Join(root, "herdr", "plugins", theme.PluginID)
	if got := theme.StateDir(); got != managed {
		t.Fatalf("standalone state is not the plugin state: %q", got)
	}
	if got := theme.AppliedFile(); got != filepath.Join(managed, "applied") {
		t.Fatalf("selection marker outside plugin state: %q", got)
	}
}

func TestConfigPathMatchesHerdrPrecedence(t *testing.T) {
	root := t.TempDir()
	t.Setenv("HERDR_CONFIG_PATH", "")
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(root, "xdg"))
	t.Setenv("APPDATA", filepath.Join(root, "roaming"))
	want := filepath.Join(root, "xdg", "herdr", "config.toml")
	if got := theme.ConfigPath(); got != want {
		t.Fatalf("XDG ignored: %s", got)
	}
	t.Setenv("HERDR_CONFIG_PATH", filepath.Join(root, "explicit.toml"))
	if got := theme.ConfigPath(); got != filepath.Join(root, "explicit.toml") {
		t.Fatalf("explicit path ignored: %s", got)
	}
}

func TestMacConfigIgnoresApplicationSupportDecoy(t *testing.T) {
	if runtime.GOOS != "darwin" {
		t.Skip("macOS config resolution")
	}
	root := t.TempDir()
	t.Setenv("HOME", root)
	t.Setenv("HERDR_CONFIG_PATH", "")
	t.Setenv("XDG_CONFIG_HOME", "")
	writeTestFile(t, filepath.Join(root, "Library", "Application Support", "herdr", "config.toml"), "")
	if got := theme.ConfigPath(); got != filepath.Join(root, ".config", "herdr", "config.toml") {
		t.Fatalf("picked unused config: %s", got)
	}
}
