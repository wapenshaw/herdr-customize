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

The plugin never runs in the background and never recolors terminals on other
machines. [Theme persistence edge cases](theme-persistence-edge-cases.md) has
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

Deleting a palette leaves the applied config alone. Pick another theme, or
remove `[theme.custom]` and reload to go back to Herdr's built-in theme.
