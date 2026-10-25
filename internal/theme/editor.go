package theme

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"unicode"
)

// PickEditor honors explicit preferences before choosing an installed fallback.
func PickEditor() string {
	for _, key := range []string{"VISUAL", "EDITOR"} {
		if value := os.Getenv(key); value != "" {
			return value
		}
	}
	for _, name := range []string{"nvim", "nano"} {
		if path, err := exec.LookPath(name); err == nil {
			return path
		}
	}
	if runtime.GOOS == "windows" {
		return "notepad.exe"
	}
	return "vi"
}

// Parse arguments without evaluating shell operators, expansions or substitutions.
// Backslashes in Windows paths stay literal; quoted executable paths are supported.
func splitEditor(command string) ([]string, error) {
	var args []string
	var word strings.Builder
	var quote rune
	started := false
	runes := []rune(command)
	for i := 0; i < len(runes); i++ {
		r := runes[i]
		if r == '\\' && i+1 < len(runes) && ((quote == '"' && runes[i+1] == '"') || (quote == 0 && unicode.IsSpace(runes[i+1]))) {
			i++
			word.WriteRune(runes[i])
			started = true
			continue
		}
		if quote != 0 {
			if r == quote {
				quote = 0
			} else {
				word.WriteRune(r)
			}
			started = true
			continue
		}
		if r == '\'' || r == '"' {
			quote = r
			started = true
			continue
		}
		if unicode.IsSpace(r) {
			if started {
				args = append(args, word.String())
				word.Reset()
				started = false
			}
			continue
		}
		word.WriteRune(r)
		started = true
	}
	if quote != 0 {
		return nil, fmt.Errorf("unclosed quote in editor command")
	}
	if started {
		args = append(args, word.String())
	}
	if len(args) == 0 || args[0] == "" {
		return nil, fmt.Errorf("editor command is empty")
	}
	return args, nil
}

// EditorCommand opens path in the preferred editor.
func EditorCommand(path string) (*exec.Cmd, error) {
	editor := PickEditor()
	// A preference may be an unquoted executable path containing spaces.
	var args []string
	if resolved, err := exec.LookPath(editor); err == nil {
		args = []string{resolved}
	} else {
		var err error
		args, err = splitEditor(editor)
		if err != nil {
			return nil, err
		}
	}
	executable, err := exec.LookPath(args[0])
	if err != nil {
		return nil, err
	}
	args = append(args[1:], path)
	if runtime.GOOS == "windows" && (strings.EqualFold(filepath.Ext(executable), ".cmd") || strings.EqualFold(filepath.Ext(executable), ".bat")) {
		// Editors such as VS Code install a .cmd shim rather than a PE executable.
		// PowerShell's literal argument quoting avoids cmd.exe expansion of paths.
		command, shell := quotedCommand(executable, args...)
		shellArgs := strings.Fields(shell)
		return exec.Command(shellArgs[0], append(shellArgs[1:], command)...), nil
	}
	return exec.Command(executable, args...), nil
}

// SetOverride records a per-token color for a user theme body.
func SetOverride(body, token, color string) (string, error) {
	if !isToken(token) {
		return "", fmt.Errorf("unknown token: %s", token)
	}
	color, err := normalizeColor(color)
	if err != nil {
		return "", err
	}
	var lines []string
	for _, line := range strings.Split(strings.TrimRight(body, "\r\n"), "\n") {
		m := overrideEntryRe.FindStringSubmatch(strings.TrimSpace(line))
		if len(m) == 3 && m[1] == token {
			continue
		}
		lines = append(lines, line)
	}
	lines = append(lines, fmt.Sprintf("# hpick-override: %s=%s", token, color))
	updated := strings.Join(lines, "\n") + "\n"
	if _, err := ParsePaletteContent(updated); err != nil {
		return "", err
	}
	return updated, nil
}

// TokenRow describes one Herdr token of a palette for the token editor.
type TokenRow struct {
	Token, Color, Origin, Role string
	Overridden                 bool
}

// TokenRows maps body to Herdr tokens in TokenOrder.
func TokenRows(body string) ([]TokenRow, map[string]string, error) {
	pal, err := ParsePaletteContent(body)
	if err != nil {
		return nil, nil, err
	}
	tokens, err := PaletteToTokens(pal)
	if err != nil {
		return nil, nil, err
	}
	rows := make([]TokenRow, 0, len(TokenOrder))
	for _, token := range TokenOrder {
		_, overridden := pal.Overrides[token]
		origin := TokenOrigin(token)
		if overridden {
			origin = "custom override"
		}
		rows = append(rows, TokenRow{Token: token, Color: tokens[token], Origin: origin, Role: TokenRole(token), Overridden: overridden})
	}
	return rows, tokens, nil
}

// UserThemeBody returns a user theme's palette for editing.
func UserThemeBody(slug string) (string, error) {
	dest, err := userThemePath(slug)
	if err != nil {
		return "", err
	}
	if _, err := ParsePaletteFile(dest); err != nil {
		return "", err
	}
	body, err := os.ReadFile(dest)
	return string(body), err
}

// CommitUserTheme replaces a user theme's palette with an edited body.
func CommitUserTheme(slug, body string) error {
	// Recheck ownership before committing; never follow a replaced symlink.
	dest, err := userThemePath(slug)
	if err != nil {
		return err
	}
	if _, err := ParsePaletteContent(body); err != nil {
		return err
	}
	return atomicWriteFile(dest, []byte(body), 0o644)
}
