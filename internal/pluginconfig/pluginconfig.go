// Package pluginconfig reads and edits the files a plugin keeps in its Herdr
// config directory. Herdr gives plugins the directory but no format, so the
// types come from a config-schema.json the plugin ships when there is one,
// and are otherwise inferred from the values and from commented-out
// defaults such as "#POLL_MS=500".
package pluginconfig

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"strconv"
	"strings"

	"github.com/pelletier/go-toml/v2"

	"github.com/wapenshaw/herdr-customize/internal/config"
	"github.com/wapenshaw/herdr-customize/internal/fsutil"
	"github.com/wapenshaw/herdr-customize/internal/herdr"
	"github.com/wapenshaw/herdr-customize/internal/schema"
)

// SchemaFile is what a plugin ships in its root to describe its config, in
// the format of Herdr's config-reference.json plus an optional "file" naming
// the config file (config.toml when absent).
const SchemaFile = "config-schema.json"

// Format is how a config file is written.
type Format int

const (
	Other Format = iota
	TOML
	Env
)

// File is one file in a plugin's config directory.
type File struct {
	Name    string
	Path    string
	Format  Format
	Missing bool // described by the schema but not created yet
	Entries []*Entry
}

// Entry is one setting in a file.
type Entry struct {
	// Key holds the name (a dotted path in TOML), type, default and
	// description, whether documented by a schema or inferred.
	Key   schema.Key
	Value any
	Set   bool
	// Documented entries come from the plugin's schema.
	Documented bool
	// ReadOnly entries cannot be edited here, such as arrays of tables.
	ReadOnly bool
}

// Config is a plugin's config directory.
type Config struct {
	Dir       string
	Files     []*File
	Schema    *schema.Reference
	SchemaErr error
}

// Dir asks Herdr for a plugin's config directory.
func Dir(pluginID string) (string, error) {
	out, err := herdr.Run("plugin", "config-dir", pluginID)
	if err != nil {
		return "", err
	}
	dir := strings.TrimSpace(out)
	if dir == "" {
		return "", fmt.Errorf("herdr gave no config directory for %s", pluginID)
	}
	return dir, nil
}

// Load reads every file in dir, using root's schema when present.
func Load(dir, root string) (*Config, error) {
	c := &Config{Dir: dir}
	if root != "" {
		if data, err := os.ReadFile(filepath.Join(root, SchemaFile)); err == nil {
			c.Schema, c.SchemaErr = schema.Parse(data)
		} else if !errors.Is(err, os.ErrNotExist) {
			c.SchemaErr = err
		}
	}
	entries, err := os.ReadDir(dir)
	if err != nil && !errors.Is(err, os.ErrNotExist) {
		return nil, err
	}
	for _, e := range entries {
		name := e.Name()
		if strings.HasPrefix(name, ".") || strings.Contains(name, ".bak-") || strings.HasSuffix(name, ".lock") {
			continue
		}
		path := filepath.Join(dir, name)
		// Follow symlinks: a plugin may link its config from elsewhere.
		info, err := os.Stat(path)
		if err != nil || !info.Mode().IsRegular() {
			continue
		}
		c.Files = append(c.Files, &File{Name: name, Path: path, Format: formatOf(name)})
	}
	if c.Schema != nil {
		name := c.schemaFile()
		if !slices.ContainsFunc(c.Files, func(f *File) bool { return f.Name == name }) {
			c.Files = append(c.Files, &File{Name: name, Path: filepath.Join(dir, name), Format: formatOf(name), Missing: true})
		}
	}
	// Editable files first, in name order.
	slices.SortStableFunc(c.Files, func(a, b *File) int {
		if (a.Format == Other) != (b.Format == Other) {
			if a.Format == Other {
				return 1
			}
			return -1
		}
		return strings.Compare(a.Name, b.Name)
	})
	for _, f := range c.Files {
		if err := c.read(f); err != nil {
			return nil, fmt.Errorf("%s: %w", f.Name, err)
		}
	}
	return c, nil
}

func formatOf(name string) Format {
	switch strings.ToLower(filepath.Ext(name)) {
	case ".toml":
		return TOML
	case ".env":
		return Env
	}
	return Other
}

func (c *Config) schemaFile() string {
	if c.Schema != nil && c.Schema.File != "" {
		return c.Schema.File
	}
	return "config.toml"
}

// documented returns the schema keys for f, if the schema describes it.
func (c *Config) documented(f *File) []schema.Key {
	if c.Schema == nil || f.Name != c.schemaFile() {
		return nil
	}
	var keys []schema.Key
	for _, s := range c.Schema.Sections {
		keys = append(keys, s.Keys...)
	}
	return keys
}

