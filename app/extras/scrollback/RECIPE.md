# Scrollback in an editor

Open the focused terminal's retained text in an editor in a new tab.

## What you see

Press Ctrl+Shift+S (Cmd+Shift+S on macOS). A small overlay says "Opening
scrollback" until the editor tab opens. Errors appear as a notification.

## How it behaves

The mod reads up to 100,000 history lines above the screen and saves the
result under the app home in `data/scrollback/<terminal>.txt`. It replaces a
previous snapshot only if that file has not changed since it was read. The
editor receives the snapshot's path as its final argument.

Optionally create `data/scrollback/config.json` under the app home:

```json
{ "editor": ["nvim"], "lines": 100000 }
```

Both fields are optional. `editor` is a nonempty array of program and argument
strings; use an array to preserve spaces in paths or arguments. `lines` is a
nonnegative integer. Without an editor override, Unix uses `$VISUAL`, then
`$EDITOR`, then `vi`; Windows uses `notepad`. Environment values are split on
whitespace, not interpreted as shell commands; use `editor` for quoted paths
or arguments. For a remote space the environment and editor are on that
machine, and `hc put` copies the snapshot there before opening the tab.

## Built from

- `mod.json`: a small `place: none` overlay, opened by `mod.open:scrollback`.
- `main.js`: a gpui-kit `View`, `focused`, `commands`, `hc`, `exec`,
  `readTextFile`, `writeTextFile`, `run`, `notify` and `closeOverlay`.
- `keys.json`: global Ctrl+Shift+S and Cmd+Shift+S bindings.

## Watch out

- Snapshots can contain secrets and remain on disk until removed manually;
  remote copies also remain on the remote machine. Reopening the same terminal
  replaces its local snapshot, so save edits elsewhere first.
- Herdr's text-file API limits snapshots to 1 MiB. Reduce `lines` if a snapshot
  is too large. Core may return only the screen or incomplete history while a
  terminal is working, blocked, or on an alternate screen.
- A remote space needs a core with `hc put`; a terminal from a different
  machine than that space cannot be opened there. If the focused space changes
  while preparing the snapshot, try again in the original space.
- Choose an editor installed on the machine where the new tab runs. The
  environment fallback does not parse shell quoting. An existing key binding
  may need a different shortcut.
- The overlay has no effect during `herdr-app check`; checking cannot exercise
  the actual terminal, editor, or remote transfer.

## Done when

- The shortcut opens the focused terminal's text in a new editor tab.
- With no focused terminal, a notification explains why nothing opened.
- `herdr-app check app/extras/scrollback` passes.
