package theme

import (
	"bufio"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"

	"github.com/wapenshaw/herdr-customize/internal/terminal"
)

// NewThemeTemplate is the starting text for a theme typed or pasted in an editor.
const NewThemeTemplate = `# Paste a ghostty-format theme below, then save and close this editor.
# Required: background, foreground, and palette 0..15 as #rrggbb colors.
# Get one from https://terminalcolors.com → Download → Ghostty.
#
# background = #1e1e2e
# foreground = #cdd6f4
# cursor-color = #f5e0dc
# palette = 0=#45475a
# palette = 1=#f38ba8
# ... through ...
# palette = 15=#a6adc8
`

// ErrThemeExists reports a user theme that SaveUserTheme would overwrite.
var ErrThemeExists = errors.New("theme already exists")

// Item is one row of the theme list.
type Item struct {
	Slug    string
	User    bool
	Applied bool
}

// ClientSetup explains how a remote client picks its own theme.
const ClientSetup = "Themes belong to your client machine.\n\nOpen a local terminal outside Herdr and run:\n\n  herdr-customize picker\n\nThen use Reload config in the Herdr client you want to update.\nInstall herdr-customize locally if you are connected to a remote server.\nThis server's theme and terminal colors are left unchanged."

// OnlyRemoteClients reports whether this runs inside Herdr while only
// herdr --remote clients are attached. Whoever opened it is then on another
// machine whose theme comes from its own config, so changing this machine's
// config would not affect them.
func OnlyRemoteClients() bool {
	if os.Getenv("HERDR_ENV") == "" {
		return false
	}
	processes, err := terminal.Processes()
	return err == nil && terminal.OnlyRemoteClients(processes)
}

// ShowClientSetup prints ClientSetup and waits for Enter.
func ShowClientSetup() {
	fmt.Println(ClientSetup + "\n\nPress Enter to close.")
	_, _ = bufio.NewReader(os.Stdin).ReadString('\n')
}

// AppliedSlug returns the recorded theme, or "" when none is usable.
func AppliedSlug() (string, error) {
	data, err := os.ReadFile(AppliedFile())
	if errors.Is(err, os.ErrNotExist) {
		return "", nil
	}
	if err != nil {
		return "", err
	}
	applied := strings.TrimSpace(string(data))
	if applied != "" && !IsValidSlug(applied) && !IsUserTheme(applied) {
		return "", nil
	}
	return applied, nil
}

// Themes lists the applied theme first, then user themes, then bundled ones.
// Unusable user entries are skipped and reported as warnings.
func Themes() ([]Item, []string, error) {
	var warnings []string
	users, err := readUserIndex()
	if err != nil {
		return nil, nil, err
	}
	bundledIndex := filepath.Join(PluginRoot(), "themes", "index.txt")
	bundled, invalid, err := readIndex(bundledIndex, IsValidSlug)
	if err != nil {
		return nil, nil, err
	}
	if len(invalid) > 0 {
		return nil, nil, fmt.Errorf("invalid slug %q in %s", invalid[0], bundledIndex)
	}
	applied, err := AppliedSlug()
	if err != nil {
		return nil, nil, err
	}
	var items []Item
	seen := make(map[string]bool)
	add := func(item Item) {
		if !seen[item.Slug] {
			seen[item.Slug] = true
			items = append(items, item)
		}
	}
	if applied != "" {
		add(Item{Slug: applied, User: IsUserTheme(applied), Applied: true})
	}
	for _, slug := range users {
		// users came from the index already; only the file needs checking.
		if _, err := userThemeFile(slug); err != nil {
			warnings = append(warnings, fmt.Sprintf("skipping unavailable user theme %q: %v", slug, err))
			continue
		}
		add(Item{Slug: slug, User: true})
	}
	for _, slug := range bundled {
		add(Item{Slug: slug})
	}
	return items, warnings, nil
}

// ValidateThemeBody checks a Ghostty palette before it is named and saved.
func ValidateThemeBody(body string) error {
	pal, err := ParsePaletteContent(body)
	if err != nil {
		return fmt.Errorf("invalid palette: %w", err)
	}
	if err := validatePalette(pal); err != nil {
		return fmt.Errorf("invalid palette: %w", err)
	}
	return nil
}

