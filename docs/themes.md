# Themes

When you pick a palette, it is written to `[theme.custom]` in your Herdr
`config.toml` and Herdr reloads. The terminal windows running Herdr on the
same machine are recolored to match.

## Where themes apply

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

The plugin never recolors terminals on other machines. [Theme persistence edge cases](theme-persistence-edge-cases.md) has
the full matrix.

## Terminal colors

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
- The new colors last as long as the terminal window, so a new window starts
  with the emulator's own colors. When Herdr starts, the plugin recolors the
  window that started it (see below).
- Run `herdr-customize sync`, or the **Customize: sync terminal colors**
  action, to send the colors again by hand.

Ghostty, iTerm2, WezTerm, kitty, Alacritty and Windows Terminal accept OSC
color sequences. Other emulators may not, and a multiplexer nested inside
Herdr can swallow them.

### When Herdr starts

When the Herdr server starts, it runs `herdr-customize sync --startup` once.
For ten seconds this sends the applied colors to each Herdr client on the
machine as it appears, including the window that started the server, and
then it exits. Nothing keeps running.

- After you install or update the plugin, it first runs at the next server
  start.
- With no theme applied it exits at once.
- A window you open later, attaching to a server that is already running,
  keeps the terminal's colors. Run `sync`, or use a scheme file (below). The
  same goes for a laptop that only runs `herdr --remote` clients, since it
  starts no server.

### Keeping the colors in new windows

A scheme file makes every new window open in the theme, including one that
attaches to a running server and one on a machine with no Herdr server. Each
time you apply or sync a theme, the plugin refreshes the scheme file your
terminal loads when it starts.

- **Ghostty, kitty, Alacritty, WezTerm**: the file lives in your own config,
  so the plugin only refreshes it once you have created it (an empty file is
  enough) and pointed the terminal at it:

  | Terminal | Create | Then add to your config |
  | --- | --- | --- |
  | Ghostty | `~/.config/ghostty/herdr-theme` (Windows: `%APPDATA%\ghostty\herdr-theme`) | `config-file = herdr-theme` |
  | kitty | `~/.config/kitty/herdr-theme.conf` | `include herdr-theme.conf` in `kitty.conf` |
  | Alacritty | `~/.config/alacritty/herdr-theme.toml` (Windows: `%APPDATA%\alacritty\herdr-theme.toml`) | `import = ["~/.config/alacritty/herdr-theme.toml"]` under `[general]` |
  | WezTerm | `~/.config/wezterm/colors/herdr.toml` | `config.color_scheme = "Herdr"` |

  `~/.config` stands for `$XDG_CONFIG_HOME` when that is set. Create the file,
  apply a theme once, then point the terminal at it.
- **Windows Terminal**: the plugin writes a color scheme named **Herdr** to
  `%LOCALAPPDATA%\Microsoft\Windows Terminal\Fragments\herdr-customize\herdr-theme.json`,
  the folder Windows Terminal reads other apps' settings from. Pick it once for
  the profile you start Herdr from (Settings → your profile → Appearance →
  Color scheme, or `"colorScheme": "Herdr"` in `settings.json`).
- **iTerm2**: the plugin writes a profile named **Herdr** to
  `~/Library/Application Support/iTerm2/DynamicProfiles/herdr-customize.json`.
  It takes everything but its colors from your default profile; start Herdr
  from it, or make it the default.

Terminals read these files when they start (Alacritty and iTerm2 also when
they change), so a window already open keeps the previous theme until it is
restarted; `herdr-customize sync` recolors it meanwhile. Uninstalling the
plugin leaves the files in place, frozen at the last theme; delete them to
remove them.

## Using `herdr --remote` from another machine

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

## Palettes

The bundled palettes come from [terminalcolors.com](https://terminalcolors.com)
and work offline. For more, copy any Ghostty palette from there and add it
from the clipboard.

A palette needs `background`, `foreground` and `palette` entries 0 to 15 as
six-digit hex colors. Names become lowercase hyphenated slugs, and a theme you
add replaces a bundled theme with the same name. Your themes are stored in
`<state>/themes/`, listed in `<state>/index.txt`, and kept across plugin
updates; `<state>` is described in [Configuration](configuration.md#state).

The token table maps palette colors to Herdr's UI tokens: `accent` uses
palette 4, `red` uses palette 1, `panel_bg` uses the background, and so on.
Per-token edits are stored in the palette file as
`# hpick-override: token=#rrggbb` comments.

The Themes page only replaces the base `[theme.custom]` tokens. The
`[theme.custom.light]` and `[theme.custom.dark]` layers used with
`auto_switch` are kept, and with `auto_switch = true` they still take priority
over the picked tokens.

## Going back to Herdr's theme

**↩ Herdr default**, below the themes, or `herdr-customize reset`, goes back
to the theme named by `theme.name` (Catppuccin when it is unset). It removes
the palette from `[theme.custom]`, forgets the applied theme, resets the
terminal colors with OSC 104/110/111 and reloads. The `[theme.custom.light]`
and `[theme.custom.dark]` layers stay, and so do the terminal scheme files,
which keep the last theme until you apply another.

A `[theme.custom]` that sets only some tokens, such as the `active_row_bg`
that Herdr Radar keeps between its `# >>> herdr-radar theme block` markers, is
a tweak on Herdr's theme rather than a palette: the reset leaves it, and the
row still shows `●` as in use. Only a table holding every token this plugin
writes counts as a theme.

Deleting a palette leaves the applied config alone; reset or pick another
theme.
