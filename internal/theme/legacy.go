package theme

import (
	"errors"
	"os"
	"path/filepath"
	"slices"
)

// LegacyPluginID is herdr-theme-picker, which this plugin continues.
const LegacyPluginID = "herdr-theme-picker"

// LegacyStateDir is where herdr-theme-picker kept user themes: beside this
// plugin's state directory when Herdr supplies one.
func LegacyStateDir() string {
	if state := os.Getenv("HERDR_PLUGIN_STATE_DIR"); state != "" {
		if filepath.Base(state) != PluginID {
			return ""
		}
		return filepath.Join(filepath.Dir(state), LegacyPluginID)
	}
	return managedStateDir(LegacyPluginID)
}

// LegacyThemes lists herdr-theme-picker user themes that are not here yet.
func LegacyThemes() []string {
	dir := LegacyStateDir()
	if dir == "" {
		return nil
	}
	slugs, _, err := readIndex(filepath.Join(dir, "index.txt"), isStoredUserSlug)
	if err != nil {
		return nil
	}
	current, _ := readUserIndex()
	var out []string
	for _, slug := range slugs {
		if slices.Contains(current, slug) || IsUserTheme(slug) {
			continue
		}
		if info, err := os.Lstat(filepath.Join(dir, "themes", slug)); err == nil && info.Mode().IsRegular() {
			out = append(out, slug)
		}
	}
	return out
}

// ImportLegacyThemes copies herdr-theme-picker user themes here, never
// replacing a theme that already exists. It returns the imported slugs.
func ImportLegacyThemes() ([]string, error) {
	dir := LegacyStateDir()
	var imported []string
	var errs []error
	for _, slug := range LegacyThemes() {
		body, err := os.ReadFile(filepath.Join(dir, "themes", slug))
		if err == nil {
			_, err = SaveUserTheme(slug, string(body), false)
		}
		switch {
		case errors.Is(err, ErrThemeExists):
		case err != nil:
			errs = append(errs, err)
		default:
			imported = append(imported, slug)
		}
	}
	return imported, errors.Join(errs...)
}
