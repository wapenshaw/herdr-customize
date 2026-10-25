# Theme persistence edge cases

## Model

Herdr reads its UI theme from the client machine's config, including when a
laptop views a server through `herdr --remote`. Plugin panes and actions run on
the server, and Herdr 0.9.3 does not tell a plugin which client invoked it.

The picker therefore acts on the machine it runs on:

1. Write `[theme.custom]` into that machine's config and record the slug in
   `<state>/applied`.
2. Recolor every interactive Herdr client terminal on that machine (OSC 4/10/11
   setters, then readback queries), and refresh the Ghostty fragment if the user
   created it.
3. Run `herdr server reload-config`. If that fails (for example on a laptop that
   only runs `--remote` clients), print a warning pointing to the client's
   reload key, `prefix+shift+r`.

No background process exists. Nothing runs at server startup or on reconnect.

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
| Several named sessions/servers on one machine | No shared daemon or lock; applies affect every local client, which share the config. |
| New terminal window | Starts with its emulator's colors. Run `sync`, or use the Ghostty fragment for persistence. |
| Client changes its own palette after apply | Nothing restores the old palette. |
| `applied` no longer matches the config | `sync` refuses rather than send colors from a stale selection. |

## Process classification

| Process | Example arguments | Classified as |
| --- | --- | --- |
| Local client | `herdr`, `herdr --session work`, `herdr session attach work` | interactive, recolored |
| Outbound remote client | `herdr --remote host` | interactive and remote: recolored, but not counted as a local invoker |
| Remote bridge | `herdr remote-client-bridge --idle-timeout-v1` (under `sshd`, no TTY) | bridge: never recolored; counts as a remote attachment |
| Server / CLI | `herdr server`, `herdr status`, `herdr plugin action invoke …` | ignored |

Arguments come from `kern.procargs2` on macOS and `/proc/<pid>/cmdline` on
Linux. Unknown arguments are rejected rather
than guessed.

## Terminal writes

| Platform | Write path |
| --- | --- |
| macOS / Linux | Revalidate PID and TTY, then write the `/dev/<tty>` character device. Permission errors (another user's terminal) are skipped. |

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
- Interactive command classification, the remote-only popup decision, and
  macOS argument parsing (`src/terminal.rs`).
- Additions placed after trailing comments (`src/config.rs`).

Native checks still needed per OS: OSC setters and readback in each emulator,
and a real `--remote` round trip.
