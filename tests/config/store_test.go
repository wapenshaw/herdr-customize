package config_test

import (
	"errors"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/wapenshaw/herdr-customize/internal/config"
	"github.com/wapenshaw/herdr-customize/tests/testutil"
)

const issuesFound = "config: issues found\n"

func store(t *testing.T, body string) *config.Store {
	t.Helper()
	path := filepath.Join(t.TempDir(), "herdr", "config.toml")
	if body != "" {
		testutil.WriteFile(t, path, body)
	}
	return &config.Store{Path: path}
}

func read(t *testing.T, path string) string {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}

func TestSaveChecksBacksUpAndReloads(t *testing.T) {
	log := testutil.FakeHerdr(t)
	s := store(t, "# mine\n[ui]\nmouse_capture = true\n")
	result, err := s.Update(func(doc *config.Doc) error { return doc.Set([]string{"ui", "pane_gaps"}, false) })
	if err != nil || !result.Changed || result.ReloadErr != nil || result.CheckSkipped != "" {
		t.Fatalf("result=%+v err=%v", result, err)
	}
	if got := read(t, s.Path); got != "# mine\n[ui]\nmouse_capture = true\npane_gaps = false\n" {
		t.Fatalf("saved:\n%s", got)
	}
	backup := s.Path + ".bak-" + time.Now().Format("20060102")
	if got := read(t, backup); got != "# mine\n[ui]\nmouse_capture = true\n" {
		t.Fatalf("backup:\n%s", got)
	}
	if calls := testutil.HerdrCalls(t, log); !slices.Equal(calls, []string{"config check", "server reload-config"}) {
		t.Fatalf("calls: %q", calls)
	}
	entries, _ := os.ReadDir(filepath.Dir(s.Path))
	for _, entry := range entries {
		if strings.HasPrefix(entry.Name(), ".herdr-customize") {
			t.Fatalf("temporary file left behind: %s", entry.Name())
		}
	}
}

func TestRejectedEditNeverReachesDisk(t *testing.T) {
	testutil.FakeHerdr(t, testutil.HerdrResponse{
		Args: "config check", ConfigContains: "prefix+zz", Exit: 1,
		Stdout: issuesFound + "invalid keybinding: keys.zoom = \"prefix+zz\"; disabling binding\n",
	})
	s := store(t, "[keys]\nzoom = \"prefix+z\"\n")
	_, err := s.Update(func(doc *config.Doc) error { return doc.Set([]string{"keys", "zoom"}, "prefix+zz") })
	var check *config.CheckError
	if !errors.As(err, &check) || len(check.Issues) != 1 || !strings.Contains(check.Issues[0], "keys.zoom") {
		t.Fatalf("expected check error, got %v", err)
	}
	if got := read(t, s.Path); got != "[keys]\nzoom = \"prefix+z\"\n" {
		t.Fatalf("config changed:\n%s", got)
	}
}

func TestExistingIssuesDoNotBlockSaves(t *testing.T) {
	testutil.FakeHerdr(t, testutil.HerdrResponse{
		Args: "config check", ConfigContains: "bogus", Exit: 1,
		Stdout: issuesFound + "unknown config key ui.bogus; ignoring key\n",
	})
	s := store(t, "[ui]\nbogus = 1\n")
	if _, err := s.Update(func(doc *config.Doc) error { return doc.Set([]string{"ui", "pane_gaps"}, true) }); err != nil {
		t.Fatal(err)
	}
	if got := read(t, s.Path); got != "[ui]\nbogus = 1\npane_gaps = true\n" {
		t.Fatalf("saved:\n%s", got)
	}
}

func TestSaveWithoutHerdr(t *testing.T) {
	s := store(t, "")
	result, err := s.Update(func(doc *config.Doc) error { return doc.Set([]string{"theme", "name"}, "nord") })
	if err != nil {
		t.Fatal(err)
	}
	if result.CheckSkipped == "" || result.ReloadErr == nil {
		t.Fatalf("missing herdr not reported: %+v", result)
	}
	if got := read(t, s.Path); got != "[theme]\nname = \"nord\"\n" {
		t.Fatalf("saved:\n%s", got)
	}
}

func TestUnchangedSaveDoesNothing(t *testing.T) {
	log := testutil.FakeHerdr(t)
	s := store(t, "[ui]\npane_gaps = true\n")
	s.NoReload = true
	result, err := s.Update(func(doc *config.Doc) error { return doc.Set([]string{"ui", "pane_gaps"}, true) })
	if err != nil || result.Changed {
		t.Fatalf("result=%+v err=%v", result, err)
	}
	if calls := testutil.HerdrCalls(t, log); len(calls) != 0 {
		t.Fatalf("unexpected calls: %q", calls)
	}
}

func TestLoadRejectsInvalidTOML(t *testing.T) {
	s := store(t, "[ui\n")
	if _, err := s.Load(); err == nil {
		t.Fatal("invalid config loaded")
	}
}
