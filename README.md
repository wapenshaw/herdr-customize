# herdr-customize

Customize [Herdr](https://herdr.dev) from one popup instead of editing
`config.toml` by hand. Four pages, switched with **Tab** or **1**–**4**:

- **Themes**: 112 bundled [terminalcolors.com](https://terminalcolors.com)
  palettes with a live preview, mapped onto Herdr's UI and your terminal's ANSI
  colors.
- **Plugins**: installed plugins and their actions, update checks, install,
  enable, disable, remove, logs, and a marketplace of `topic:herdr-plugin`
  repositories.
- **Keys**: every Herdr action with its binding, custom `[[keys.command]]`
  entries, and plugin actions that have no key yet. Conflicts and keys that break
  Herdr's rules are flagged before you save.
- **Settings**: every documented setting, generated from the config reference
  for your Herdr version.

Every config change edits `config.toml` in place (comments and layout are
kept), is checked with `herdr config check` before it is written, and reloads
Herdr.

<p align="center">
  <img alt="Herdr theme preview" src="https://github.com/user-attachments/assets/5100a157-1ed1-4659-be98-b846fb98681d" width="70%"/>
</p>

## Install

```bash
herdr plugin install wapenshaw/herdr-customize
```

Herdr builds the plugin with Go 1.24+. Supported on Linux, macOS and Windows.
The Plugins page uses `git` for update checks.

Herdr does not bind keys from plugin manifests. Add one binding to
`config.toml` and reload; after that the Keys page can add the rest:

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

The actions `herdr-customize.open`, `.plugins`, `.keys` and `.settings` open
the popup on that page.

Reinstall with the same command, or run `herdr plugin update`, to update.

For development, link a checkout instead (a linked checkout is not tracked for
updates):

```bash
git clone https://github.com/wapenshaw/herdr-customize.git
cd herdr-customize
herdr plugin link "$PWD"
herdr server reload-config
```

### Coming from herdr-theme-picker or Plugin Manager

This plugin replaces both. After installing it:

- The Themes page offers to import user themes saved by herdr-theme-picker
  (**I**, or `herdr-customize import-themes`). Existing themes are never
  replaced.
- The Keys page marks bindings to actions that are no longer installed, such as
  `herdr-theme-picker.open` or `ray.plugin-manager.open`. Point them at
  `herdr-customize.open` / `herdr-customize.plugins`, or delete them.
- Uninstall the old plugins from the Plugins page when you are done.

## Themes

Pick a palette and it is written to `[theme.custom]` in your Herdr
`config.toml`, Herdr reloads, and the terminal windows running Herdr on the same
machine are recolored to match.

### Where themes apply

Herdr reads its UI theme from the **client machine's** config. That includes a
laptop viewing a server through `herdr --remote`. The server never decides a
client's colors. This plugin follows the same rule: it writes the config of the
machine it runs on and recolors only Herdr terminals on that machine.

| Setup | Open the Themes page | Result |
| --- | --- | --- |
| Local `herdr` | Picker opens | Theme applied, Herdr reloads, its terminal recolors. |
| SSH into a host, then run `herdr` there | Picker opens on the host | The host's config is the client config; the SSH terminal recolors if it supports OSC colors. |
| `herdr --remote host` from a laptop, nobody attached locally on the host | Setup instructions | Run the picker on the laptop instead (see below). |
| `herdr --remote` client and a local client attached at the same time | Picker opens | Herdr cannot say who pressed the key. Picking changes the **host's** theme only. |
| A client without this plugin, or with no theme picked | — | Nothing is written. It keeps Herdr's default theme or its own config. |

The plugin never runs in the background and never recolors terminals on other
machines.

### Picker and custom themes

- **/** searches. The preview maps the palette onto a mock Herdr workspace.
- **Enter** applies the theme and closes the popup. **Esc** clears a search,
  then closes.
- **a** opens an editor for a Ghostty-format palette; **c** imports one from the
  clipboard. Either way you name it, and it is saved and applied.
- **e** edits a user theme's token colors with a live preview. **s** saves and
  applies; **Esc** discards the edits.
- **d** deletes a user theme after confirmation. Bundled themes are read-only.

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
herdr-customize ui themes
```

On Windows, run `herdr-customize.exe ui themes` from PowerShell or Windows Terminal.

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
herdr-customize ui [page]              # the popup: themes, plugins, keys or settings
herdr-customize apply dracula-default  # apply a theme by name
herdr-customize sync                   # re-send the applied colors to local clients
herdr-customize sync --client 12345    # ... to one client PID only
herdr-customize preview dracula-default
herdr-customize add [--clipboard]
herdr-customize edit my-theme
herdr-customize delete my-theme
herdr-customize import-themes          # copy user themes from herdr-theme-picker
```

Bundled palettes work offline. Import more from the clipboard or an editor by
copying any Ghostty palette from [terminalcolors.com](https://terminalcolors.com).

## Plugins

The list shows each plugin with its version, source and update state: `●` up
to date, `↑` behind (with the new version when its manifest says), `?` could not
be checked, `⌂` linked from a local checkout. Checks run `git ls-remote` against
the installed ref and need no GitHub token.

- **Enter** shows a plugin's actions. **Enter** on an action closes the popup
  and runs it; **b** binds it to a key on the Keys page.
- **u** updates the selected plugin and **U** updates every plugin that is
  behind. Both run `herdr plugin update` in the terminal so Herdr's trust prompt
  is the final gate. A plugin installed at an exact commit is skipped by **U**;
  **u** offers to move the pin.
- **i** installs `owner/repo[/subdir][@ref]`. **e** enables or disables, **x**
  uninstalls (or unlinks a local checkout), **o** opens the repository, **l**
  shows recent command logs, **c** opens `plugins.json` in your editor.
- **m** browses the marketplace: GitHub repositories tagged `herdr-plugin`,
  sorted by stars or recent activity (**s**), searchable with **/**. Searches use
  `GH_TOKEN`, `GITHUB_TOKEN` or `gh auth token` when available for a higher rate
  limit; set `HERDR_CUSTOMIZE_NO_TOKEN=1` to stay anonymous.

## Keys

One list for everything that owns a key:

- **Herdr actions** from the config reference, with the current binding (set
  values are highlighted) and the default. **Enter** edits; type
  comma-separated keys, or press **ctrl+r** and then the key to record it. An
  empty value, or **x**, restores the default.
- **Custom commands**: your `[[keys.command]]` entries. **n** adds one, **Enter**
  edits, **d** deletes. The form covers all four types (`plugin_action`,
  `popup`, `pane`, `shell`) and popup sizes.
- **Plugin actions without a key**, with **Enter** to bind one.

`⚠ conflict` marks a key used twice in the same mode. `✗` marks a key Herdr
would reject or that would swallow typing (a plain letter with no `prefix+` or
modifier), and `plugin_action` commands whose action is not installed.

## Settings

Every documented setting except keybindings and themes (those have their own
pages), grouped as in Herdr's config reference. Set values are highlighted and
marked `*`; the rest show their default.

- Settings with a fixed set of values show them on the row with the current one
  in brackets. **←**/**→** cycle through them and save once you stop. Booleans
  show `off`/`on` the same way; **Enter** also toggles them.
- The values come from the config reference for your Herdr version: enum values,
  plus names a string setting's description lists ("Accepts ctrl, alt, …").
  Those settings also take other values: pick `other…` or press **Enter** to type
  one.
- **Enter** on any other setting edits the value. Lists are entered as TOML,
  e.g. `["codex", "claude"]`.
- **x** restores the default by removing the key.
- **E** opens the whole `config.toml` in your editor. The edit is checked before
  it replaces the file; if Herdr rejects it, **E** reopens your edited copy.
- Problems reported by `herdr config check`, and keys Herdr does not document
  (typos, removed settings), are shown at the top and at the bottom of the list.

The reference matching your Herdr version is downloaded once and cached;
without network the bundled 0.9.5 reference is used.

## Configuration and state

Config lookup matches Herdr: `HERDR_CONFIG_PATH`, then
`$XDG_CONFIG_HOME/herdr/config.toml`, then `%APPDATA%\herdr\config.toml` on
Windows or `~/.config/herdr/config.toml` elsewhere. A missing config is created.

Edits change only the lines they touch, so comments, ordering and formatting
survive. Before writing, the new file is checked with `herdr config check`; a
change that adds a problem is refused and the file is left as it was. Problems
the file already had do not block unrelated edits. Inline tables are left
alone: change those with **E** on the Settings page.

The theme picker owns only the base `[theme.custom]` tokens. Everything else is
preserved: unrelated TOML, comments, permissions, symlinks, and
`[theme.custom.light]` / `[theme.custom.dark]` overrides used with
`auto_switch`. With `auto_switch = true` those layers still win over the picked
tokens. The first change each day backs up the original to
`config.toml.bak-YYYYMMDD`. Writes are atomic, and Herdr reloads after each
save.

`HERDR_PLUGIN_STATE_DIR` sets the state directory. Otherwise state lives in
`$XDG_STATE_HOME/herdr/plugins/herdr-customize` (`~/.local/state/...` by
default, `%LOCALAPPDATA%\herdr\plugins\herdr-customize` on Windows), so
standalone runs and the installed plugin share user themes and the `applied`
marker.

See [theme persistence edge cases](docs/theme-persistence-edge-cases.md) for the
full matrix.

## Ideas

Sidebar row layouts with a preview, saved pane layouts, tab bar and window
title builders, sounds and notifications, and restoring config backups.

## Development

```sh
go test ./...
go vet ./...
go fmt ./...
go build -o herdr-customize ./cmd/herdr-customize
```

- `cmd/herdr-customize`: CLI routing.
- `internal/ui`: the Bubble Tea popup and its four pages.
- `internal/config`: in-place `config.toml` edits, checks, backups and reloads.
- `internal/schema`: Herdr's config reference (bundled and per-version cache).
- `internal/keys`: key parsing, Herdr's binding rules and conflicts.
- `internal/plugins`: `herdr plugin` calls, update checks, marketplace search.
- `internal/theme`: palettes, user themes, `[theme.custom]` writes, apply and sync.
- `internal/terminal`: client discovery and OSC color writes.
- `internal/herdr`, `internal/fsutil`, `internal/proc`, `internal/instance`:
  the Herdr CLI, atomic writes, detached helpers and file locks.
- `tests/`: public API and regression tests; `tests/testutil` isolates every
  run in a temporary home/config/state and fakes `herdr`.

Tests must not recolor live terminals, reload real Herdr sessions, or touch the
real clipboard. Cross-build platform-sensitive changes for Windows, Linux, and
macOS; emulator and SSH behavior needs native checks. Windows checks are listed
in [docs/windows-test-plan.md](docs/windows-test-plan.md).

## Credits

- The theme picker started as
  [qintmb/herdr-theme-picker](https://github.com/qintmb/herdr-theme-picker),
  and its contributors.
- The Plugins page is a Go port of
  [speardragon/herdr-plugin-manager](https://github.com/speardragon/herdr-plugin-manager)
  (MIT).

## License

MIT; see [LICENSE](LICENSE).
