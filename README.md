# herdr-customize

Customize [Herdr](https://herdr.dev) from one popup instead of editing
`config.toml` by hand. It runs on Linux, macOS and Windows.

![The Settings page: every Herdr setting grouped by section, with the allowed values on each row](docs/screenshots/settings.png)

The popup has four pages. **Tab** and **Shift+Tab** switch pages, as do
**1** to **4**. Arrow keys only move within a page.

- **Settings**: every documented setting for your Herdr version, with the
  allowed values shown on each row.
- **Keys**: every Herdr action and its binding, your custom
  `[[keys.command]]` entries, and plugin actions that have no key yet.
  Conflicts are flagged before you save.
- **Themes**: 112 bundled [terminalcolors.com](https://terminalcolors.com)
  palettes with a live preview. Picking one recolors Herdr's UI and the
  terminal running it.
- **Plugins**: your installed plugins and their actions, update checks,
  install, enable/disable, uninstall, logs, each plugin's own settings, and a
  marketplace.

The plugin edits `config.toml` in place and keeps your comments and layout.
It runs `herdr config check` on every change before writing it, then reloads
Herdr.

| Keys | Themes | Plugins |
| --- | --- | --- |
| ![Keys page flagging a conflict](docs/screenshots/keys.png) | ![Themes page with a live preview of the selected palette](docs/screenshots/themes.png) | ![Plugins page with an update available](docs/screenshots/plugins.png) |

## Install

```bash
herdr plugin install wapenshaw/herdr-customize
```

The install downloads the prebuilt binary for your platform from this
repository's GitHub release and checks it against the SHA-256 sums published
with it.
Release binaries exist for:

- Linux on x86_64 and arm64 (static, so they run on any distro)
- macOS on Apple Silicon and Intel
- Windows on x64 (ARM64 Windows runs it under emulation)

If the download fails, or there is no release for the version you are
installing, the install builds from source with `cargo` instead. That needs
Rust 1.99 or newer, plus the Visual Studio Build Tools (C++ workload) on
Windows. Set `HERDR_CUSTOMIZE_BUILD_FROM_SOURCE=1` to always build from
source.

On Windows the install step runs in the Windows PowerShell 5.1 that ships
with Windows, so PowerShell 7 is not needed. Herdr started from PowerShell 7
works too.

The Plugins page uses `git` for update checks.

### Bind a key

Herdr does not bind keys from plugin manifests, so bind one once after
installing:

```bash
herdr plugin action invoke bind --plugin herdr-customize
```

That binds `prefix+t` to the popup and reloads Herdr, and the result shows
as a Herdr notification. It changes nothing if one of the plugin's actions
already has a key, and refuses a key another binding uses. For another key,
run `./target/release/herdr-customize bind <key>` in the plugin's checkout,
or change it on the Keys page. The Keys page can add the rest.

To bind it by hand instead, add this to `config.toml` and reload:

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

`herdr-customize.open` opens the popup on Settings, and `.keys`, `.themes`
and `.plugins` open it on that page.

To update, run `herdr plugin update herdr-customize` or update it from its own
Plugins page.

### Coming from herdr-theme-picker or Plugin Manager

This plugin replaces both. The Keys page marks bindings whose action is no
longer installed, such as `herdr-theme-picker.open` or
`ray.plugin-manager.open`. Point them at `herdr-customize.themes` or
`herdr-customize.plugins`, or delete them, then uninstall the old plugins from
the Plugins page. Themes you added to herdr-theme-picker can be added again
from the clipboard or your editor.

## Themes

When you pick a palette, it is written to `[theme.custom]` in your Herdr
`config.toml` and Herdr reloads. The terminal windows running Herdr on the
same machine are recolored to match.

### Where themes apply

Herdr reads its UI theme from the config on the client machine, including
a laptop that connects to a server with `herdr --remote`. The server never
decides a client's colors. This plugin follows the same rule: it writes the
config of the machine it runs on and recolors only Herdr terminals on that
machine.

| Setup | Opening the Themes page | Result |
| --- | --- | --- |
| Local `herdr` | The picker opens | The theme is applied, Herdr reloads, and its terminal is recolored. |
| SSH into a host, then run `herdr` there | The picker opens on the host | The host's config is the client config. The SSH terminal is recolored if it supports OSC colors. |
| `herdr --remote host` from a laptop, with nobody attached locally on the host | Setup instructions | Run the picker on the laptop instead (see below). |
| A `herdr --remote` client and a local client attached at the same time | The picker opens | Herdr cannot tell who pressed the key, so picking changes only the host's theme. |
| A client without this plugin, or with no theme picked | Not applicable | Nothing is written. That client keeps Herdr's default theme or its own config. |

The plugin never runs in the background and never recolors terminals on other
machines.

### Picking and adding themes

Themes are grouped as in use, yours, dark and light. Each row shows a strip of
the theme's 16 colors. The preview on the right draws a mock Herdr workspace in
the selected theme, with a line of status colors and the full palette.

- Type to search. Matches in a theme's name rank first, and letters typed in
  order match too, so `catlat` finds `catppuccin-latte`. **Backspace** and
  **ctrl+u** edit the query. **Esc** clears it, and a second **Esc** closes
  the popup.
- **↑**/**↓** move through the list. **←**/**→** switch between all, dark,
  light and your themes.
- **Enter** applies the selected theme and closes the popup.
- **ctrl+t** replaces the mock with the token table, which shows the palette
  entry behind each Herdr color.
- **ctrl+a**, or **＋ Add new theme…** at the end of the list, opens a
  Ghostty-format template in your editor. **ctrl+v**, or **＋ Add from
  clipboard…**, reads a palette from the clipboard, and pasting with your
  terminal's paste works too. Either way, you name the theme, and it is saved
  and applied.
- **ctrl+e** edits a theme you added, one token at a time, with a live
  preview. **s** saves and applies; **Esc** discards your edits.
- **ctrl+d** deletes a theme you added, after asking. Bundled themes cannot be
  deleted.
- **?** lists every key.

`●` marks the applied theme and `★` marks the ones you added. The editor comes
from `VISUAL` or `EDITOR` and may include arguments such as `code --wait`.
Without either, the plugin tries `nvim` and `nano`, then falls back to `vi`, or
to Notepad on Windows.

A palette needs `background`, `foreground` and `palette` entries 0 to 15 as
six-digit hex colors. Names become lowercase hyphenated slugs, and a theme you
add replaces a bundled theme with the same name. Your themes are stored in
`<state>/themes/`, listed in `<state>/index.txt`, and kept across plugin
updates.

The token table maps palette colors to Herdr's UI tokens: `accent` uses
palette 4, `red` uses palette 1, `panel_bg` uses the background, and so on.
Per-token edits are stored in the palette file as
`# hpick-override: token=#rrggbb` comments.

Deleting a palette leaves the applied config alone. Pick another theme, or
remove `[theme.custom]` and reload to go back to Herdr's built-in theme.

### Terminal colors

The Herdr UI (sidebar, tabs, borders) uses `[theme.custom]`, but text inside
panes uses your terminal emulator's 16 ANSI colors. Applying a theme therefore
also sends OSC 4/10/11 color sequences to the terminal of every interactive
Herdr client on the same machine. It then queries the colors back, so Herdr
refreshes its pane colors.

- Recolored: local `herdr`, `herdr --session …`, `herdr session attach …`, and
  `herdr --remote …` clients started on this machine. They all read this
  machine's config, so they share the theme.
- Left alone: the Herdr server, CLI commands such as `herdr status`, the
  `remote-client-bridge` process that serves another machine's client, and
  terminals owned by other user accounts.
- The new colors last as long as the terminal window; a new window starts
  with the emulator's own colors. Run `herdr-customize sync`, or the
  **Customize: sync terminal colors** action, to send them again.
- In Ghostty, you can keep the colors in new windows: create
  `~/.config/ghostty/herdr-theme` (on Windows,
  `%APPDATA%\ghostty\herdr-theme`) and add `config-file = herdr-theme` to your
  Ghostty config. The plugin refreshes that file each time you apply a theme,
  but never creates it.

Ghostty, iTerm2, WezTerm, kitty, Alacritty and Windows Terminal accept OSC
color sequences. Other emulators may not, and a multiplexer nested inside
Herdr can swallow them.

### Using `herdr --remote` from another machine

A `--remote` client takes its theme from its own machine, so pick the theme
there. Clone this repository on that machine and fetch the binary with the
same script the plugin install uses:

```sh
git clone https://github.com/wapenshaw/herdr-customize.git
cd herdr-customize
sh scripts/fetch-or-build.sh
./target/release/herdr-customize ui themes
```

On Windows, run `powershell -ExecutionPolicy Bypass -File scripts\fetch-or-build.ps1`
(or the same with `pwsh`), then `.\target\release\herdr-customize.exe ui themes`.

Run it in a terminal outside Herdr. A Herdr pane runs on the server, so a
picker started there would change the server's config. The binary finds the
bundled palettes in the checkout's `themes/` directory.

The picker writes the laptop's config, creating it if needed, and recolors
the laptop's `herdr --remote` windows. The laptop has no Herdr server to
reload, so press Herdr's reload key in the client afterwards
(`prefix+shift+r`).

### Commands

```sh
herdr-customize ui [page]              # the popup: settings, keys, themes or plugins
herdr-customize open [page]            # open the popup inside Herdr from a shell
herdr-customize bind [key]             # bind a key to the popup (default: prefix+t)
herdr-customize apply dracula-default  # apply a theme by name
herdr-customize sync                   # send the applied colors to local clients again
herdr-customize sync --client 12345    # ... to one client PID only
herdr-customize preview dracula-default
```

Themes are added, edited and deleted on the Themes page. The bundled palettes
work offline. For more, copy any Ghostty palette from
[terminalcolors.com](https://terminalcolors.com) and add it from the
clipboard.

## Plugins

The list on the left shows each plugin's version and update state: `●` up to
date, `↑` an update is available, `?` the check failed, `⌂` linked from a
local checkout. The checks run `git ls-remote` against the installed ref and
need no GitHub token. **Install from GitHub…** and **Browse the
marketplace…** sit below the plugins.

The panel on the right describes the selected plugin and lists what you can do
with it: its actions, Settings (with a count of what it keeps), Recent runs,
Update (with the version it would move to), Disable, Uninstall and Open
repository. **→** or **Enter** moves into the panel, **↑**/**↓** pick, and
**Enter** runs the item. **←** or **Esc** goes back. The letter keys below
work from either side, and **?** lists them.

- **Enter** on an action closes the popup and runs the action. **b** binds the
  action to a key on the Keys page.
- **u** updates the selected plugin and **U** updates every plugin that has an
  update. Both run `herdr plugin update` in the terminal, so Herdr's trust
  prompt has the final say. **U** skips plugins pinned to an exact commit;
  **u** offers to move the pin.
- **i** installs `owner/repo[/subdir][@ref]`. **e** enables or disables, **x**
  uninstalls (or unlinks a local checkout), **o** opens the repository, **l**
  shows recent command logs, and **c** opens `plugins.json` in your editor.
- **m** opens the marketplace: GitHub repositories with the `herdr-plugin`
  topic, sorted by stars or recent activity (**s**) and searchable with **/**.
  Searches use `GH_TOKEN`, `GITHUB_TOKEN` or `gh auth token` when one is
  available, for a higher rate limit. Set `HERDR_CUSTOMIZE_NO_TOKEN=1` to stay
  anonymous.
- **s** opens the selected plugin's settings.

### Plugin settings

Herdr gives each plugin a config directory (`herdr plugin config-dir <id>`) but
no file format. **s** reads whatever the plugin keeps there, so it works with
any plugin:

- `*.toml` files: every key, typed from its value (on/off, number, text,
  list). Commented-out defaults such as `# port = 6363` are listed too, with
  the comment above them as the description.
- `*.env` files: `KEY=value` lines and commented-out defaults (`#KEY=500`),
  typed from the value. Setting a key writes `KEY=value` below its default,
  and **x** removes that line again.
- Other files, such as scripts or JSON, are listed, and **Enter** opens them in
  your editor.

The keys match the Settings page: **←**/**→** pick on/off and listed values,
**Enter** edits anything else, **x** resets, and **E** opens the file. Edits
keep comments and layout, and the file is backed up once a day. Herdr does
not check plugin settings; each plugin reads its own, and some only pick up
changes after a restart.

Plugin authors can describe their settings by shipping `config-schema.json` in
the plugin root. It uses the format of Herdr's config reference, plus an
optional `file` (default `config.toml`). Listed keys then show their
description, default and allowed values even before the file exists:

```json
{
  "file": "config.toml",
  "sections": [{ "id": "general", "title": "General", "keys": [
    { "key": "toggle_placement", "type": "enum", "default": "\"split\"",
      "values": ["split", "popup", "tab"], "description": "Where the panel opens." },
    { "key": "index_on_startup", "type": "boolean", "default": "true",
      "description": "Index sessions when Herdr starts." }
  ]}]
}
```

Types are `boolean`, `integer`, `float`, `string`, `enum` and `array`, and
`default` is a TOML value.

## Keys

One list covers everything that owns a key:

- **Herdr actions** from the config reference, each with its current binding
  (highlighted when you have set it) and its default. **Enter** edits the
  binding: type comma-separated keys, or press **ctrl+r** and then the key to
  record it. An empty value, or **x**, restores the default.
- **Custom commands**: your `[[keys.command]]` entries. **n** adds one,
  **Enter** edits and **d** deletes. The form covers all four types
  (`plugin_action`, `popup`, `pane`, `shell`) and popup sizes.
- **Plugin actions without a key**. **Enter** binds one.

`⚠ conflict` marks a key that is used twice in the same mode. `✗` marks a key
Herdr would reject, a key that would swallow typing (a plain letter with no
`prefix+` or modifier), and a `plugin_action` whose action is not installed.

## Settings

The Settings page lists every documented setting except keybindings and
themes, which have their own pages. Settings are grouped as in Herdr's config
reference.
Settings you have changed are highlighted and marked `*`; the rest show their
default.

- A setting with a fixed set of values shows them all on its row, with the
  current one in brackets. **←**/**→** step through them, wrapping around, and
  save once you stop. **Enter** steps forward too. Booleans show `off`/`on`
  the same way.
- The values come from the config reference for your Herdr version: enum
  values, plus any names that a string setting's description lists
  ("Accepts ctrl, alt, …"). Those string settings take other values too:
  **←**/**→** pick a listed one, and **Enter** lets you type any value.
- **Enter** on any other setting edits its value. Lists are entered as TOML,
  such as `["codex", "claude"]`.
- **x** restores the default by removing the key.
- **E** opens the whole `config.toml` in your editor. Your edit is checked
  before it replaces the file. If Herdr rejects it, **E** reopens your edited
  copy.
- Problems reported by `herdr config check`, and keys Herdr does not document
  (typos, removed settings), are listed at the top and bottom of the page.

The reference for your Herdr version is downloaded once and cached. Without a
network connection, the bundled 0.9.5 reference is used.

## Configuration and state

The plugin finds `config.toml` the same way Herdr does: `HERDR_CONFIG_PATH`,
then `$XDG_CONFIG_HOME/herdr/config.toml`, then `%APPDATA%\herdr\config.toml`
on Windows or `~/.config/herdr/config.toml` elsewhere. A missing config is
created.

Edits change only the lines they touch, so comments, ordering and formatting
stay as they were. This works for keys inside inline tables
(`ui = { sidebar_width = 30 }`) and dotted keys too. Before writing, the
plugin checks the new file with `herdr config check`. If the change adds a
problem, the plugin refuses it and leaves the file alone. Problems the file
already had do not block unrelated edits.

The Themes page only replaces the base `[theme.custom]` tokens. Unrelated
TOML, comments, permissions, symlinks, and the `[theme.custom.light]` /
`[theme.custom.dark]` layers used with `auto_switch` are kept. With
`auto_switch = true`, those layers still take priority over the picked
tokens. The first change each day backs up the original file to
`config.toml.bak-YYYYMMDD`. Writes are atomic, and Herdr reloads after each
save. Writers take turns through a lock file in the state folder below, so two
popups never undo each other's edits.

State is stored in `HERDR_PLUGIN_STATE_DIR` when it is set, and otherwise in
`$XDG_STATE_HOME/herdr/plugins/herdr-customize` (by default
`~/.local/state/...`, or `%LOCALAPPDATA%\herdr\plugins\herdr-customize` on
Windows). Standalone runs and the installed plugin therefore share your themes
and the record of the applied one.

[Theme persistence edge cases](docs/theme-persistence-edge-cases.md) has the
full matrix.

## Ideas

Sidebar row layouts with a preview, saved pane layouts, tab bar and window
title builders, sounds and notifications, and restoring config backups.

## Development

How changes are made here (branches, commits, pull requests and releases) is
in [CONTRIBUTING.md](CONTRIBUTING.md).

```sh
cargo test
cargo clippy --all-targets
cargo fmt
cargo build --release
```

`herdr plugin link` skips the build step, so build before linking a checkout:

```bash
git clone https://github.com/wapenshaw/herdr-customize.git
cd herdr-customize
cargo build --release
herdr plugin link "$PWD"
herdr server reload-config
```

- `src/main.rs`: CLI routing.
- `src/bind.rs`: `bind`, which adds the first key binding for the popup.
- `src/ui/`: the popup. `mod.rs` holds the event loop, `Msg`, `Ctx` and the
  `Page` trait. The four pages, the plugin settings screen and shared widgets
  sit beside it, along with `tests.rs`, a driver that runs the popup without a
  terminal.
- `src/config.rs`: in-place `config.toml` edits with `toml_edit`, checks,
  backups and reloads.
- `src/schema/`: Herdr's config reference, bundled plus a per-version cache.
- `src/keys.rs`: key parsing, Herdr's binding rules and conflicts.
- `src/plugins/`: `herdr plugin` calls, update checks and marketplace search.
- `src/pluginconfig.rs`: typed reading and editing of plugin TOML and `.env`
  files.
- `src/theme/`: palettes, your themes, `[theme.custom]` writes, apply, sync
  and the preview.
- `src/terminal/`: finding Herdr clients and recoloring them. `unix.rs` writes
  to the client's TTY; `windows.rs` runs a helper that attaches to the
  client's console.
- `src/herdr.rs`, `src/fsutil.rs`, `src/proc.rs`: the Herdr CLI, atomic
  writes and locks, and detached helpers.
- `scripts/fetch-or-build.*`: the install step that downloads a release
  binary or builds from source.

Tests must not recolor live terminals, reload a real Herdr session or touch
the real clipboard. The UI tests point every path into a temporary sandbox and
run a scripted `herdr` (a shell script, or a batch file on Windows). Terminal
emulator and SSH behavior still needs checking by hand on each OS; the Windows
steps are in [the Windows test plan](docs/windows-test-plan.md).

### Releases

[release-please](https://github.com/googleapis/release-please) cuts releases
from the Conventional Commits on `main`. It keeps a release pull request open
that bumps the version in `Cargo.toml`, `Cargo.lock` and `herdr-plugin.toml`
and writes `CHANGELOG.md`. Merging that pull request tags `vX.Y.Z` and opens
a draft release. The release workflow then builds the binaries listed under
[Install](#install), attaches them with `SHA256SUMS` and publishes the draft,
so an install never finds a release without its binaries.

Running the release workflow by hand builds every target without releasing
anything. Given the tag of an existing release, it rebuilds that release's
binaries and replaces them.

### Go version

herdr-customize was first written in Go. That version is kept on the
[`go`](https://github.com/wapenshaw/herdr-customize/tree/go) branch, and
[docs/go-vs-rust.md](docs/go-vs-rust.md) compares the two implementations:
startup time, memory, binary size, build times and how the code differs.

## Credits

- The Themes page started as
  [qintmb/herdr-theme-picker](https://github.com/qintmb/herdr-theme-picker).
  Thanks to its contributors.
- The Plugins page is a port of
  [speardragon/herdr-plugin-manager](https://github.com/speardragon/herdr-plugin-manager)
  (MIT).

## License

MIT; see [LICENSE](LICENSE).
