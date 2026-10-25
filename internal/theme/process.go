package theme

import (
	"bytes"
	"fmt"
	"os/exec"
	"runtime"
	"strings"
)

type limitedBuffer struct{ buffer bytes.Buffer }

func (b *limitedBuffer) String() string { return b.buffer.String() }

func (b *limitedBuffer) Write(p []byte) (int, error) {
	if b.buffer.Len()+len(p) > maxPaletteBytes {
		return 0, fmt.Errorf("output exceeds %d bytes", maxPaletteBytes)
	}
	return b.buffer.Write(p)
}

// quotedCommand renders exe and args as one command line for a shell that is
// selected explicitly, so an inherited SHELL cannot change how paths are read.
func quotedCommand(exe string, args ...string) (string, string) {
	shell := "sh -c"
	quote := func(s string) string { return "'" + strings.ReplaceAll(s, "'", "'\\''") + "'" }
	prefix := ""
	if runtime.GOOS == "windows" {
		shell = "powershell.exe -NoProfile -NonInteractive -Command"
		if _, err := exec.LookPath("pwsh.exe"); err == nil {
			shell = "pwsh.exe -NoProfile -NonInteractive -Command"
		}
		quote = func(s string) string { return "'" + strings.ReplaceAll(s, "'", "''") + "'" }
		prefix = "& "
	}
	parts := []string{prefix + quote(exe)}
	for _, arg := range args {
		parts = append(parts, quote(arg))
	}
	return strings.Join(parts, " "), shell
}
