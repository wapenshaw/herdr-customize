# Customize for the Herdr app

A screen over the Herdr app, on **ctrl+shift+y** (**cmd+shift+y** on
macOS). It has three pages: every palette as a theme with a preview, the
app's keys with a form to change them, and mods and layouts from GitHub,
with a review before anything is installed.

| Themes | Keys | Mods |
| --- | --- | --- |
| ![Themes: the palettes with a terminal drawn in the selected one's colors](../docs/screenshots/app-themes.png) | ![Keys: a binding open in the form, with its command's description](../docs/screenshots/app-keys.png) | ![Mods: packages from herdr-mod repositories, each with its picture, name, stars and Review](../docs/screenshots/app-mods.png) |

## Add it

Ask an agent running in the app:

```text
Read ~/.config/herdr-app/AGENTS.md, then add this mod: github.com/wapenshaw/herdr-customize/tree/main/app
```

The mod is the screen of the herdr-customize program and does nothing
without it, so the [recipe](RECIPE.md) has the agent set up the program
rather than copy these files. To do it by hand:

```sh
git clone --depth 1 https://github.com/wapenshaw/herdr-customize ~/.local/share/herdr-customize
cd ~/.local/share/herdr-customize
sh scripts/fetch-or-build.sh          # Windows: powershell -File scripts/fetch-or-build.ps1
target/release/herdr-customize app install
```

The [guide](../docs/app.md) covers each page and the commands.
