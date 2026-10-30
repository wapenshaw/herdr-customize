# Theme persistence edge cases

## Model

Herdr reads its UI theme from the client machine's config, including when a
laptop views a server through `herdr --remote`. Plugin panes and actions run on
the server, and Herdr 0.9.3 does not tell a plugin which client invoked it.

The picker therefore acts on the machine it runs on:

1. Write `[theme.custom]` into that machine's config and record the slug in
   `<state>/applied`.
2. Recolor every interactive Herdr client terminal on that machine (OSC 4/10/11
   setters, then readback queries), and refresh the scheme files terminals load on start: the Ghostty, kitty,
   Alacritty and WezTerm ones the user created, and the Windows Terminal
   fragment and iTerm2 dynamic profile when those terminals are installed.
3. Run `herdr server reload-config`. If that fails (for example on a laptop that
   only runs `--remote` clients), print a warning pointing to the client's
   reload key, `prefix+shift+r`.

Nothing runs in the background. When the server starts, Herdr runs
`herdr-customize sync --startup`, which syncs each client that appears within
ten seconds and then exits.

## Connection matrix

| Case | Handling |
| --- | --- |
| Local `herdr`, `prefix+t` | Picker opens; config, terminal, and reload as above. |
| SSH shell, then `herdr` on the host | The client runs on the host, so the host config is the client config. Its SSH terminal is recolored if the emulator supports OSC colors. |
| `herdr --remote host`, no local client on the host | The popup detects only `remote-client-bridge` processes and shows instructions to run the picker on the client machine. Nothing is written. |
| `--remote` and local clients attached together | The invoker cannot be identified; the picker opens and changes the host's theme. The remote client is unaffected. |
| Picker run on the laptop outside Herdr | Writes the laptop config and recolors the laptop's `herdr --remote` windows. The reload fails (no local server) and the user presses `prefix+shift+r`. |
| Client without the plugin, or no theme picked | Nothing is written. It keeps Herdr's default or its own config. |
| Client with its own terminal palette on another machine | Never touched. |
| Client on the same machine with its own terminal palette | Recolored on apply, because it reads the same config. Use a separate `HERDR_CONFIG_PATH` and run its own picker to keep it independent. |
| Several named sessions/servers on one machine | No shared daemon or lock; applies affect every local client, which share the config. Starting a server also re-sends the same colors to windows already open. |
| New terminal window that starts the server | Synced within ten seconds by `sync --startup`. |
| New terminal window attaching to a running server, or on a machine with no server (`--remote` only) | Starts with its emulator's colors. Run `sync`, or use a terminal scheme file (see [Themes](themes.md#keeping-the-colors-in-new-windows)). Most terminals read it only when they start. |
| Client changes its own palette after apply | Nothing restores the old palette. |
| `applied` no longer matches the config | `sync` refuses rather than send colors from a stale selection. |

## Process classification

| Process | Example arguments | Classified as |
| --- | --- | --- |
| Local client | `herdr`, `herdr --session work`, `herdr session attach work` | interactive, recolored |
| Outbound remote client | `herdr --remote host` | interactive and remote: recolored, but not counted as a local invoker |
| Remote bridge | `herdr remote-client-bridge --idle-timeout-v1` (under `sshd`, no TTY) | bridge: never recolored; counts as a remote attachment |
| Server / CLI | `herdr server`, `herdr status`, `herdr plugin action invoke …` | ignored |

Arguments come from `kern.procargs2` on macOS, `/proc/<pid>/cmdline` on Linux,
and the process command line on Windows. Unknown arguments are rejected rather
than guessed.

## Terminal writes

| Platform | Write path |
| --- | --- |
| macOS / Linux | Revalidate PID and TTY, then write the `/dev/<tty>` character device. Permission errors (another user's terminal) are skipped. |
| Windows | A short-lived helper (`terminal-sync <pid>`) attaches to the exact client console and writes `CONOUT$` with VT processing enabled, then restores the console mode. |

The picker never prints color setters to its own pane. Writes can interleave
with a client's frame output; a native check per emulator is still needed.

## Config writes

- Only the base `[theme.custom]` tokens are replaced. `[theme.custom.light]` and
  `[theme.custom.dark]` survive whether they are standard tables, dotted keys
  or inline tables; `toml_edit` edits each spelling where it stands.
- Repeated applies produce byte-identical files.
- Unrelated TOML, comments, CRLF, permissions, and symlinks are preserved; the
  first change each day is backed up; writes are atomic.

## Regression coverage

- New block, token replacement with layers kept and repeated writes stable,
  layers without a base block, inline theme tables, and the OSC payload
  (`src/theme/apply.rs`).
- Interactive command classification (including `herdr.exe` paths), the
  remote-only popup decision, and Windows command-line splitting
  (`src/terminal/mod.rs`); macOS argument parsing (`src/terminal/unix.rs`).
- Additions placed after trailing comments (`src/config.rs`).

Native checks still needed per OS: OSC setters and readback in each emulator,
and a real `--remote` round trip.
Windows steps are listed in [the Windows test plan](windows-test-plan.md).
