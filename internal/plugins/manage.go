package plugins

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"time"

	"github.com/wapenshaw/herdr-customize/internal/config"
	"github.com/wapenshaw/herdr-customize/internal/herdr"
)

// SetEnabled enables or disables a plugin.
func SetEnabled(id string, enabled bool) error {
	verb := "disable"
	if enabled {
		verb = "enable"
	}
	_, err := herdr.Run("plugin", verb, id)
	return err
}

// Remove uninstalls a GitHub plugin or unlinks a local checkout.
func Remove(p Plugin) error {
	verb := "uninstall"
	if !p.GitHub() {
		verb = "unlink"
	}
	_, err := herdr.Run("plugin", verb, p.ID)
	return err
}

// Log is one recorded plugin command run.
type Log struct {
	ID       string   `json:"log_id"`
	Plugin   string   `json:"plugin_id"`
	Command  []string `json:"command"`
	Event    string   `json:"event"`
	Action   string   `json:"action_id"`
	Status   string   `json:"status"`
	ExitCode *int     `json:"exit_code"`
	Started  int64    `json:"started_unix_ms"`
	Finished int64    `json:"finished_unix_ms"`
	Stdout   string   `json:"stdout"`
	Stderr   string   `json:"stderr"`
}

// Trigger names what ran the command: an event or an action.
func (l Log) Trigger() string {
	switch {
	case l.Event != "":
		return l.Event
	case l.Action != "":
		return l.Action
	}
	return strings.Join(l.Command, " ")
}

// StartedAt is when the command started.
func (l Log) StartedAt() time.Time { return time.UnixMilli(l.Started) }

// Logs returns the most recent command logs for a plugin.
func Logs(id string, limit int) ([]Log, error) {
	out, err := herdr.Run("plugin", "log", "list", "--plugin", id, "--limit", fmt.Sprint(limit))
	if err != nil {
		return nil, err
	}
	var response struct {
		Result struct {
			Logs []Log `json:"logs"`
		} `json:"result"`
	}
	if err := json.Unmarshal([]byte(out), &response); err != nil {
		return nil, fmt.Errorf("herdr plugin log list: %w", err)
	}
	logs := response.Result.Logs
	slices.Reverse(logs) // newest first
	return logs, nil
}

// RegistryPath is Herdr's record of installed plugins, beside its socket
// or, failing that, beside config.toml.
func RegistryPath() string {
	if socket := os.Getenv("HERDR_SOCKET_PATH"); socket != "" && runtime.GOOS != "windows" {
		path := filepath.Join(filepath.Dir(socket), "plugins.json")
		if _, err := os.Stat(path); err == nil {
			return path
		}
	}
	return filepath.Join(filepath.Dir(config.Path()), "plugins.json")
}

// OpenURL opens url in the default browser without a shell.
func OpenURL(url string) error {
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "darwin":
		cmd = exec.Command("open", url)
	case "windows":
		cmd = exec.Command("rundll32", "url.dll,FileProtocolHandler", url)
	default:
		cmd = exec.Command("xdg-open", url)
	}
	if err := cmd.Start(); err != nil {
		return fmt.Errorf("open %s: %w", url, err)
	}
	go cmd.Wait()
	return nil
}
