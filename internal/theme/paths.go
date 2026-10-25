package theme

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/wapenshaw/herdr-customize/internal/config"
)

// PluginID is the Herdr plugin id from herdr-plugin.toml.
const PluginID = "herdr-customize"

// ConfigPath returns the location of Herdr's config.toml across platforms.
func ConfigPath() string { return config.Path() }

// PluginRoot returns the directory containing plugin assets (themes, herdr-plugin.toml).
func PluginRoot() string {
	if root := os.Getenv("HERDR_PLUGIN_ROOT"); root != "" {
		return root
	}
	exe, err := os.Executable()
	if err == nil {
		exe = strings.TrimPrefix(exe, `\\?\`)
		dir := filepath.Dir(exe)
		if _, err := os.Stat(filepath.Join(dir, "themes")); err == nil {
			return dir
		}
	}
	cwd, err := os.Getwd()
	if err == nil {
		if _, err := os.Stat(filepath.Join(cwd, "themes")); err == nil {
			return cwd
		}
		parent := filepath.Dir(cwd)
		if _, err := os.Stat(filepath.Join(parent, "themes")); err == nil {
			return parent
		}
		grandParent := filepath.Dir(parent)
		if _, err := os.Stat(filepath.Join(grandParent, "themes")); err == nil {
			return grandParent
		}
	}
	return "."
}

// StateDir returns the directory where plugin state (applied theme, user themes) is persisted.
func StateDir() string {
	if state := os.Getenv("HERDR_PLUGIN_STATE_DIR"); state != "" {
		return state
	}
	// Herdr supplies the variable above to plugin commands. Standalone
	// picker/sync runs use the same location so both share user themes and
	// the applied marker.
	if managed := managedStateDir(); managed != "" {
		return managed
	}
	return filepath.Join(".", ".state")
}

func managedStateDir() string {
	if state := os.Getenv("XDG_STATE_HOME"); state != "" {
		return filepath.Join(state, "herdr", "plugins", PluginID)
	}
	if runtime.GOOS == "windows" {
		if local := os.Getenv("LOCALAPPDATA"); local != "" {
			return filepath.Join(local, "herdr", "plugins", PluginID)
		}
		if profile := os.Getenv("USERPROFILE"); profile != "" {
			return filepath.Join(profile, "AppData", "Local", "herdr", "plugins", PluginID)
		}
	}
	if home, err := os.UserHomeDir(); err == nil {
		return filepath.Join(home, ".local", "state", "herdr", "plugins", PluginID)
	}
	return ""
}

// CacheDir returns the download cache directory for remote themes.
func CacheDir() string {
	return filepath.Join(StateDir(), "cache")
}

// UserThemesDir returns the directory storing user-added themes.
func UserThemesDir() string {
	return filepath.Join(StateDir(), "themes")
}

// UserIndexFile returns the path to the user themes index list.
func UserIndexFile() string {
	return filepath.Join(StateDir(), "index.txt")
}

// AppliedFile returns the path to the file storing the active theme slug.
// It is per machine, like the config it describes.
func AppliedFile() string {
	return filepath.Join(StateDir(), "applied")
}

// IsUserTheme reports whether slug is an existing user-added theme.
func IsUserTheme(slug string) bool {
	_, err := userThemePath(slug)
	return err == nil
}
