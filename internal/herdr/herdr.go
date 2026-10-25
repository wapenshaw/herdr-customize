// Package herdr wraps the herdr CLI: config checks, reloads and plugin
// commands. Every call honors HERDR_BIN_PATH, which Herdr sets for plugins.
package herdr

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"regexp"
	"strings"
)

// Bin returns the herdr executable to run.
func Bin() string {
	if bin := os.Getenv("HERDR_BIN_PATH"); bin != "" {
		return bin
	}
	return "herdr"
}

// Command builds a herdr invocation without running it.
func Command(args ...string) *exec.Cmd {
	return exec.Command(Bin(), args...)
}

// Run executes herdr and returns stdout. A failure carries the CLI's own
// message: herdr prints JSON errors for socket commands and text otherwise.
func Run(args ...string) (string, error) {
	cmd := Command(args...)
	var stdout, stderr bytes.Buffer
	cmd.Stdout, cmd.Stderr = &stdout, &stderr
	if err := cmd.Run(); err != nil {
		msg := ErrorMessage(stderr.String() + stdout.String())
		if msg == "" {
			return stdout.String(), err
		}
		return stdout.String(), fmt.Errorf("%s", msg)
	}
	return stdout.String(), nil
}

// ErrorMessage extracts the first useful line of herdr output, unwrapping
// {"error":{"message":...}} payloads.
func ErrorMessage(output string) string {
	for _, line := range strings.Split(output, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		if strings.HasPrefix(line, "{") {
			var payload struct {
				Error struct {
					Message string `json:"message"`
				} `json:"error"`
			}
			if json.Unmarshal([]byte(line), &payload) == nil && payload.Error.Message != "" {
				return payload.Error.Message
			}
		}
		return line
	}
	return ""
}

// Reload executes 'herdr server reload-config'. A machine that only connects
// with herdr --remote has no local server; the user reloads instead.
func Reload() error {
	out, err := Command("server", "reload-config").CombinedOutput()
	if err != nil {
		return fmt.Errorf("%w: %s", err, strings.TrimSpace(string(out)))
	}
	return nil
}

var versionRe = regexp.MustCompile(`\b(\d+\.\d+\.\d+)\b`)

// Version returns the installed herdr version, such as "0.9.5".
func Version() (string, error) {
	out, err := Command("--version").Output()
	if err != nil {
		return "", err
	}
	m := versionRe.FindStringSubmatch(string(out))
	if m == nil {
		return "", fmt.Errorf("unrecognized herdr version output: %q", strings.TrimSpace(string(out)))
	}
	return m[1], nil
}

// CheckResult is the outcome of 'herdr config check'.
type CheckResult struct {
	// Ran is false when the check could not run, for example on a machine
	// without herdr or with a herdr too old to have the command.
	Ran    bool
	Issues []string
	// Skipped explains why the check did not run.
	Skipped string
}

// CheckConfig validates the config at path with herdr's own parser.
func CheckConfig(path string) CheckResult {
	cmd := Command("config", "check")
	cmd.Env = append(os.Environ(), "HERDR_CONFIG_PATH="+path)
	out, err := cmd.CombinedOutput()
	text := strings.TrimSpace(string(out))
	if err == nil {
		return CheckResult{Ran: true}
	}
	var exit *exec.ExitError
	if errors.As(err, &exit) && exit.ExitCode() == 1 && strings.HasPrefix(text, "config: issues found") {
		var issues []string
		for _, line := range strings.Split(text, "\n")[1:] {
			if line = strings.TrimRight(line, "\r"); strings.TrimSpace(line) != "" {
				issues = append(issues, line)
			}
		}
		return CheckResult{Ran: true, Issues: issues}
	}
	reason := err.Error()
	if text != "" {
		reason = ErrorMessage(text)
	}
	return CheckResult{Skipped: reason}
}
