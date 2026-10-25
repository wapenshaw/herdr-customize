// Package schema describes Herdr's config keys from the config reference
// that Herdr publishes with each release. A copy for the version this plugin
// was built against is embedded; Load prefers a cached copy that matches the
// installed Herdr.
package schema

import (
	"context"
	_ "embed"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"time"

	"github.com/pelletier/go-toml/v2"
)

// BundledVersion is the Herdr release the embedded reference comes from.
const BundledVersion = "0.9.5"

//go:embed reference.json
var bundled []byte

// Key types used by the reference.
const (
	TypeBoolean     = "boolean"
	TypeInteger     = "integer"
	TypeString      = "string"
	TypePath        = "path"
	TypeEnum        = "enum"
	TypeColor       = "color"
	TypeKeybinding  = "keybinding"
	TypeArray       = "array"
	TypeStringList  = "list of strings"
	TypeTokenRows   = "list of token rows"
	TypeTokenTables = "table of token rows"
)

// Reference is the whole config reference.
type Reference struct {
	// Version is the Herdr release it describes.
	Version  string    `json:"-"`
	Sections []Section `json:"sections"`
}

// Section groups related keys, such as "ui" or "keys".
type Section struct {
	ID    string `json:"id"`
	Title string `json:"title"`
	Keys  []Key  `json:"keys"`
}

// Key is one config key.
type Key struct {
	Key         string   `json:"key"`
	Type        string   `json:"type"`
	Default     string   `json:"default"`
	Description string   `json:"description"`
	Values      []string `json:"values"`
}

// Path splits the dotted key.
func (k Key) Path() []string { return strings.Split(k.Key, ".") }

// DefaultValue decodes Default. It reports false for defaults that are prose,
// such as "unset" or a value with a platform note.
func (k Key) DefaultValue() (any, bool) {
	var document map[string]any
	if err := toml.Unmarshal([]byte("v = "+k.Default), &document); err != nil {
		return nil, false
	}
	return document["v"], true
}

// Editable reports whether the key has a simple value a form can edit.
func (k Key) Editable() bool {
	switch k.Type {
	case TypeBoolean, TypeInteger, TypeString, TypePath, TypeEnum, TypeColor, TypeKeybinding, TypeArray, TypeStringList:
		return true
	}
	return false
}

// Choices lists the values a key takes: an enum's values, or for a string
// key, the names its description lists after "Accepts" (as in "Accepts
// ctrl, alt, cmd, or a + separated combination"). Only enums are closed;
// see Open. Everything comes from the reference, so it follows Herdr.
func (k Key) Choices() []string {
	if len(k.Values) > 0 {
		return k.Values
	}
	if k.Type != TypeString {
		return nil
	}
	m := acceptsList.FindStringSubmatch(k.Description)
	if m == nil {
		return nil
	}
	var choices []string
	// An empty value is offered first when the description says it disables
	// the setting.
	if strings.Contains(k.Description, "Empty disables") {
		choices = append(choices, "")
	}
	for _, name := range strings.Split(m[1], ", ") {
		// "a, b, or a combination" ends the list at "or".
		if name == "or" || name == "and" {
			break
		}
		choices = append(choices, name)
	}
	return choices
}

// acceptsList matches "Accepts a, b, c" with at least two plain names.
var acceptsList = regexp.MustCompile(`Accepts ((?:[a-z_]+, )+[a-z_]+)\b`)

// Open reports a key whose Choices are suggestions: other values are valid.
func (k Key) Open() bool { return k.Type != TypeEnum && len(k.Choices()) > 0 }

// Section returns the section with id.
func (r *Reference) Section(id string) (Section, bool) {
	for _, s := range r.Sections {
		if s.ID == id {
			return s, true
		}
	}
	return Section{}, false
}

// Lookup returns the key named key.
func (r *Reference) Lookup(key string) (Key, bool) {
	for _, s := range r.Sections {
		for _, k := range s.Keys {
			if k.Key == key {
				return k, true
			}
		}
	}
	return Key{}, false
}

func parse(data []byte, version string) (*Reference, error) {
	var ref Reference
	if err := json.Unmarshal(data, &ref); err != nil {
		return nil, err
	}
	if len(ref.Sections) == 0 {
		return nil, errors.New("config reference has no sections")
	}
	ref.Version = version
	return &ref, nil
}

// Bundled returns the embedded reference.
func Bundled() *Reference {
	ref, err := parse(bundled, BundledVersion)
	if err != nil {
		panic(fmt.Sprintf("embedded config reference: %v", err))
	}
	return ref
}

// Load returns the cached reference for version from cacheDir, or the
// bundled one.
func Load(cacheDir, version string) *Reference {
	if version != "" && version != BundledVersion {
		if data, err := os.ReadFile(cachePath(cacheDir, version)); err == nil {
			if ref, err := parse(data, version); err == nil {
				return ref
			}
		}
	}
	return Bundled()
}

// URL is where Herdr publishes the reference for a release.
func URL(version string) string {
	return "https://raw.githubusercontent.com/herdrdev/herdr/v" + version + "/docs/next/website/src/data/config-reference.json"
}

func cachePath(dir, version string) string {
	return filepath.Join(dir, "config-reference-"+version+".json")
}

// Fetch downloads the reference for version into cacheDir and returns it.
func Fetch(ctx context.Context, cacheDir, version string) (*Reference, error) {
	if version == BundledVersion {
		return Bundled(), nil
	}
	ctx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, URL(version), nil)
	if err != nil {
		return nil, err
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("config reference for herdr %s: %s", version, resp.Status)
	}
	data, err := io.ReadAll(io.LimitReader(resp.Body, 4<<20))
	if err != nil {
		return nil, err
	}
	ref, err := parse(data, version)
	if err != nil {
		return nil, err
	}
	if err := os.MkdirAll(cacheDir, 0o755); err == nil {
		_ = os.WriteFile(cachePath(cacheDir, version), data, 0o644)
	}
	return ref, nil
}