func (c *Config) read(f *File) error {
	var data []byte
	if !f.Missing {
		var err error
		if data, err = os.ReadFile(f.Path); err != nil {
			return err
		}
	}
	var found []*Entry
	switch f.Format {
	case TOML:
		entries, err := readTOML(data)
		if err != nil {
			return err
		}
		found = entries
	case Env:
		found = readEnv(data)
	default:
		return nil
	}
	f.Entries = merge(c.documented(f), found, f.Format)
	return nil
}

// merge puts documented keys first, filled with the values found, then any
// other keys found in the file.
func merge(documented []schema.Key, found []*Entry, format Format) []*Entry {
	byName := map[string]*Entry{}
	for _, e := range found {
		byName[e.Key.Key] = e
	}
	var out []*Entry
	for _, k := range documented {
		e := &Entry{Key: k, Documented: true}
		if f, ok := byName[k.Key]; ok {
			e.Set, e.Value = f.Set, f.Value
			if format == Env && f.Set {
				e.Value = typed(k.Type, envString(f.Value))
			}
			delete(byName, k.Key)
		}
		out = append(out, e)
	}
	for _, e := range found {
		if _, ok := byName[e.Key.Key]; ok {
			out = append(out, e)
		}
	}
	return out
}

// typeOf infers a schema type from a decoded value.
func typeOf(value any) string {
	switch value.(type) {
	case bool:
		return schema.TypeBoolean
	case int64:
		return schema.TypeInteger
	case float64:
		return schema.TypeFloat
	case []any:
		return schema.TypeArray
	case string:
		return schema.TypeString
	}
	return schema.TypeArray
}

// Set writes key in f, creating the file when needed. Load again to see
// the change.
func (f *File) Set(key string, value any) error {
	return f.edit(func(data []byte) ([]byte, error) {
		if f.Format == Env {
			return setEnv(data, key, envString(value))
		}
		doc, err := config.Parse(data)
		if err != nil {
			return nil, err
		}
		if err := doc.Set(splitKey(key), value); err != nil {
			return nil, err
		}
		return doc.Bytes(), nil
	})
}

// Unset removes key from f, so the plugin's default applies.
func (f *File) Unset(key string) error {
	return f.edit(func(data []byte) ([]byte, error) {
		if f.Format == Env {
			return unsetEnv(data, key), nil
		}
		doc, err := config.Parse(data)
		if err != nil {
			return nil, err
		}
		if _, err := doc.Unset(splitKey(key)); err != nil {
			return nil, err
		}
		return doc.Bytes(), nil
	})
}

func (f *File) edit(change func([]byte) ([]byte, error)) error {
	if f.Format == Other {
		return fmt.Errorf("%s is not a TOML or .env file", f.Name)
	}
	old, err := os.ReadFile(f.Path)
	if err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	data, err := change(old)
	if err != nil {
		return err
	}
	if string(data) == string(old) {
		return nil
	}
	if len(old) > 0 {
		if err := fsutil.DailyBackup(f.Path, old); err != nil {
			return err
		}
	}
	if err := os.MkdirAll(filepath.Dir(f.Path), 0o755); err != nil {
		return err
	}
	return fsutil.AtomicWrite(f.Path, data, 0o644)
}

// splitKey splits a dotted TOML path; quoted parts are not supported.
func splitKey(key string) []string { return strings.Split(key, ".") }

/* ---------------------------------------------------------------- TOML */

var (
	tomlHeader = regexp.MustCompile(`^\s*\[\s*([A-Za-z0-9_.-]+)\s*\]\s*(#.*)?$`)
	tomlArray  = regexp.MustCompile(`^\s*\[\[`)
	tomlKey    = regexp.MustCompile(`^\s*([A-Za-z0-9_.-]+)\s*=`)
	tomlNote   = regexp.MustCompile(`^\s*#\s?(.*)$`)
)

