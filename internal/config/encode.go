package config

import (
	"fmt"
	"math"
	"regexp"
	"slices"
	"strconv"
	"strings"

	"github.com/pelletier/go-toml/v2"
)

var bareKeyRe = regexp.MustCompile(`^[A-Za-z0-9_-]+$`)

// RenderKey renders a dotted key, quoting parts that are not bare keys.
func RenderKey(path []string) string {
	parts := make([]string, len(path))
	for i, part := range path {
		if bareKeyRe.MatchString(part) {
			parts[i] = part
		} else {
			parts[i] = quote(part)
		}
	}
	return strings.Join(parts, ".")
}

// Encode renders value as a TOML value. Tables become inline tables with
// sorted keys; []Field keeps its order.
func Encode(value any) (string, error) {
	switch v := value.(type) {
	case string:
		return quote(v), nil
	case bool:
		return strconv.FormatBool(v), nil
	case int:
		return strconv.Itoa(v), nil
	case int64:
		return strconv.FormatInt(v, 10), nil
	case float64:
		if math.IsInf(v, 0) || math.IsNaN(v) {
			return "", fmt.Errorf("unsupported number %v", v)
		}
		if v == math.Trunc(v) && math.Abs(v) < 1e15 {
			return strconv.FormatFloat(v, 'f', 1, 64), nil
		}
		return strconv.FormatFloat(v, 'g', -1, 64), nil
	case []string:
		items := make([]any, len(v))
		for i, item := range v {
			items[i] = item
		}
		return Encode(items)
	case []any:
		parts := make([]string, len(v))
		for i, item := range v {
			encoded, err := Encode(item)
			if err != nil {
				return "", err
			}
			parts[i] = encoded
		}
		return "[" + strings.Join(parts, ", ") + "]", nil
	case map[string]any:
		keys := make([]string, 0, len(v))
		for key := range v {
			keys = append(keys, key)
		}
		slices.Sort(keys)
		fields := make([]Field, len(keys))
		for i, key := range keys {
			fields[i] = Field{Key: key, Value: v[key]}
		}
		return Encode(fields)
	case []Field:
		parts := make([]string, len(v))
		for i, field := range v {
			encoded, err := Encode(field.Value)
			if err != nil {
				return "", err
			}
			parts[i] = RenderKey([]string{field.Key}) + " = " + encoded
		}
		if len(parts) == 0 {
			return "{}", nil
		}
		return "{ " + strings.Join(parts, ", ") + " }", nil
	}
	return "", fmt.Errorf("unsupported value type %T", value)
}

// quote renders a TOML basic string.
func quote(s string) string {
	var b strings.Builder
	b.WriteByte('"')
	for _, r := range s {
		switch r {
		case '"':
			b.WriteString(`\"`)
		case '\\':
			b.WriteString(`\\`)
		case '\b':
			b.WriteString(`\b`)
		case '\t':
			b.WriteString(`\t`)
		case '\n':
			b.WriteString(`\n`)
		case '\f':
			b.WriteString(`\f`)
		case '\r':
			b.WriteString(`\r`)
		default:
			if r < 0x20 || r == 0x7f {
				fmt.Fprintf(&b, `\u%04X`, r)
			} else {
				b.WriteRune(r)
			}
		}
	}
	b.WriteByte('"')
	return b.String()
}

// decodeEncoded returns how TOML decodes an encoded value.
func decodeEncoded(encoded string) (any, error) {
	var document map[string]any
	if err := toml.Unmarshal([]byte("v = "+encoded), &document); err != nil {
		return nil, fmt.Errorf("invalid value %s: %w", encoded, err)
	}
	return document["v"], nil
}
