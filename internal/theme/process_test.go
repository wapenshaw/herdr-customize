package theme

import (
	"encoding/json"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"runtime"
	"strings"
	"testing"
)

func TestOutputBound(t *testing.T) {
	var b limitedBuffer
	if _, err := io.Copy(&b, strings.NewReader(strings.Repeat("x", maxPaletteBytes+1))); err == nil {
		t.Fatal("unbounded subprocess output accepted")
	}
}

func TestQuotedCommandExecutesQuotedPaths(t *testing.T) {
	suffix := ""
	if runtime.GOOS == "windows" {
		suffix = ".exe"
	}
	exe := filepath.Join(t.TempDir(), "preview 'quoted' $ path"+suffix)
	copyTestExecutable(t, exe)
	path := filepath.Join(t.TempDir(), "scratch 'quoted' $ file")
	command, shell := quotedCommand(exe, "preview-file", path, "picked row")
	args := strings.Fields(shell)
	cmd := exec.Command(args[0], append(args[1:], command)...)
	cmd.Env = append(os.Environ(), "THEME_TEST_ARGV=1")
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("command did not execute: %v %s", err, out)
	}
	var got []string
	if err := json.Unmarshal(out, &got); err != nil {
		t.Fatal(err, string(out))
	}
	want := []string{"preview-file", path, "picked row"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("quoting changed arguments: got %q, want %q", got, want)
	}
}