// readTOML lists the keys in file order, and keys that only appear as a
// commented-out default ("# key = value") with the comment above them.
func readTOML(data []byte) ([]*Entry, error) {
	doc, err := config.Parse(data)
	if err != nil {
		return nil, err
	}
	var out []*Entry
	seen := map[string]bool{}
	add := func(e *Entry) {
		if !seen[e.Key.Key] {
			seen[e.Key.Key] = true
			out = append(out, e)
		}
	}
	table := ""
	var note []string
	defaults := map[string]*Entry{}
	for _, line := range strings.Split(strings.ReplaceAll(string(data), "\r\n", "\n"), "\n") {
		switch {
		case tomlHeader.MatchString(line):
			table, note = tomlHeader.FindStringSubmatch(line)[1], nil
		case tomlArray.MatchString(line):
			// Keys inside an array of tables are not followed.
			table, note = "\x00", nil
		case tomlKey.MatchString(line):
			path := join(table, tomlKey.FindStringSubmatch(line)[1])
			if value, ok := doc.Get(splitKey(path)...); ok {
				if _, isTable := value.(map[string]any); !isTable {
					add(setEntry(path, value, defaults[path]))
				}
			}
			note = nil
		case tomlNote.MatchString(line):
			text := tomlNote.FindStringSubmatch(line)[1]
			if k, v, ok := commentedTOML(text); ok {
				path := join(table, k)
				defaults[path] = &Entry{Key: schema.Key{
					Key:         path,
					Type:        typeOf(v),
					Default:     strings.TrimSpace(text[strings.Index(text, "=")+1:]),
					Description: strings.Join(note, " "),
				}}
				note = nil
			} else if strings.TrimSpace(text) != "" {
				note = append(note, strings.TrimSpace(text))
			}
		default:
			note = nil
		}
	}
	// Keys written in forms the scan does not follow, such as inline tables.
	for _, path := range leaves(doc.Map(), nil) {
		if value, ok := doc.Get(splitKey(path)...); ok {
			add(setEntry(path, value, defaults[path]))
		}
	}
	// Commented-out defaults, in file order, for keys that are not set.
	for _, line := range orderedDefaults(data) {
		if def := defaults[line]; def != nil && !seen[line] {
			add(def)
		}
	}
	return out, nil
}

func setEntry(path string, value any, def *Entry) *Entry {
	e := &Entry{Key: schema.Key{Key: path, Type: typeOf(value)}, Value: value, Set: true}
	if def != nil {
		e.Key.Default, e.Key.Description = def.Key.Default, def.Key.Description
	}
	if items, ok := value.([]any); ok && len(items) > 0 {
		if _, isTable := items[0].(map[string]any); isTable {
			e.ReadOnly = true
		}
	}
	return e
}

// orderedDefaults returns commented-out key paths in file order.
func orderedDefaults(data []byte) []string {
	var out []string
	table := ""
	for _, line := range strings.Split(strings.ReplaceAll(string(data), "\r\n", "\n"), "\n") {
		if m := tomlHeader.FindStringSubmatch(line); m != nil {
			table = m[1]
		} else if tomlArray.MatchString(line) {
			table = "\x00"
		} else if m := tomlNote.FindStringSubmatch(line); m != nil {
			if k, _, ok := commentedTOML(m[1]); ok {
				out = append(out, join(table, k))
			}
		}
	}
	return out
}

// commentedTOML reads "key = value" from a comment's text.
func commentedTOML(text string) (string, any, bool) {
	m := tomlKey.FindStringSubmatch(text)
	if m == nil {
		return "", nil, false
	}
	var doc map[string]any
	if toml.Unmarshal([]byte(text), &doc) != nil {
		return "", nil, false
	}
	value, ok := lookupPath(doc, splitKey(m[1]))
	if !ok {
		return "", nil, false
	}
	if _, isTable := value.(map[string]any); isTable {
		return "", nil, false
	}
	return m[1], value, true
}

func lookupPath(doc map[string]any, path []string) (any, bool) {
	var cur any = doc
	for _, p := range path {
		m, ok := cur.(map[string]any)
		if !ok {
			return nil, false
		}
		if cur, ok = m[p]; !ok {
			return nil, false
		}
	}
	return cur, true
}

func leaves(table map[string]any, prefix []string) []string {
	names := make([]string, 0, len(table))
	for name := range table {
		names = append(names, name)
	}
	slices.Sort(names)
	var out []string
	for _, name := range names {
		path := append(slices.Clone(prefix), name)
		if sub, ok := table[name].(map[string]any); ok {
			out = append(out, leaves(sub, path)...)
			continue
		}
		out = append(out, strings.Join(path, "."))
	}
	return out
}

func join(table, key string) string {
	if table == "" {
		return key
	}
	return table + "." + key
}

/* ----------------------------------------------------------------- env */

var (
	envSet       = regexp.MustCompile(`^\s*(export\s+)?([A-Za-z_][A-Za-z0-9_]*)=(.*)$`)
	envCommented = regexp.MustCompile(`^\s*#\s*(?:export\s+)?([A-Z_][A-Z0-9_]*)=(.*)$`)
	envNote      = regexp.MustCompile(`^\s*#\s?(.*)$`)
	integerText  = regexp.MustCompile(`^-?[0-9]+$`)
)

