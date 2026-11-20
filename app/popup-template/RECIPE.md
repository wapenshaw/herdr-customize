# Named terminal popup

Open a named command in a floating terminal window from its configured shortcut.

## What you see

A draggable title bar sits above the terminal. Close dismisses the window; errors
appear as notifications.

## How it behaves

The generator installs a separate mod for each popup and writes `popup.json`
next to its `main.js`. It contains `name`, an `argv` array, `width` and
`height` as fractions of the window, and a boolean `persistent`. The command runs in the focused
local terminal's working directory when available. Arguments are passed to
`hc run`; core may interpret a single argument as a shell command.

A persistent popup reuses its live labeled terminal when reopened, and closing
its window leaves the process running. For a nonpersistent popup the title bar
owns the terminal: whenever its window goes (Close, a layout switch, being
replaced by reopening) the bar's `close()` hook ends the terminal. A window
also closes when its terminal ends.

## Built from

- A generated `mod.open` binding and a small launcher overlay.
- A window layout with a title-bar mod and a core terminal block.
- `hc run`, `hc ls`, `terminal.kill`, and Herdr's overlay and notification APIs.

## Watch out

- Popups start locally; they cannot be launched from a remote space.
- Switching layouts closes popup windows. A persistent terminal keeps running
  and can be reopened; a nonpersistent terminal is ended by the bar's `close()`.
- Persistence reuses a live terminal, not a restarted command after it exits or
  after the core loses it.
- Reopening an already open nonpersistent popup replaces its window and ends
  the old terminal. Quitting the app runs the same hook within the app's
  250 ms cleanup limit; that path is not verified.

## Done when

- The shortcut opens the named command in a window of the configured size.
- Closing the window, switching layouts or reopening ends a nonpersistent
  command and leaves a persistent command running.
- Reopening a persistent popup returns to its live terminal.
