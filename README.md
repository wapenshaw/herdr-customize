# herdr-customize

Customize [Herdr](https://herdr.dev) from a popup instead of editing
`config.toml` by hand.

Today it ships the theme picker: 112 bundled
[terminalcolors.com](https://terminalcolors.com) palettes with a live preview,
mapped onto Herdr's UI and your terminal's ANSI colors. Plugin management, a
keybinding editor and a full settings editor are next; see the
[roadmap](#roadmap).

<p align="center">
  <img alt="Herdr theme preview" src="https://github.com/user-attachments/assets/5100a157-1ed1-4659-be98-b846fb98681d" width="70%"/>
</p>

## Install

```bash
herdr plugin install wapenshaw/herdr-customize
```

Herdr builds the plugin with Go 1.24+. The theme picker needs
[fzf](https://github.com/junegunn/fzf). Supported on Linux, macOS and Windows.

Herdr does not bind keys from plugin manifests, so add a binding to
`config.toml` and reload:

```toml
[[keys.command]]
key = "prefix+t"
type = "plugin_action"
command = "herdr-customize.open"
description = "customize herdr"
```

```bash
herdr server reload-config
```

Reinstall with the same command, or run `herdr plugin update`, to update.

For development, link a checkout instead (a linked checkout is not tracked for
updates):

```bash
git clone https://github.com/wapenshaw/herdr-customize.git
cd herdr-customize
herdr plugin link "$PWD"
herdr server reload-config
```

## Themes

Pick a palette and it is written to `[theme.custom]` in your Herdr
`config.toml`, Herdr reloads, and the terminal windows running Herdr on the same
machine are recolored to match.

### Where themes apply

Herdr reads its UI theme from the **client machine's** config. That includes a
laptop viewing a server through `herdr --remote`. The server never decides a
client's colors. This plugin follows the same rule: it writes the config of the
machine it runs on and recolors only Herdr terminals on that machine.

| Setup | Press `prefix+t` | Result |
| --- | --- | --- |
| Local `herdr` | Picker opens | Theme applied, Herdr reloads, its terminal recolors. |
| SSH into a host, then run `herdr` there | Picker opens on the host | The host's config is the client config; the SSH terminal recolors if it supports OSC colors. |
| `herdr --remote host` from a laptop, nobody attached locally on the host | Setup instructions | Run the picker on the laptop instead (see below). |
| `herdr --remote` client and a local client attached at the same time | Picker opens | Herdr cannot say who pressed the key. Picking changes the **host's** theme only. |
| A client without this plugin, or with no theme picked | — | Nothing is written. It keeps Herdr's default theme or its own config. |

The plugin never runs in the background and never recolors terminals on other
machines.

### Picker and custom themes

- Type to search. The preview maps the palette onto a mock Herdr workspace.
- **Enter** applies the theme and closes the popup. **Esc** cancels.
- **Tab** or **+ Add new theme…** opens an editor for a Ghostty-format palette.
- **+ Add from clipboard…** imports a palette from the clipboard and applies it.
- **Ctrl-E** edits a user theme's token overrides. **Save & apply** commits;
  **Esc** discards the scratch edits.
- **Ctrl-D** deletes a user theme after confirmation. Bundled themes are read-only.

The applied theme is marked `✓`; user themes are marked `★`. `VISUAL` and
`EDITOR` are respected, including arguments such as `code --wait`. Fallback
editors are `nvim`, `nano`, then `vi` on Unix or Notepad on Windows.

Palettes need `background`, `foreground`, and `palette` entries 0–15 as six-digit
hex colors. New names become lowercase hyphenated slugs. User themes override
bundled themes with the same name. They live in `<state>/themes/`, indexed by
`<state>/index.txt`, and survive plugin updates.

The preview legend shows how colors become Herdr UI tokens. For example,
`accent` uses palette 4, `red` uses palette 1, and `panel_bg` uses the background.
Per-token edits are stored as `# hpick-override: token=#rrggbb` comments.

Deleting a palette does not change the applied config. Pick another theme, or
remove `[theme.custom]` and reload to return to your built-in theme.

### Terminal colors

The Herdr UI (sidebar, tabs, borders) uses `[theme.custom]`. Text inside panes
uses your terminal emulator's 16 ANSI colors. Applying a theme therefore also
sends OSC 4/10/11 color setters to the terminal of every interactive Herdr
client on the **same machine**, then queries the colors back so Herdr refreshes
its pane colors.

- Targets: local `herdr`, `herdr --session …`, `herdr session attach …`, and
  `herdr --remote …` clients started on this machine. Each reads this machine's
  config, so they share the theme.
- Never targeted: the Herdr server, CLI commands such as `herdr status`, the
  `remote-client-bridge` process serving another machine's client, and
  terminals owned by other user accounts.
- Changes last as long as the terminal window. A new window starts with its
  emulator's own colors. Run `herdr-customize sync` or the **Customize:
  sync terminal colors** action to re-send them.
- **Ghostty:** to keep the colors in new windows, create
  `~/.config/ghostty/herdr-theme` (on Windows,
  `%APPDATA%\ghostty\herdr-theme`) and add `config-file = herdr-theme` to your
  Ghostty config. The picker refreshes that file on every apply. It never
  creates the file.

Support varies by emulator. Ghostty, iTerm2, WezTerm, kitty, Alacritty, and
Windows Terminal implement OSC color setters; nested multiplexers can swallow
them.

### Using `herdr --remote` from another machine

A `--remote` client takes its theme from its own machine, so pick the theme
there:

```sh
go install github.com/wapenshaw/herdr-customize/cmd/herdr-customize@latest
herdr-customize picker
```

On Windows, run `herdr-customize.exe picker` from PowerShell or Windows Terminal.

Run it in a terminal **outside** Herdr. A Herdr pane runs on the server, so a
picker started there changes the server's config. The bundled palettes live in
`themes/`, so set `HERDR_PLUGIN_ROOT` to a checkout of this repo, or keep the
binary beside a `themes/` directory.

The picker writes the laptop's config (creating it if needed) and recolors the
laptop's `herdr --remote` terminal windows. A laptop has no local Herdr server
to reload, so the picker tells you to press Herdr's reload key in the client:
**`prefix+shift+r`**. The server is not involved.

### Commands

```sh
herdr-customize picker                 # interactive theme picker
herdr-customize apply dracula-default  # apply by name
herdr-customize sync                   # re-send the applied colors to local clients
herdr-customize sync --client 12345    # ... to one client PID only
herdr-customize preview dracula-default
herdr-customize add [--clipboard]
herdr-customize edit my-theme
herdr-customize delete my-theme
```

Bundled palettes work offline. Import more from the clipboard or an editor by
copying any Ghostty palette from [terminalcolors.com](https://terminalcolors.com).

## Configuration and state

Config lookup matches Herdr: `HERDR_CONFIG_PATH`, then
`$XDG_CONFIG_HOME/herdr/config.toml`, then `%APPDATA%\herdr\config.toml` on
Windows or `~/.config/herdr/config.toml` elsewhere. A missing config is created.

The theme picker owns only the base `[theme.custom]` tokens. Everything else is
preserved: unrelated TOML, comments, permissions, symlinks, and
`[theme.custom.light]` / `[theme.custom.dark]` overrides used with
`auto_switch`. With `auto_switch = true` those layers still win over the picked
tokens. The first change each day backs up the original to
`config.toml.bak-YYYYMMDD`. Writes are atomic.

`HERDR_PLUGIN_STATE_DIR` sets the state directory. Otherwise state lives in
`$XDG_STATE_HOME/herdr/plugins/herdr-customize` (`~/.local/state/...` by
default, `%LOCALAPPDATA%\herdr\plugins\herdr-customize` on Windows), so
standalone runs and the installed plugin share user themes and the `applied`
marker.

See [theme persistence edge cases](docs/theme-persistence-edge-cases.md) for the
full matrix.

## Roadmap

One popup with tabs, all in Go, on Linux, macOS and Windows:

- **Plugins**: list, install, update, enable/disable, uninstall, marketplace,
  and run plugin actions. A Go port of
  [herdr-plugin-manager](https://github.com/speardragon/herdr-plugin-manager).
- **Keys**: every built-in action with its current and default binding, record a
  new key, conflict warnings, custom `[[keys.command]]` entries, and one-step
  bindings for plugin actions that have none.
- **Settings**: an editor generated from Herdr's config reference (toggles,
  choices, numbers, colors, paths). Every save keeps your comments, runs
  `herdr config check`, rolls back on failure and reloads.
- Later: sidebar row layouts with a preview, saved pane layouts, tab bar and
  window title builders, sounds and notifications, config backups.

## Development

```sh
go test ./...
go vet ./...
go fmt ./...
go build -o herdr-customize ./cmd/herdr-customize
```

- `cmd/herdr-customize`: CLI routing.
- `internal/theme`: palettes, picker/editor, config writes, apply and sync.
- `internal/terminal`: client discovery and OSC color writes.
- `internal/instance`: cross-platform file locks.
- `tests/`: public API and regression tests; `tests/testutil` isolates every
  run in a temporary home/config/state and fakes `fzf` and `herdr`.

Tests must not recolor live terminals, reload real Herdr sessions, or touch the
real clipboard. Cross-build platform-sensitive changes for Windows, Linux, and
macOS; emulator and SSH behavior needs native checks. Windows checks are listed
in [docs/windows-test-plan.md](docs/windows-test-plan.md).

## Credits

- The theme picker started as
  [qintmb/herdr-theme-picker](https://github.com/qintmb/herdr-theme-picker),
  and its contributors.
- Plugin management is based on
  [speardragon/herdr-plugin-manager](https://github.com/speardragon/herdr-plugin-manager).

## License

MIT; see [LICENSE](LICENSE).