// readEnv lists KEY=value lines and commented-out "#KEY=default" lines, with
// the comment lines above each as its description.
func readEnv(data []byte) []*Entry {
	var out []*Entry
	index := map[string]*Entry{}
	var note []string
	for _, line := range strings.Split(strings.ReplaceAll(string(data), "\r\n", "\n"), "\n") {
		switch {
		case envCommented.MatchString(line):
			m := envCommented.FindStringSubmatch(line)
			def := unquote(m[2])
			e := entryFor(index, &out, m[1])
			e.Key.Type = inferEnv(def)
			if encoded, err := config.Encode(typed(e.Key.Type, def)); err == nil {
				e.Key.Default = encoded
			}
			e.Key.Description = strings.Join(note, " ")
			note = nil
		case envSet.MatchString(line):
			m := envSet.FindStringSubmatch(line)
			e := entryFor(index, &out, m[2])
			e.Set, e.Value = true, unquote(m[3])
			if e.Key.Description == "" {
				e.Key.Description = strings.Join(note, " ")
			}
			note = nil
		case envNote.MatchString(line):
			if text := strings.TrimSpace(envNote.FindStringSubmatch(line)[1]); text != "" {
				note = append(note, text)
			}
		default:
			note = nil
		}
	}
	for _, e := range out {
		if e.Key.Type == "" {
			e.Key.Type = inferEnv(e.Value.(string))
		}
		if e.Set {
			e.Value = typed(e.Key.Type, e.Value.(string))
		}
	}
	return out
}

func entryFor(index map[string]*Entry, out *[]*Entry, name string) *Entry {
	if e, ok := index[name]; ok {
		return e
	}
	e := &Entry{Key: schema.Key{Key: name}}
	index[name] = e
	*out = append(*out, e)
	return e
}

func inferEnv(text string) string {
	switch {
	case text == "true" || text == "false":
		return schema.TypeBoolean
	case integerText.MatchString(text):
		return schema.TypeInteger
	}
	return schema.TypeString
}

// typed converts env text to the value its type implies.
func typed(kind, text string) any {
	switch kind {
	case schema.TypeBoolean:
		if b, err := strconv.ParseBool(text); err == nil {
			return b
		}
	case schema.TypeInteger:
		if n, err := strconv.ParseInt(text, 10, 64); err == nil {
			return n
		}
	case schema.TypeFloat:
		if n, err := strconv.ParseFloat(text, 64); err == nil {
			return n
		}
	}
	return text
}

func envString(value any) string {
	switch v := value.(type) {
	case string:
		return v
	case bool:
		return strconv.FormatBool(v)
	case int64:
		return strconv.FormatInt(v, 10)
	case float64:
		return strconv.FormatFloat(v, 'g', -1, 64)
	}
	return fmt.Sprint(value)
}

func unquote(s string) string {
	s = strings.TrimSpace(s)
	if len(s) >= 2 && (s[0] == '"' || s[0] == '\'') && s[len(s)-1] == s[0] {
		return s[1 : len(s)-1]
	}
	return s
}

// setEnv writes KEY=value: in place of an existing line (keeping its
// quotes), else just below the commented-out default, else at the end.
func setEnv(data []byte, key, value string) ([]byte, error) {
	if strings.ContainsAny(value, "\r\n") {
		return nil, errors.New("a value cannot span lines")
	}
	nl := "\n"
	if strings.Contains(string(data), "\r\n") {
		nl = "\r\n"
	}
	lines := strings.Split(strings.ReplaceAll(string(data), "\r\n", "\n"), "\n")
	commented := -1
	for i, line := range lines {
		if m := envSet.FindStringSubmatch(line); m != nil && m[2] == key {
			raw := strings.TrimSpace(m[3])
			quoted := value
			if len(raw) >= 2 && (raw[0] == '"' || raw[0] == '\'') && raw[len(raw)-1] == raw[0] {
				quoted = string(raw[0]) + value + string(raw[0])
			}
			lines[i] = line[:strings.Index(line, "=")+1] + quoted
			return []byte(strings.Join(lines, nl)), nil
		}
		if m := envCommented.FindStringSubmatch(line); m != nil && m[1] == key && commented < 0 {
			commented = i
		}
	}
	entry := key + "=" + value
	if commented >= 0 {
		lines = slices.Insert(lines, commented+1, entry)
		return []byte(strings.Join(lines, nl)), nil
	}
	text := strings.Join(lines, nl)
	if text != "" && !strings.HasSuffix(text, nl) {
		text += nl
	}
	return []byte(text + entry + nl), nil
}

// unsetEnv removes KEY=value lines, leaving commented-out defaults.
func unsetEnv(data []byte, key string) []byte {
	nl := "\n"
	if strings.Contains(string(data), "\r\n") {
		nl = "\r\n"
	}
	lines := strings.Split(strings.ReplaceAll(string(data), "\r\n", "\n"), "\n")
	lines = slices.DeleteFunc(lines, func(line string) bool {
		m := envSet.FindStringSubmatch(line)
		return m != nil && m[2] == key
	})
	return []byte(strings.Join(lines, nl))
}
