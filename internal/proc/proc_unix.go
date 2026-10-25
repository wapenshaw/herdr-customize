//go:build !windows

package proc

import (
	"errors"
	"os/exec"
	"syscall"
)

func startDetached(command func() *exec.Cmd) (*exec.Cmd, error) {
	cmd := command()
	// Setsid runs in the child before exec, so the helper has left the popup's
	// process group by the time Start returns.
	cmd.SysProcAttr = &syscall.SysProcAttr{Setsid: true}
	return cmd, cmd.Start()
}

func alive(pid int) bool {
	err := syscall.Kill(pid, 0)
	return err == nil || errors.Is(err, syscall.EPERM)
}
