// Package config edits Herdr's config.toml in place. Every edit touches only
// the bytes of the key or array-table entry it changes, so comments, ordering,
// spacing and line endings elsewhere in the file survive.
package config

import (
	"bytes"
	"errors"
	"fmt"
	"reflect"
	"slices"
	"strings"

	"github.com/pelletier/go-toml/v2"
	"github.com/pelletier/go-toml/v2/unstable"
)

// ErrInline reports a value defined inside an inline table or inline array of
// tables, which cannot be edited without rewriting the user's formatting.
var ErrInline = errors.New("defined inline; edit it in the config file")

// Doc is a parsed config.toml that can be edited without losing formatting.
type Doc struct {
	data []byte
}

// Field is one key of an array-table entry, in output order.
type Field struct {
	Key   string
	Value any
}

// Parse validates data as TOML and returns an editable document.
func Parse(data []byte) (*Doc, error) {
	var document map[string]any
	if err := toml.Unmarshal(data, &document); err != nil {
		return nil, fmt.Errorf("invalid TOML: %w", err)
	}
	return &Doc{data: slices.Clone(data)}, nil
}

// Bytes returns the current document.
func (d *Doc) Bytes() []byte { return slices.Clone(d.data) }

// Map decodes the document.
func (d *Doc) Map() map[string]any {
	document := map[string]any{}
	_ = toml.Unmarshal(d.data, &document)
	return document
}

// Get returns the decoded value at path. Integers decode as int64, floats as
// float64, arrays as []any and tables as map[string]any.
func (d *Doc) Get(path ...string) (any, bool) {
	return lookup(d.Map(), path)
}

func lookup(document map[string]any, path []string) (any, bool) {
	var current any = document
	for _, key := range path {
		table, ok := current.(map[string]any)
		if !ok {
			return nil, false
		}
		if current, ok = table[key]; !ok {
			return nil, false
		}
	}
	return current, true
}

// Set writes value at path, replacing an existing value in place or inserting
// a new key into the closest existing table.
func (d *Doc) Set(path []string, value any) error {
	if len(path) == 0 {
		return fmt.Errorf("empty key path")
	}
	encoded, err := Encode(value)
	if err != nil {
		return err
	}
	want, err := decodeEncoded(encoded)
	if err != nil {
		return err
	}
	idx, err := d.index()
	if err != nil {
		return err
	}
	nl := d.newline()
	var candidates [][]byte
	if kv := idx.find(path); kv != nil {
		candidates = append(candidates, splice(d.data, kv.valStart, kv.valEnd, encoded))
	} else {
		if idx.inlineAncestor(path) {
			return fmt.Errorf("%s: %w", strings.Join(path, "."), ErrInline)
		}
		candidates = idx.insertions(d.data, path, encoded, nl)
	}
	for _, candidate := range candidates {
		var document map[string]any
		if toml.Unmarshal(candidate, &document) != nil {
			continue
		}
		if got, ok := lookup(document, path); ok && reflect.DeepEqual(got, want) {
			d.data = candidate
			return nil
		}
	}
	return fmt.Errorf("cannot place %s in this config; edit it in the config file", strings.Join(path, "."))
}

// Unset removes the key at path. It reports false when the key is absent.
func (d *Doc) Unset(path []string) (bool, error) {
	idx, err := d.index()
	if err != nil {
		return false, err
	}
	kv := idx.find(path)
	if kv == nil {
		if idx.inlineAncestor(path) {
			return false, fmt.Errorf("%s: %w", strings.Join(path, "."), ErrInline)
		}
		return false, nil
	}
	candidate := splice(d.data, kv.start, kv.end, "")
	var document map[string]any
	if err := toml.Unmarshal(candidate, &document); err != nil {
		return false, fmt.Errorf("removing %s breaks the config: %w", strings.Join(path, "."), err)
	}
	d.data = candidate
	return true, nil
}

// ArrayTable returns the decoded entries of an array of tables such as
// keys.command, whether written as [[headers]] or inline.
func (d *Doc) ArrayTable(path []string) []map[string]any {
	value, ok := d.Get(path...)
	if !ok {
		return nil
	}
	var entries []map[string]any
	switch list := value.(type) {
	case []any:
		for _, item := range list {
			if entry, ok := item.(map[string]any); ok {
				entries = append(entries, entry)
			}
		}
	case []map[string]any:
		entries = list
	}
	return entries
}

