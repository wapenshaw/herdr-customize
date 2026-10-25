package config

import (
	"os"
	"path/filepath"
	"runtime"
)

// Path returns the location of Herdr's config.toml, using Herdr's precedence:
// HERDR_CONFIG_PATH, XDG_CONFIG_HOME, %APPDATA% on Windows, then ~/.config.
func Path() string {
	if custom := os.Getenv("HERDR_CONFIG_PATH"); custom != "" {
		return custom
	}
	if xdg := os.Getenv("XDG_CONFIG_HOME"); xdg != "" {
		return filepath.Join(xdg, "herdr", "config.toml")
	}
	if runtime.GOOS == "windows" {
		if appData := os.Getenv("APPDATA"); appData != "" {
			return filepath.Join(appData, "herdr", "config.toml")
		}
		if profile := os.Getenv("USERPROFILE"); profile != "" {
			return filepath.Join(profile, "AppData", "Roaming", "herdr", "config.toml")
		}
	}
	if home, err := os.UserHomeDir(); err == nil {
		return filepath.Join(home, ".config", "herdr", "config.toml")
	}
	return "config.toml"
}
