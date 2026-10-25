package theme

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestResolvePaletteLocalAndCache(t *testing.T) {
	t.Setenv("HERDR_PLUGIN_STATE_DIR", t.TempDir())
	valid := fixturePalette(t)

	// 1. Resolves cached theme if valid
	cache := filepath.Join(CacheDir(), "cached-theme")
	writeTestFile(t, cache, valid)
	path, err := ResolvePalette("cached-theme")
	if err != nil || path != cache {
		t.Fatalf("failed to resolve cached theme: path=%s err=%v", path, err)
	}

	// 2. Corrupt cache falls through to not found with helpful link
	writeTestFile(t, cache, "invalid content\n")
	_, err = ResolvePalette("cached-theme")
	if err == nil || !strings.Contains(err.Error(), "terminalcolors.com") {
		t.Fatalf("expected error mentioning terminalcolors.com, got %v", err)
	}

	// 3. Unknown slug returns error mentioning terminalcolors.com
	_, err = ResolvePalette("nonexistent-theme")
	if err == nil || !strings.Contains(err.Error(), "terminalcolors.com") {
		t.Fatalf("expected error mentioning terminalcolors.com, got %v", err)
	}
}