// AppendArrayTable adds a [[path]] entry after the existing ones.
func (d *Doc) AppendArrayTable(path []string, fields []Field) error {
	idx, err := d.index()
	if err != nil {
		return err
	}
	elements, err := idx.arrayElements(d, path)
	if err != nil {
		return err
	}
	nl := d.newline()
	var block strings.Builder
	block.WriteString(nl + "[[" + RenderKey(path) + "]]" + nl)
	for _, field := range fields {
		encoded, err := Encode(field.Value)
		if err != nil {
			return err
		}
		block.WriteString(RenderKey([]string{field.Key}) + " = " + encoded + nl)
	}
	pos := len(d.data)
	if len(elements) > 0 {
		pos = idx.contentEnd(elements[len(elements)-1])
	}
	candidate := insertBlock(d.data, pos, block.String(), nl)
	return d.commitArray(candidate, path, len(elements), len(elements)+1, fields)
}

// UpdateArrayTable rewrites entry i of [[path]] to exactly fields. Existing
// keys keep their position and trailing comments; missing keys are removed.
func (d *Doc) UpdateArrayTable(path []string, i int, fields []Field) error {
	before := len(d.ArrayTable(path))
	working := &Doc{data: slices.Clone(d.data)}
	keep := make(map[string]bool)
	for _, field := range fields {
		keep[field.Key] = true
		encoded, err := Encode(field.Value)
		if err != nil {
			return err
		}
		idx, err := working.index()
		if err != nil {
			return err
		}
		elements, err := idx.arrayElements(working, path)
		if err != nil {
			return err
		}
		if i < 0 || i >= len(elements) {
			return fmt.Errorf("%s entry %d does not exist", RenderKey(path), i+1)
		}
		h := elements[i]
		if kv := idx.findIn(h, []string{field.Key}); kv != nil {
			working.data = splice(working.data, kv.valStart, kv.valEnd, encoded)
		} else {
			line := RenderKey([]string{field.Key}) + " = " + encoded + working.newline()
			working.data = insertLine(working.data, idx.contentEnd(h), line, working.newline())
		}
	}
	for {
		idx, err := working.index()
		if err != nil {
			return err
		}
		elements, err := idx.arrayElements(working, path)
		if err != nil {
			return err
		}
		var stale *keyValue
		for _, kv := range idx.kvs {
			if kv.header == elements[i] && len(kv.key) == 1 && !keep[kv.key[0]] {
				stale = kv
				break
			}
		}
		if stale == nil {
			break
		}
		working.data = splice(working.data, stale.start, stale.end, "")
	}
	return d.commitArray(working.data, path, i, before, fields)
}

// DeleteArrayTable removes entry i of [[path]] including its header.
func (d *Doc) DeleteArrayTable(path []string, i int) error {
	idx, err := d.index()
	if err != nil {
		return err
	}
	elements, err := idx.arrayElements(d, path)
	if err != nil {
		return err
	}
	if i < 0 || i >= len(elements) {
		return fmt.Errorf("%s entry %d does not exist", RenderKey(path), i+1)
	}
	h := elements[i]
	end := idx.contentEnd(h)
	// Take the blank lines that separated this entry from the next one.
	for end < len(d.data) {
		next := lineEnd(d.data, end)
		if strings.TrimSpace(string(d.data[end:next])) != "" {
			break
		}
		end = next
	}
	candidate := splice(d.data, h.start, end, "")
	var document map[string]any
	if err := toml.Unmarshal(candidate, &document); err != nil {
		return fmt.Errorf("removing %s entry breaks the config: %w", RenderKey(path), err)
	}
	if got := len((&Doc{data: candidate}).ArrayTable(path)); got != len(elements)-1 {
		return fmt.Errorf("could not remove %s entry %d", RenderKey(path), i+1)
	}
	d.data = candidate
	return nil
}

// commitArray accepts candidate when entry i of path now decodes to fields
// and the array has the expected length.
func (d *Doc) commitArray(candidate []byte, path []string, i, length int, fields []Field) error {
	next, err := Parse(candidate)
	if err != nil {
		return fmt.Errorf("edit breaks the config: %w", err)
	}
	entries := next.ArrayTable(path)
	if len(entries) != length || i >= len(entries) {
		return fmt.Errorf("could not update %s", RenderKey(path))
	}
	want := make(map[string]any, len(fields))
	for _, field := range fields {
		encoded, err := Encode(field.Value)
		if err != nil {
			return err
		}
		if want[field.Key], err = decodeEncoded(encoded); err != nil {
			return err
		}
	}
	if !reflect.DeepEqual(entries[i], want) {
		return fmt.Errorf("could not update %s entry %d", RenderKey(path), i+1)
	}
	d.data = candidate
	return nil
}

