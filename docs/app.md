# The Herdr app

The Herdr app (`herdr-app`) is a desktop window over terminals that
herdr-core runs. It is separate from Herdr in the terminal: it keeps its
own themes, keys and mods in its own folder, and it follows edits to those
files live. herdr-customize opens a popup inside the app with three pages,
Themes, Keys and Mods, and never touches Herdr's `config.toml` from there.

![The popup in the Herdr app, on the Themes page](screenshots/app-themes.png)

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

`app install` writes a small mod, `mods/herdr-customize`, that opens the
popup in a floating terminal, and binds **ctrl+shift+m** (**cmd+shift+m** on
macOS) to it. A key already in use is never taken: pass another one, as in
`herdr-customize app install alt-shift-c`. Run it again after moving the
binary; it updates the mod and keeps the key.

The app's folder is `HERDR_APP_HOME`, else `$XDG_CONFIG_HOME/herdr-app`,
else `~/.config/herdr-app` (on Windows, under your user profile). The app
must be installed; herdr-customize finds `herdr-app` on `PATH` or where its
installers put it, or at `HERDR_APP_BIN`.

## Themes

The list holds the app's own themes (graphite, ink, moss and paper), the
theme files in its `themes` folder, and every palette herdr-customize has.
**Enter** applies one and leaves the popup open, so you can try the next:
the app shows it for its lightness, as its dark theme or its light one, and
remembers the choice for the shown layout. A palette is written to
`themes/<name>.json` the first time it is picked.

**↩ Herdr app default** goes back to graphite and paper. **ctrl+a** and
**ctrl+v** add a palette, as on the Themes page in Herdr, and **ctrl+d**
deletes a theme file. **ctrl+s** writes every palette to the app at once, so
the app's own theme picker lists them too.

herdr-customize records what it wrote in `data/herdr-customize/themes.json`.
A theme file you edited, or one it did not write, is left alone.

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
herdr-customize app ui [themes|keys|mods]  # the popup, in any terminal
herdr-customize app install [key]          # the mod and its key
herdr-customize app themes [--force]       # every palette to the app
herdr-customize app apply dracula-default  # show a theme in the app
herdr-customize app reset                  # back to graphite and paper
```

`--home DIR` before the command picks another app folder; the mod passes
the app's own. `apply` and `reset` need the app open.
