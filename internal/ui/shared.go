package ui

import (
	"context"
	"errors"
	"fmt"
	"path/filepath"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/wapenshaw/herdr-customize/internal/config"
	"github.com/wapenshaw/herdr-customize/internal/herdr"
	"github.com/wapenshaw/herdr-customize/internal/plugins"
	"github.com/wapenshaw/herdr-customize/internal/schema"
	"github.com/wapenshaw/herdr-customize/internal/theme"
)

// shared is state every page reads: the config file and the key reference.
type shared struct {
	store *config.Store
	ref   *schema.Reference
}

func newShared() *shared {
	return &shared{store: config.Default(), ref: schema.Bundled()}
}

type (
	// referenceMsg delivers the key reference matching the installed Herdr.
	referenceMsg struct{ ref *schema.Reference }
	// savedMsg reports a config save; every page reloads after one.
	savedMsg struct {
		what   string
		result config.SaveResult
		err    error
	}
	// pluginsMsg delivers the installed plugins.
	pluginsMsg struct {
		list []plugins.Plugin
		err  error
	}
)

func schemaCacheDir() string { return filepath.Join(theme.StateDir(), "schema") }

// loadReference prefers the reference for the installed Herdr version.
func loadReference() tea.Msg {
	version, err := herdr.Version()
	if err != nil || version == schema.BundledVersion {
		return nil
	}
	ref := schema.Load(schemaCacheDir(), version)
	if ref.Version == version {
		return referenceMsg{ref: ref}
	}
	fetched, err := schema.Fetch(context.Background(), schemaCacheDir(), version)
	if err != nil {
		return nil
	}
	return referenceMsg{ref: fetched}
}

func loadPlugins() tea.Msg {
	list, err := plugins.List()
	return pluginsMsg{list: list, err: err}
}

// save applies edit to the config in the background.
func (s *shared) save(what string, edit func(*config.Doc) error) tea.Cmd {
	return func() tea.Msg {
		result, err := s.store.Update(edit)
		return savedMsg{what: what, result: result, err: err}
	}
}

// describe turns a save into a status line.
func (m savedMsg) describe() (string, bool) {
	var check *config.CheckError
	switch {
	case errors.As(m.err, &check):
		return "not saved: " + check.Error(), true
	case errors.Is(m.err, config.ErrInline):
		return fmt.Sprintf("not saved: %v", m.err), true
	case m.err != nil:
		return "not saved: " + m.err.Error(), true
	case !m.result.Changed:
		return m.what + ": no change", false
	case m.result.ReloadErr != nil:
		return m.what + " saved; Herdr did not reload, press prefix+shift+r", false
	case m.result.CheckSkipped != "":
		return m.what + " saved (herdr config check unavailable)", false
	}
	return m.what + " saved and reloaded", false
}
