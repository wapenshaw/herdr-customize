// Package proc starts helper processes that outlive the popup. Herdr tears
// down a popup's whole process group when it closes, so a helper has to be
// in its own session (Unix) or detached process group (Windows).
package proc

import (
	"os"
	"os/exec"
	"time"
)

// StartDetached starts path outside the caller's process group with no
// standard streams, and does not wait for it.
func StartDetached(path string, args ...string) error {
	cmd, err := startDetached(func() *exec.Cmd { return exec.Command(path, args...) })
	if err != nil {
		return err
	}
	return cmd.Process.Release()
}

// WaitExit waits until pid has exited or timeout passes, and reports
// whether it exited.
func WaitExit(pid int, timeout time.Duration) bool {
	deadline := time.Now().Add(timeout)
	for alive(pid) {
		if time.Now().After(deadline) {
			return false
		}
		time.Sleep(50 * time.Millisecond)
	}
	return true
}

// Self is this executable, for starting helpers.
func Self() (string, error) { return os.Executable() }
