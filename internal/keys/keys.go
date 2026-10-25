// Package keys parses Herdr key strings, finds conflicting bindings and
// applies Herdr's documented rules for where a key may be used.
package keys

import (
	"fmt"
	"slices"
	"strings"
	"unicode"
	"unicode/utf8"
)

// Combo is one parsed key such as prefix+shift+t.
type Combo struct {
	Prefix bool
	Mods   []string // canonical order: ctrl, alt, shift, cmd
	Key    string
}

var modOrder = []string{"ctrl", "alt", "shift", "cmd"}

var modAliases = map[string]string{
	"ctrl": "ctrl", "control": "ctrl",
	"alt": "alt", "option": "alt", "opt": "alt", "meta": "alt",
	"shift": "shift",
	"cmd":   "cmd", "command": "cmd", "super": "cmd", "win": "cmd",
}

var keyAliases = map[string]string{
	"escape": "esc", "return": "enter", "spacebar": "space", " ": "space",
	"-": "minus", ",": "comma", ".": "period", "`": "backtick", "&": "ampersand",
	"=": "equal", "/": "slash", ";": "semicolon", "'": "quote", "[": "leftbracket", "]": "rightbracket",
	"\\": "backslash", "pgup": "pageup", "pgdown": "pagedown", "del": "delete", "ins": "insert",
	"arrowup": "up", "arrowdown": "down", "arrowleft": "left", "arrowright": "right",
}

var namedKeys = []string{
	"enter", "tab", "esc", "space", "backspace", "delete", "insert", "home", "end",
	"pageup", "pagedown", "up", "down", "left", "right",
	"minus", "plus", "comma", "period", "backtick", "ampersand", "equal", "slash",
	"semicolon", "quote", "leftbracket", "rightbracket", "backslash",
	"question", "exclamation", "at", "hash", "dollar", "percent", "caret", "asterisk",
	"underscore", "pipe", "tilde", "colon", "lessthan", "greaterthan",
}

// Parse reads one key string. Ranges such as prefix+1..9 must be expanded
// with Expand first.
func Parse(s string) (Combo, error) {
	s = strings.TrimSpace(s)
	if s == "" {
		return Combo{}, fmt.Errorf("empty key")
	}
	parts := strings.Split(s, "+")
	// A trailing "+" names the plus key itself, as in ctrl++.
	if strings.HasSuffix(s, "++") || s == "+" {
		parts = append(strings.Split(strings.TrimSuffix(s, "++"), "+"), "plus")
		if s == "+" {
			parts = []string{"plus"}
		}
	}
	var c Combo
	for i, part := range parts {
		last := i == len(parts)-1
		lower := strings.ToLower(strings.TrimSpace(part))
		if !last {
			if lower == "prefix" && i == 0 {
				c.Prefix = true
				continue
			}
			mod, ok := modAliases[lower]
			if !ok {
				return Combo{}, fmt.Errorf("unknown modifier %q in %q", part, s)
			}
			if !slices.Contains(c.Mods, mod) {
				c.Mods = append(c.Mods, mod)
			}
			continue
		}
		key, err := canonicalKey(strings.TrimSpace(part))
		if err != nil {
			return Combo{}, fmt.Errorf("%w in %q", err, s)
		}
		// An uppercase letter is the shifted letter.
		if r, _ := utf8.DecodeRuneInString(key); utf8.RuneCountInString(key) == 1 && unicode.IsUpper(r) {
			key = strings.ToLower(key)
			if !slices.Contains(c.Mods, "shift") {
				c.Mods = append(c.Mods, "shift")
			}
		}
		c.Key = key
	}
	slices.SortFunc(c.Mods, func(a, b string) int { return slices.Index(modOrder, a) - slices.Index(modOrder, b) })
	return c, nil
}

func canonicalKey(key string) (string, error) {
	if key == "" {
		return "", fmt.Errorf("missing key")
	}
	if utf8.RuneCountInString(key) == 1 {
		if alias, ok := keyAliases[key]; ok {
			return alias, nil
		}
		return key, nil
	}
	lower := strings.ToLower(key)
	if alias, ok := keyAliases[lower]; ok {
		return alias, nil
	}
	if slices.Contains(namedKeys, lower) {
		return lower, nil
	}
	if len(lower) >= 2 && lower[0] == 'f' {
		var n int
		if _, err := fmt.Sscanf(lower[1:], "%d", &n); err == nil && n >= 1 && n <= 24 && fmt.Sprint(n) == lower[1:] {
			return lower, nil
		}
	}
	return "", fmt.Errorf("unknown key %q", key)
}

