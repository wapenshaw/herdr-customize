# Using the popup

The popup has four pages. **Tab** and **Shift+Tab** switch pages, as do
**1** to **4**. Arrow keys only move within a page. **?** lists every key on
the current page, and **Esc** closes the popup.

Pages that open a file use the editor from `VISUAL` or `EDITOR`, which may
include arguments such as `code --wait`. Without either, the plugin tries
`nvim` and `nano`, then falls back to `vi`, or to Notepad on Windows.

## Settings

The Settings page lists every documented setting except keybindings and
themes, which have their own pages. Settings are grouped as in Herdr's config
reference. Settings you have changed are highlighted and marked `*`; the rest
show their default.

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

## Themes

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

`●` marks the applied theme and `★` marks the ones you added. What a theme
changes, and on which machine, is in [Themes](themes.md).

## Plugins

The list on the left shows each plugin's version and update state: `●` up to
date, `↑` an update is available, `?` the check failed, `⌂` linked from a
local checkout. **Install from GitHub…** and **Browse the marketplace…** sit
below the plugins.

The panel on the right describes the selected plugin and lists what you can do
with it: its actions, Settings (with a count of what it keeps), Recent runs,
Update (with the version it would move to), Disable, Uninstall and Open
repository. **→** or **Enter** moves into the panel, **↑**/**↓** pick, and
**Enter** runs the item. **←** or **Esc** goes back. The letter keys below
work from either side.

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

Update checks run `git ls-remote` against the installed ref, once per repo and
ref and four at a time, give up after 20 seconds, and need no GitHub token.

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

Plugin authors can describe their settings with a schema; see
[plugin-authors.md](plugin-authors.md).
