package theme

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/wapenshaw/herdr-customize/internal/fsutil"
	"github.com/wapenshaw/herdr-customize/internal/instance"
)

// atomicWriteFile keeps readers from observing truncated configs, indexes or palettes.
func atomicWriteFile(path string, data []byte, mode os.FileMode) error {
	return fsutil.AtomicWrite(path, data, mode)
}

// readIndex returns an index file's unique non-blank entries, split into
// accepted slugs and rejected entries.
func readIndex(path string, accept func(string) bool) (slugs, rejected []string, err error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, nil, err
	}
	seen := make(map[string]bool)
	for _, line := range strings.Split(string(data), "\n") {
		slug := strings.TrimSpace(line)
		if slug == "" || seen[slug] {
			continue
		}
		seen[slug] = true
		if accept(slug) {
			slugs = append(slugs, slug)
		} else {
			rejected = append(rejected, slug)
		}
	}
	return slugs, rejected, nil
}

// userThemeFile returns a stored user theme's path. User theme operations
// never follow links outside their storage directory.
func userThemeFile(slug string) (string, error) {
	if !isStoredUserSlug(slug) {
		return "", fmt.Errorf("invalid slug: %q", slug)
	}
	path := filepath.Join(UserThemesDir(), slug)
	info, err := os.Lstat(path)
	if err != nil {
		return "", err
	}
	if !info.Mode().IsRegular() {
		return "", fmt.Errorf("user theme is not a regular file: %s", slug)
	}
	return path, nil
}

// userThemePath is userThemeFile restricted to themes listed in the index.
func userThemePath(slug string) (string, error) {
	path, err := userThemeFile(slug)
	if err != nil {
		return "", err
	}
	slugs, err := readUserIndex()
	if err != nil {
		return "", err
	}
	for _, item := range slugs {
		if item == slug {
			return path, nil
		}
	}
	return "", fmt.Errorf("%q is not a user-added theme", slug)
}

// Invalid legacy entries must not disable the whole picker. Keep their bytes
// on disk for recovery on the original OS, but never resolve them as paths.
func readUserIndex() ([]string, error) {
	slugs, rejected, err := readIndex(UserIndexFile(), isStoredUserSlug)
	if errors.Is(err, os.ErrNotExist) {
		return nil, nil
	}
	for _, slug := range rejected {
		fmt.Fprintf(os.Stderr, "Skipping unsupported theme name %q in %s; its file and index entry are preserved. Rename it on the original system to import it here.\n", slug, UserIndexFile())
	}
	return slugs, err
}

// Preserve unrecognized entries rather than silently deleting legacy data when
// another theme is saved or removed.
func updateUserIndex(slug string, remove bool) error {
	lock, err := instance.Acquire(UserIndexFile() + ".lock")
	if err != nil {
		return fmt.Errorf("lock theme index: %w", err)
	}
	defer lock.Close()
	data, err := os.ReadFile(UserIndexFile())
	if err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	var lines []string
	found := false
	for _, line := range strings.Split(strings.TrimRight(string(data), "\r\n"), "\n") {
		if strings.TrimSpace(line) == slug {
			found = true
			if remove {
				continue
			}
		}
		if line != "" {
			lines = append(lines, line)
		}
	}
	if !remove && !found {
		lines = append(lines, slug)
	}
	return atomicWriteFile(UserIndexFile(), []byte(strings.Join(lines, "\n")+"\n"), 0o644)
}