// String renders the canonical form.
func (c Combo) String() string {
	var parts []string
	if c.Prefix {
		parts = append(parts, "prefix")
	}
	parts = append(parts, c.Mods...)
	return strings.Join(append(parts, c.Key), "+")
}

// Plain reports a printable key with no prefix or modifier, which would
// intercept typing in panes.
func (c Combo) Plain() bool {
	if c.Prefix || len(c.Mods) > 0 {
		return false
	}
	return utf8.RuneCountInString(c.Key) == 1 || slices.Contains([]string{"space", "minus", "comma", "period", "slash"}, c.Key)
}

// Expand turns a binding with a 1..9 range into one key per digit.
func Expand(s string) []string {
	if i := strings.Index(s, "1..9"); i >= 0 {
		out := make([]string, 0, 9)
		for d := '1'; d <= '9'; d++ {
			out = append(out, s[:i]+string(d)+s[i+4:])
		}
		return out
	}
	return []string{s}
}

// Values reads a binding value: a string, an array of strings, or nothing.
func Values(value any) []string {
	switch v := value.(type) {
	case string:
		if v == "" {
			return nil
		}
		return []string{v}
	case []any:
		var out []string
		for _, item := range v {
			if s, ok := item.(string); ok && s != "" {
				out = append(out, s)
			}
		}
		return out
	case []string:
		return v
	}
	return nil
}

// Navigate reports a navigate-mode action, which Herdr checks separately
// from the rest of the bindings.
func Navigate(action string) bool {
	return strings.HasPrefix(action, "navigate_")
}

// Problems returns Herdr's documented rules that value breaks for action
// (the name under [keys], such as "zoom").
func Problems(action, value string) []string {
	var problems []string
	for _, key := range Expand(value) {
		c, err := Parse(key)
		if err != nil {
			problems = append(problems, err.Error())
			continue
		}
		switch {
		case action == "prefix":
			if c.Prefix {
				problems = append(problems, "the prefix key cannot itself use prefix+")
			}
		case Navigate(action):
			if c.Prefix {
				problems = append(problems, "navigate-mode keys cannot use prefix+")
			}
			if len(c.Mods) == 0 && slices.Contains([]string{"esc", "enter", "tab", "left", "right", "1", "2", "3", "4", "5", "6", "7", "8", "9"}, c.Key) {
				problems = append(problems, fmt.Sprintf("navigate mode reserves %q", c.Key))
			}
			if c.Key == "tab" && slices.Equal(c.Mods, []string{"shift"}) {
				problems = append(problems, "navigate mode reserves shift+tab")
			}
		case c.Plain():
			problems = append(problems, fmt.Sprintf("%q without prefix+ or a modifier intercepts typing in panes", key))
		}
	}
	return problems
}

// Binding is one thing that owns keys.
type Binding struct {
	Owner string
	Keys  []string
	// Navigate bindings only apply in navigate mode.
	Navigate bool
}

func modeKey(navigate bool, c Combo) string {
	if navigate {
		return "navigate:" + c.String()
	}
	return c.String()
}

// Conflicts maps each key used by more than one binding in the same mode to
// the owners that use it. Navigate-mode keys are prefixed with "navigate:".
func Conflicts(bindings []Binding) map[string][]string {
	owners := make(map[string][]string)
	for _, b := range bindings {
		for _, value := range b.Keys {
			for _, key := range Expand(value) {
				c, err := Parse(key)
				if err != nil {
					continue
				}
				id := modeKey(b.Navigate, c)
				if !slices.Contains(owners[id], b.Owner) {
					owners[id] = append(owners[id], b.Owner)
				}
			}
		}
	}
	for id, list := range owners {
		if len(list) < 2 {
			delete(owners, id)
		}
	}
	return owners
}

// Clashes returns the other owners that share a key with b.
func Clashes(conflicts map[string][]string, b Binding) []string {
	var out []string
	for _, value := range b.Keys {
		for _, key := range Expand(value) {
			c, err := Parse(key)
			if err != nil {
				continue
			}
			for _, other := range conflicts[modeKey(b.Navigate, c)] {
				if other != b.Owner && !slices.Contains(out, other) {
					out = append(out, other)
				}
			}
		}
	}
	return out
}
