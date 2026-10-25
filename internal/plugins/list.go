// Package plugins reads and manages installed Herdr plugins through the
// herdr plugin CLI, checks GitHub for updates, and searches the marketplace.
package plugins

import (
	"encoding/json"
	"fmt"
	"runtime"
	"slices"

	"github.com/wapenshaw/herdr-customize/internal/herdr"
)

// Plugin is one installed or linked plugin.
type Plugin struct {
	ID          string   `json:"plugin_id"`
	Name        string   `json:"name"`
	Version     string   `json:"version"`
	Description string   `json:"description"`
	Enabled     bool     `json:"enabled"`
	Root        string   `json:"plugin_root"`
	Manifest    string   `json:"manifest_path"`
	Platforms   []string `json:"platforms"`
	Source      Source   `json:"source"`
	Actions     []Action `json:"actions"`
}

// Source says where a plugin came from.
type Source struct {
	// Kind is "github" for installs and something else for linked checkouts.
	Kind   string `json:"kind"`
	Owner  string `json:"owner"`
	Repo   string `json:"repo"`
	Subdir string `json:"subdir"`
	// RequestedRef is the branch, tag or commit named at install time.
	RequestedRef string `json:"requested_ref"`
	LegacyRef    string `json:"ref"`
	Commit       string `json:"resolved_commit"`
	Path         string `json:"managed_path"`
}

// Action is a plugin action that a keybinding or the Plugins page can run.
type Action struct {
	ID        string   `json:"id"`
	Title     string   `json:"title"`
	Command   []string `json:"command"`
	Contexts  []string `json:"contexts"`
	Platforms []string `json:"platforms"`
}

// GitHub reports whether the plugin was installed from GitHub.
func (p Plugin) GitHub() bool {
	return p.Source.Kind == "github" && p.Source.Owner != "" && p.Source.Repo != ""
}

// Slug is owner/repo for GitHub installs.
func (p Plugin) Slug() string { return p.Source.Owner + "/" + p.Source.Repo }

// Spec is what herdr plugin install takes: owner/repo[/subdir].
func (p Plugin) Spec() string {
	if p.Source.Subdir != "" {
		return p.Slug() + "/" + p.Source.Subdir
	}
	return p.Slug()
}

// Ref is the requested install ref, or "" for the default branch.
func (p Plugin) Ref() string {
	if p.Source.RequestedRef != "" {
		return p.Source.RequestedRef
	}
	return p.Source.LegacyRef
}

// RepoURL is the plugin's GitHub page, at the installed commit for subdir plugins.
func (p Plugin) RepoURL() string {
	if !p.GitHub() {
		return ""
	}
	url := "https://github.com/" + p.Slug()
	if p.Source.Subdir != "" && p.Source.Commit != "" {
		url += "/tree/" + p.Source.Commit + "/" + p.Source.Subdir
	}
	return url
}

// QualifiedID is the plugin.action name keybindings use.
func (a Action) QualifiedID(plugin string) string { return plugin + "." + a.ID }

// Available reports whether the action runs on this OS.
func (a Action) Available() bool {
	if len(a.Platforms) == 0 {
		return true
	}
	return slices.Contains(a.Platforms, platform())
}

func platform() string {
	if runtime.GOOS == "darwin" {
		return "macos"
	}
	return runtime.GOOS
}

// List returns the installed plugins.
func List() ([]Plugin, error) {
	out, err := herdr.Run("plugin", "list", "--json")
	if err != nil {
		return nil, fmt.Errorf("herdr plugin list: %w", err)
	}
	return ParseList([]byte(out))
}

// ParseList decodes herdr plugin list --json output.
func ParseList(data []byte) ([]Plugin, error) {
	var payload struct {
		Result struct {
			Plugins []Plugin `json:"plugins"`
		} `json:"result"`
	}
	if err := json.Unmarshal(data, &payload); err != nil {
		return nil, fmt.Errorf("unexpected herdr plugin list output: %w", err)
	}
	return payload.Result.Plugins, nil
}