func (d *Doc) newline() string {
	if bytes.Contains(d.data, []byte("\r\n")) {
		return "\r\n"
	}
	return "\n"
}

// header is a [table] or [[array table]] line and the body that follows it.
type header struct {
	path      []string
	array     bool
	start     int // first byte of the header line
	bodyStart int // first byte after the header line
}

// keyValue is one key = value expression and the full lines it occupies.
type keyValue struct {
	header           *header // nil at the root
	key              []string
	path             []string
	start, end       int // whole lines, including the trailing newline
	valStart, valEnd int
	inline           bool // the value is an inline table or array of them
}

type index struct {
	headers []*header
	kvs     []*keyValue
}

func (d *Doc) index() (*index, error) {
	var parser unstable.Parser
	parser.Reset(d.data)
	idx := &index{}
	var current *header
	for parser.NextExpression() {
		node := parser.Expression()
		var key []string
		keyStart, keyEnd := -1, -1
		it := node.Key()
		for it.Next() {
			part := it.Node()
			if keyStart < 0 {
				keyStart = int(part.Raw.Offset)
			}
			keyEnd = int(part.Raw.Offset + part.Raw.Length)
			key = append(key, string(part.Data))
		}
		switch node.Kind {
		case unstable.Table, unstable.ArrayTable:
			start := lineStart(d.data, keyStart)
			current = &header{path: key, array: node.Kind == unstable.ArrayTable, start: start, bodyStart: lineEnd(d.data, keyEnd)}
			idx.headers = append(idx.headers, current)
		case unstable.KeyValue:
			valEnd := int(node.Raw.Offset + node.Raw.Length)
			eq := bytes.IndexByte(d.data[keyEnd:valEnd], '=')
			if eq < 0 {
				return nil, fmt.Errorf("malformed key/value at byte %d", keyStart)
			}
			valStart := keyEnd + eq + 1
			for valStart < valEnd && (d.data[valStart] == ' ' || d.data[valStart] == '\t') {
				valStart++
			}
			kv := &keyValue{header: current, key: key, start: lineStart(d.data, keyStart), end: lineEnd(d.data, valEnd), valStart: valStart, valEnd: valEnd}
			if current != nil {
				kv.path = append(slices.Clone(current.path), key...)
			} else {
				kv.path = key
			}
			if value := node.Value(); value != nil {
				kv.inline = value.Kind == unstable.InlineTable
				if value.Kind == unstable.Array {
					child := value.Children()
					for child.Next() {
						if child.Node().Kind == unstable.InlineTable {
							kv.inline = true
							break
						}
					}
				}
			}
			idx.kvs = append(idx.kvs, kv)
		}
	}
	if err := parser.Error(); err != nil {
		return nil, err
	}
	return idx, nil
}

// find returns the key at path outside array tables.
func (idx *index) find(path []string) *keyValue {
	for _, kv := range idx.kvs {
		if (kv.header == nil || !kv.header.array) && slices.Equal(kv.path, path) {
			return kv
		}
	}
	return nil
}

func (idx *index) findIn(h *header, key []string) *keyValue {
	for _, kv := range idx.kvs {
		if kv.header == h && slices.Equal(kv.key, key) {
			return kv
		}
	}
	return nil
}

// inlineAncestor reports whether a prefix of path is an inline value.
func (idx *index) inlineAncestor(path []string) bool {
	for _, kv := range idx.kvs {
		if (kv.header == nil || !kv.header.array) && len(kv.path) < len(path) && slices.Equal(kv.path, path[:len(kv.path)]) {
			return true
		}
	}
	return false
}

// contentEnd is the end of the last key in h's body, or of the header line.
func (idx *index) contentEnd(h *header) int {
	end := h.bodyStart
	for _, kv := range idx.kvs {
		if kv.header == h && kv.end > end {
			end = kv.end
		}
	}
	return end
}