// SaveUserTheme stores body as a user theme named name and returns its slug.
// An existing theme is replaced only when overwrite is set; otherwise the
// slug is returned with ErrThemeExists.
func SaveUserTheme(name, body string, overwrite bool) (string, error) {
	if err := ValidateThemeBody(body); err != nil {
		return "", err
	}
	slug := Slugify(name)
	if !IsValidSlug(slug) {
		return "", fmt.Errorf("invalid or reserved theme name: %q", name)
	}
	dest := filepath.Join(UserThemesDir(), slug)
	if info, err := os.Lstat(dest); err == nil {
		if !info.Mode().IsRegular() {
			return "", fmt.Errorf("theme is not a regular file: %s", slug)
		}
		if !overwrite {
			return slug, ErrThemeExists
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return "", err
	}
	if err := saveUserTheme(slug, body); err != nil {
		return "", err
	}
	return slug, nil
}

func readAnswer(reader *bufio.Reader, prompt string) (string, error) {
	fmt.Fprint(os.Stderr, prompt)
	answer, err := reader.ReadString('\n')
	if err != nil && !(errors.Is(err, io.EOF) && answer != "") {
		return "", err
	}
	return strings.TrimSpace(answer), nil
}

// finalizeTheme names and saves body, prompting on stdin.
func finalizeTheme(body string) (string, error) {
	if err := ValidateThemeBody(body); err != nil {
		return "", err
	}
	reader := bufio.NewReader(os.Stdin)
	name, err := readAnswer(reader, "Theme name: ")
	if err != nil {
		return "", err
	}
	slug, err := SaveUserTheme(name, body, false)
	if errors.Is(err, ErrThemeExists) {
		yn, err := readAnswer(reader, fmt.Sprintf("%q already exists. Overwrite? [y/N] ", slug))
		if err != nil {
			return "", err
		}
		if !strings.EqualFold(yn, "y") && !strings.EqualFold(yn, "yes") {
			return "", nil
		}
		slug, err = SaveUserTheme(name, body, true)
		if err != nil {
			return "", err
		}
	} else if err != nil {
		return "", err
	}
	fmt.Fprintf(os.Stderr, "Saved theme %q.\n", slug)
	return slug, nil
}

func saveUserTheme(slug, body string) error {
	if !IsValidSlug(slug) {
		return fmt.Errorf("invalid slug: %q", slug)
	}
	if _, err := ParsePaletteContent(body); err != nil {
		return err
	}
	if err := os.MkdirAll(UserThemesDir(), 0o755); err != nil {
		return err
	}
	dest := filepath.Join(UserThemesDir(), slug)
	if info, err := os.Lstat(dest); err == nil {
		if !info.Mode().IsRegular() {
			return fmt.Errorf("theme is not a regular file: %s", slug)
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	if err := atomicWriteFile(dest, []byte(body), 0o644); err != nil {
		return err
	}
	if err := updateUserIndex(slug, false); err != nil {
		return fmt.Errorf("theme saved at %s, but index update failed: %w", dest, err)
	}
	return nil
}

func AddThemeClipboard() (string, error) {
	clip, err := ReadClipboard()
	if err != nil {
		return "", err
	}
	if strings.TrimSpace(clip) == "" {
		return "", fmt.Errorf("clipboard is empty (copy a Ghostty theme from %s first)", Hyperlink("https://terminalcolors.com", "https://terminalcolors.com"))
	}
	return finalizeTheme(clip)
}

// NewThemeFile writes NewThemeTemplate to a temporary file for an editor.
// The caller removes it.
func NewThemeFile() (string, error) {
	f, err := os.CreateTemp("", "herdr-theme-*.txt")
	if err != nil {
		return "", err
	}
	if _, err := f.WriteString(NewThemeTemplate); err != nil {
		f.Close()
		os.Remove(f.Name())
		return "", err
	}
	if err := f.Close(); err != nil {
		os.Remove(f.Name())
		return "", err
	}
	return f.Name(), nil
}

func AddThemeEditor() (string, error) {
	path, err := NewThemeFile()
	if err != nil {
		return "", err
	}
	defer os.Remove(path)
	cmd, err := EditorCommand(path)
	if err != nil {
		return "", err
	}
	cmd.Stdin, cmd.Stdout, cmd.Stderr = os.Stdin, os.Stdout, os.Stderr
	if err := cmd.Run(); err != nil {
		return "", fmt.Errorf("editor failed: %w", err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	return finalizeTheme(string(data))
}

// DeleteTheme confirms on stdin; guards apply equally to the picker and CLI.
func DeleteTheme(slug string) error { return deleteTheme(slug, os.Stdin) }

func deleteTheme(slug string, input io.Reader) error {
	if _, err := userThemePath(slug); err != nil {
		return err
	}
	answer, err := readAnswer(bufio.NewReader(input), fmt.Sprintf("Delete %q? [y/N] ", slug))
	if errors.Is(err, io.EOF) {
		return nil
	}
	if err != nil {
		return err
	}
	if !strings.EqualFold(answer, "y") && !strings.EqualFold(answer, "yes") {
		return nil
	}
	if err := RemoveUserTheme(slug); err != nil {
		return err
	}
	fmt.Fprintf(os.Stderr, "Deleted theme %q.\n", slug)
	return nil
}

// RemoveUserTheme deletes a user theme and clears the selection marker if it
// pointed at it. Bundled themes cannot be removed.
func RemoveUserTheme(slug string) error {
	dest, err := userThemePath(slug)
	if err != nil {
		return err
	}
	marker, err := os.ReadFile(AppliedFile())
	if err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	if err := os.Remove(dest); err != nil {
		return fmt.Errorf("delete theme: %w", err)
	}
	indexErr := updateUserIndex(slug, true)
	var markerErr error
	if strings.TrimSpace(string(marker)) == slug {
		markerErr = os.Remove(AppliedFile())
	}
	if err := errors.Join(indexErr, markerErr); err != nil {
		return fmt.Errorf("theme deleted, but state cleanup failed: %w", err)
	}
	return nil
}
