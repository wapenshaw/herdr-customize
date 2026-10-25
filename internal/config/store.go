package config

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/wapenshaw/herdr-customize/internal/fsutil"
	"github.com/wapenshaw/herdr-customize/internal/herdr"
)

// Store loads and saves one config file.
type Store struct {
	Path string
	// NoReload skips 'herdr server reload-config' after a save.
	NoReload bool
}

// Default returns the store for this machine's Herdr config.
func Default() *Store { return &Store{Path: Path()} }

// Load parses the config. A missing file is an empty document.
func (s *Store) Load() (*Doc, error) {
	data, err := os.ReadFile(s.Path)
	if errors.Is(err, os.ErrNotExist) {
		return &Doc{}, nil
	}
	if err != nil {
		return nil, err
	}
	doc, err := Parse(data)
	if err != nil {
		return nil, fmt.Errorf("%s: %w", s.Path, err)
	}
	return doc, nil
}

// SaveResult describes what a save did besides writing the file.
type SaveResult struct {
	Changed bool
	// CheckSkipped is set when 'herdr config check' could not run.
	CheckSkipped string
	// ReloadErr is set when the config was saved but Herdr did not reload.
	ReloadErr error
}

// CheckError lists problems Herdr found in an edit. The file is unchanged.
type CheckError struct {
	Issues []string
}

func (e *CheckError) Error() string {
	return "herdr rejected the change: " + strings.Join(e.Issues, "; ")
}

// Update loads the config, applies edit, and saves the result.
func (s *Store) Update(edit func(*Doc) error) (SaveResult, error) {
	doc, err := s.Load()
	if err != nil {
		return SaveResult{}, err
	}
	if err := edit(doc); err != nil {
		return SaveResult{}, err
	}
	return s.Save(doc)
}

// Save validates doc with 'herdr config check' before replacing the file, so a
// rejected edit never reaches disk. Problems the config already had do not
// block a save. The first change each day backs up the original file.
func (s *Store) Save(doc *Doc) (SaveResult, error) {
	var result SaveResult
	data := doc.Bytes()
	original, err := os.ReadFile(s.Path)
	missing := errors.Is(err, os.ErrNotExist)
	if err != nil && !missing {
		return result, err
	}
	if !missing && bytes.Equal(original, data) {
		return result, nil
	}
	if _, err := Parse(data); err != nil {
		return result, err
	}
	dir := filepath.Dir(s.Path)
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return result, err
	}
	issues, skipped, err := s.newIssues(dir, data, missing)
	if err != nil {
		return result, err
	}
	if len(issues) > 0 {
		return result, &CheckError{Issues: issues}
	}
	result.CheckSkipped = skipped
	if !missing {
		if err := fsutil.DailyBackup(s.Path, original); err != nil {
			return result, err
		}
	}
	if err := fsutil.AtomicWrite(s.Path, data, 0o600); err != nil {
		return result, err
	}
	result.Changed = true
	if !s.NoReload {
		result.ReloadErr = herdr.Reload()
	}
	return result, nil
}

// newIssues checks data in a temporary file beside the config, so relative
// paths resolve the same way, and returns issues the current file lacks.
func (s *Store) newIssues(dir string, data []byte, missing bool) ([]string, string, error) {
	f, err := os.CreateTemp(dir, ".herdr-customize-check-*.toml")
	if err != nil {
		return nil, "", err
	}
	defer os.Remove(f.Name())
	_, writeErr := f.Write(data)
	if err := errors.Join(writeErr, f.Close()); err != nil {
		return nil, "", err
	}
	after := herdr.CheckConfig(f.Name())
	if !after.Ran {
		return nil, after.Skipped, nil
	}
	if len(after.Issues) == 0 {
		return nil, "", nil
	}
	known := make(map[string]bool)
	if !missing {
		for _, issue := range herdr.CheckConfig(s.Path).Issues {
			known[normalizeIssue(issue)] = true
		}
	}
	var fresh []string
	for _, issue := range after.Issues {
		if !known[normalizeIssue(issue)] {
			fresh = append(fresh, strings.TrimSpace(issue))
		}
	}
	return fresh, "", nil
}

var digitsRe = regexp.MustCompile(`[0-9]+`)

// normalizeIssue ignores line and column numbers, which an edit can shift.
func normalizeIssue(issue string) string {
	return digitsRe.ReplaceAllString(strings.TrimSpace(issue), "#")
}
