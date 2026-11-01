# The Herdr app

The Herdr app (`herdr-app`) is a desktop window over terminals that
herdr-core runs. It is separate from Herdr in the terminal: it keeps its
own themes, keys and mods in its own folder, and it follows edits to those
files live. herdr-customize writes its palettes as the app's themes and
adds a screen to the app with three pages: Themes, Keys and Mods. It never
touches Herdr's `config.toml` from there.

| Themes | Keys | Mods |
| --- | --- | --- |
| ![Themes: the palettes with a terminal drawn in the selected one's colors](screenshots/app-themes.png) | ![Keys: a binding open in the form, with its command's description](screenshots/app-keys.png) | ![Mods: herdr-mod repositories on GitHub by stars, each with Review](screenshots/app-mods.png) |

## Set it up

The easiest way is to ask an agent running in the app, as for any mod on
herdr.dev:

```text
Read ~/.config/herdr-app/AGENTS.md, then add this mod: github.com/wapenshaw/herdr-customize/tree/main/app
```

Its [recipe](../app/RECIPE.md) has the agent set up the program rather
than copy the mod's files, since the screen does nothing without it.

By hand: herdr-customize needs its folder, not just the binary, because the
palettes live beside it. Clone this repository to
`~/.local/share/herdr-customize`, run `scripts/fetch-or-build.sh` there (or
`.ps1` on Windows), which puts the binary in `target/release`, then run
`target/release/herdr-customize app install`. If Herdr in the terminal
already has the plugin, its binary works too.

`app install` writes 26 well-known palettes, and the ones you added in
Herdr, to the app's `themes` folder. It also writes a mod,
`mods/herdr-customize`, that draws the screen with the app's own controls
and colors, and binds **ctrl+shift+y** (**cmd+shift+y** on macOS) to it.
There, **Tab** and **Shift+Tab** switch pages and **Esc** closes it. A key already in
use is never taken: pass another one, as in
`herdr-customize app install alt-shift-c`. Run it again after moving the
binary; it updates the mod and keeps the key.

The mod is also published on its own in [app/](../app), so it can be added
like any other. A copy that `app install` did not write looks for the
binary in `~/.local/share/herdr-customize`, then on `PATH`, and shows the
steps above when there is none.

The app's folder is `HERDR_APP_HOME`, else `$XDG_CONFIG_HOME/herdr-app`,
else `~/.config/herdr-app` (on Windows, under your user profile). The app
must be installed; herdr-customize finds `herdr-app` on `PATH` or where its
installers put it, or at `HERDR_APP_BIN`.

## Themes

Every palette, with a preview of a terminal in its colors. Type to filter,
**↑**/**↓** to browse and **Enter** to use one. The app keeps a dark and a
light theme for each layout and shows the one that matches **Appearance**,
so a light theme picked while the system is dark shows only once it turns
light; the page says so.

The app's own **Settings > Look** lists the files in its `themes` folder.
Settings draws every one each time it opens, and with all 112 palettes it
fails with "This view could not be rendered", so herdr-customize writes only
some there; using any other palette writes its file first.

A theme from herdr-customize recolors the whole window. The app derives its
backgrounds, accent and agent colors from any theme, but draws panel text
in neutral grey; these themes give the sidebar, tabs and menus the
palette's own text color, with muted and dim shades of it that stay
readable. An accent or sidebar color you set for a palette on Herdr's Themes
page comes along.

`app themes` writes the set again after you add a palette in Herdr. A theme
file you edited, or one herdr-customize did not write, is left alone;
`--force` replaces it. The record is `data/herdr-customize/themes.json`.

## Keys

`keys.json` by section, then the app's commands that no key runs yet. Pick
a binding to change or unbind it, or a command to give it a key; **Record**
takes the next key you press. **New binding** starts an empty one.

A section decides where a key counts: `terminal` wins over `layout:<name>`,
which wins over `global`, and `mod:<name>` counts while that mod has focus.
**◆** marks a key another section also binds. The form refuses a key the app
cannot read and an action it does not know, and warns about a key that
types text, one programs rely on (ctrl+c, d, l, r, w, a, e, u, k, z and [),
ctrl+alt, which is AltGr on Windows, and a key a section that wins already
takes, as `terminal` takes ctrl+shift+c to copy. Saving a key another
binding holds replaces that binding, and the form says which before you
save. `app install` refuses a key a section that wins already takes.

## Mods

**Installed** lists the packages installed from here, the mods and layouts
added by hand, and the ones the app ships; a layout opens on click.
**Browse GitHub** searches repositories tagged `herdr-mod` or
`herdr-layout`, by stars or by last update. Type `owner/repo`, or
`owner/repo/folder` for one package of a repository that holds several, and
press **Enter** to review it directly.

Nothing is written before a review, which shows what the package adds: its
mods, layouts, themes and keys, and whether its code runs programs, reaches
the network or reads and writes files. A name another package or you already
use stops the install. A key you already use stays yours unless you tick
**Give these keys to the package**. The install takes the commit you
reviewed, or refuses if the repository moved on since, and each mod then
goes through the app's own check.

**Check for update** reviews a package again; one with files you changed
asks first. **Remove** takes it away with the keys it added; files you
changed stay. The record is `data/herdr-customize/packages.json`. Downloads
use `git`.

## Commands

```sh
herdr-customize app install [key]          # the themes, the mod and its key
herdr-customize app themes [--force]       # the palettes to the app again
herdr-customize app apply dracula-default  # show a theme in the app
herdr-customize app reset                  # back to graphite and paper
```

`--home DIR` before the command picks another app folder; the screen passes
the app's own. `apply` and `reset` need the app open. The screen reads and
writes through `herdr-customize app api`, which prints JSON and is not meant
to be typed.
