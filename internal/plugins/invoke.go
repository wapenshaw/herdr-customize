package plugins

import (
	"bufio"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strconv"
	"strings"
	"time"

	"github.com/wapenshaw/herdr-customize/internal/herdr"
	"github.com/wapenshaw/herdr-customize/internal/proc"
)

// InvokeAfterClose runs action once this popup has closed. Herdr refuses to
// open UI from an action while a popup is showing, so a detached copy of
// this program waits for the popup process to exit and invokes it then.
func InvokeAfterClose(action string) error {
	self, err := proc.Self()
	if err != nil {
		return err
	}
	return proc.StartDetached(self, "invoke-after-close", strconv.Itoa(os.Getpid()), action)
}

// RunInvokeAfterClose is the detached half of InvokeAfterClose.
func RunInvokeAfterClose(pid int, action string) error {
	proc.WaitExit(pid, 30*time.Second)
	var err error
	// Herdr takes a moment to return to the workspace view.
	for range 15 {
		if _, err = herdr.Run("plugin", "action", "invoke", action); err == nil {
			return nil
		}
		message := err.Error()
		if !strings.Contains(message, "ui_busy") && !strings.Contains(message, "popup already open") {
			return err
		}
		time.Sleep(200 * time.Millisecond)
	}
	return err
}

// Interactive returns a command that runs herdr with args on the terminal,
// for installs and updates whose trust prompt needs it, then waits for
// enter so the outcome stays readable.
func Interactive(args ...string) (*exec.Cmd, error) {
	self, err := proc.Self()
	if err != nil {
		return nil, err
	}
	return exec.Command(self, append([]string{"herdr-run"}, args...)...), nil
}

// RunInteractive is the body of Interactive's command.
func RunInteractive(stdin io.Reader, stdout io.Writer, args []string) int {
	fmt.Fprintf(stdout, "\n$ herdr %s\n\n", strings.Join(args, " "))
	cmd := herdr.Command(args...)
	cmd.Stdin, cmd.Stdout, cmd.Stderr = os.Stdin, os.Stdout, os.Stderr
	code := 0
	if err := cmd.Run(); err != nil {
		code = 1
		var exit *exec.ExitError
		if errors.As(err, &exit) {
			code = exit.ExitCode()
		} else {
			fmt.Fprintln(stdout, err)
		}
	}
	if code == 0 {
		fmt.Fprint(stdout, "\n✓ done")
	} else {
		fmt.Fprintf(stdout, "\n✗ cancelled or failed (exit %d)", code)
	}
	fmt.Fprint(stdout, " · press enter to return ")
	bufio.NewReader(stdin).ReadString('\n')
	return code
}