func (idx *index) arrayElements(d *Doc, path []string) ([]*header, error) {
	var elements []*header
	for _, h := range idx.headers {
		if h.array && slices.Equal(h.path, path) {
			elements = append(elements, h)
		}
	}
	if len(elements) != len(d.ArrayTable(path)) {
		return nil, fmt.Errorf("%s: %w", RenderKey(path), ErrInline)
	}
	return elements, nil
}

// insertions lists ways to add path, most conventional first. Set keeps the
// first one that parses and decodes to the intended value.
func (idx *index) insertions(data []byte, path []string, encoded, nl string) [][]byte {
	var out [][]byte
	// 1. Under the deepest existing [table] that contains path.
	var parents []*header
	for _, h := range idx.headers {
		if !h.array && len(h.path) < len(path) && slices.Equal(h.path, path[:len(h.path)]) {
			parents = append(parents, h)
		}
	}
	slices.SortStableFunc(parents, func(a, b *header) int { return len(b.path) - len(a.path) })
	for _, h := range parents {
		pos := idx.contentEnd(h)
		indent := ""
		for _, kv := range idx.kvs {
			if kv.header == h {
				indent = leadingSpace(data[kv.start:kv.end])
			}
		}
		line := indent + RenderKey(path[len(h.path):]) + " = " + encoded + nl
		out = append(out, insertLine(data, pos, line, nl))
	}
	// 2. A new [parent] table beside its closest relative.
	if len(path) > 1 {
		parent := path[:len(path)-1]
		block := nl + "[" + RenderKey(parent) + "]" + nl + RenderKey(path[len(path)-1:]) + " = " + encoded + nl
		pos, best := len(data), 0
		for _, h := range idx.headers {
			if n := commonPrefix(h.path, parent); n > 0 && n >= best {
				best, pos = n, idx.contentEnd(h)
			}
		}
		out = append(out, insertBlock(data, pos, block, nl))
	}
	// 3. A dotted key at the root, before the first table.
	pos := len(data)
	rootEnd := -1
	for _, kv := range idx.kvs {
		if kv.header == nil {
			rootEnd = kv.end
		}
	}
	line := RenderKey(path) + " = " + encoded + nl
	switch {
	case rootEnd >= 0:
		out = append(out, insertLine(data, rootEnd, line, nl))
	case len(idx.headers) > 0:
		pos = idx.headers[0].start
		out = append(out, splice(data, pos, pos, line+nl))
	default:
		out = append(out, insertLine(data, pos, line, nl))
	}
	return out
}

func commonPrefix(a, b []string) int {
	n := 0
	for n < len(a) && n < len(b) && a[n] == b[n] {
		n++
	}
	return n
}

func leadingSpace(line []byte) string {
	n := 0
	for n < len(line) && (line[n] == ' ' || line[n] == '\t') {
		n++
	}
	return string(line[:n])
}

func lineStart(data []byte, pos int) int {
	return bytes.LastIndexByte(data[:pos], '\n') + 1
}

// lineEnd returns the offset just past the newline ending the line at pos.
func lineEnd(data []byte, pos int) int {
	if n := bytes.IndexByte(data[pos:], '\n'); n >= 0 {
		return pos + n + 1
	}
	return len(data)
}

func splice(data []byte, start, end int, text string) []byte {
	out := make([]byte, 0, len(data)-(end-start)+len(text))
	out = append(out, data[:start]...)
	out = append(out, text...)
	return append(out, data[end:]...)
}

// insertLine inserts a complete line at pos, which must be a line start or
// the end of a file that may lack its final newline.
func insertLine(data []byte, pos int, line, nl string) []byte {
	if pos > 0 && data[pos-1] != '\n' {
		line = nl + line
	}
	return splice(data, pos, pos, line)
}

// insertBlock inserts a block that starts with a newline, keeping one blank
// line between it and the text before it.
func insertBlock(data []byte, pos int, block, nl string) []byte {
	if pos == 0 {
		block = strings.TrimPrefix(block, nl)
	} else if data[pos-1] != '\n' {
		block = nl + block
	} else if pos >= 2 && data[pos-2] == '\n' || pos >= 3 && string(data[pos-3:pos]) == "\n\r\n" {
		block = strings.TrimPrefix(block, nl)
	}
	if pos < len(data) && !bytes.HasPrefix(data[pos:], []byte(nl)) {
		block += nl
	}
	return splice(data, pos, pos, block)
}
