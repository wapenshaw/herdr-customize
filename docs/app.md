# The Herdr app

The Herdr app (`herdr-app`) is a desktop window over terminals that
herdr-core runs. It is separate from Herdr in the terminal: it keeps its
own themes, keys and mods in its own folder, and it follows edits to those
files live. herdr-customize writes its palettes as the app's themes and
opens a popup inside the app with two pages, Keys and Mods. It never
touches Herdr's `config.toml` from there.

## Set it up

herdr-customize needs its folder, not just the binary, because the palettes
live beside it. If Herdr in the terminal already has the plugin, use that
binary:

```sh
herdr-customize app install
```

Otherwise clone this repository and run `scripts/fetch-or-build.sh` (or
`.ps1` on Windows), which puts the binary in `target/release`, then run
`target/release/herdr-customize app install`.

`app install` writes 26 well-known palettes, and the ones you added in
Herdr, to the app's `themes` folder. It also writes a small mod,
`mods/herdr-customize`, that opens the popup in a floating terminal, and
binds **ctrl+shift+m** (**cmd+shift+m** on macOS) to it. A key already in
use is never taken: pass another one, as in
`herdr-customize app install alt-shift-c`. Run it again after moving the
binary; it updates the mod and keeps the key.

The app's folder is `HERDR_APP_HOME`, else `$XDG_CONFIG_HOME/herdr-app`,
else `~/.config/herdr-app` (on Windows, under your user profile). The app
must be installed; herdr-customize finds `herdr-app` on `PATH` or where its
installers put it, or at `HERDR_APP_BIN`.

## Themes

Pick themes in the app's own **Settings > Look**, which lists every file in
its `themes` folder, dark and light apart. The app keeps a dark and a light
theme for each layout and shows the one that matches **Appearance**: a light
theme picked while the system is dark shows only once Appearance is Light.

A theme recolors the terminals; the app derives its panel colors from the
theme's background, so a change between two dark themes is a change of
tint.

Settings draws every theme file each time it opens, and with all 112
palettes it fails with "This view could not be rendered", so
herdr-customize writes only some. `app apply <name>` writes any other one and
shows it, and `app themes` writes the set again after you add a palette in
Herdr. A theme file you edited, or one herdr-customize did not write, is
left alone; `--force` replaces it. The record is
`data/herdr-customize/themes.json`.

## Keys

`keys.json` by section, then the app's commands that no key runs yet.
**Enter** changes a binding or binds a command, **n** adds one, **d**
unbinds, and **o** opens the file in your editor.

A section decides where a key counts: `terminal` wins over `layout:<name>`,
which wins over `global`, and `mod:<name>` counts while that mod has focus.
**◆** marks a key another section also binds. The form refuses a key the app
cannot read and an action it does not know, and warns about a key that
types text, one programs rely on (ctrl+c, d, l, r, w, a, e, u, k, z and [),
and ctrl+alt, which is AltGr on Windows.

## Mods

Mods and layouts installed from here, the ones added by hand, and the ones
the app ships. **m** opens a marketplace of GitHub repositories tagged
`herdr-mod` or `herdr-layout` (←/→ switches), and **i** installs from
`owner/repo`, or `owner/repo/folder` for one package of a repository that
holds several.

Nothing is written before a review, which shows what the package adds: its
mods, layouts, themes and keys, and whether its code runs programs, reaches
the network or reads and writes files. A name another package or you already
use stops the install. A key you already use stays yours unless you press
**k**. Each mod then goes through the app's own check.

**u** updates a package, asking first when you changed one of its files.
**x** removes it with the keys it added; files you changed stay. The record
is `data/herdr-customize/packages.json`. Downloads use `git`.

## Commands

```sh
herdr-customize app ui [keys|mods]         # the popup, in any terminal
herdr-customize app install [key]          # the themes, the mod and its key
herdr-customize app themes [--force]       # the palettes to the app again
herdr-customize app apply dracula-default  # show a theme in the app
herdr-customize app reset                  # back to graphite and paper
```

`--home DIR` before the command picks another app folder; the mod passes
the app's own. `apply` and `reset` need the app open.
