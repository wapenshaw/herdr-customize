# herdr-customize

herdr-customize adds a Customize screen to the [Herdr app](https://herdr.dev) for themes, keys, mods, layouts, core and app settings, popups, and programs. It runs on Linux, macOS and Windows.

## Pages

| | |
|---|---|
| **Themes**: over 100 palettes with a live preview; one click makes one your light or dark theme.<br>![Themes: the palettes with a terminal drawn in the selected one's colors](docs/screenshots/app-themes.png) | **Keys**: every binding, a form to change or add one, and presets for common keys.<br>![Keys: the bindings with presets offering keys that are still free](docs/screenshots/app-keys.png) |
| **Mods**: what is installed, the mods bundled with Customize, and Browse for herdr-mod repositories on GitHub.<br>![Mods: this screen, mods added by hand, and the ones the app ships](docs/screenshots/app-mods.png) | **Layouts**: switch, copy and style layouts, or install one from the community gallery.<br>![Layouts: the built-in layouts with the active one selected](docs/screenshots/app-layouts.png) |
| **Core**: herdr-core's shell, scrollback and agent settings, and update checks.<br>![Core: updates, and the shell setting open with its hints](docs/screenshots/app-core.png) | **App**: the app's look, terminal and clipboard settings.<br>![App: settings by group, with zoom open](docs/screenshots/app-app.png) |
| **Popups**: a key that opens a command such as lazygit or btop in a floating terminal.<br>![Popups: the add form with examples, and a btop popup](docs/screenshots/app-popups.png) | **Programs**: rename, recolor or change the agents in the + menu, or add your own.<br>![Programs: the agents in the + menu with whether each is installed](docs/screenshots/app-programs.png) |

See [the app guide](docs/app.md) for details.

## Install

Clone the repository, fetch or build its binary, then install the screen:

```sh
git clone --depth 1 https://github.com/wapenshaw/herdr-customize ~/.local/share/herdr-customize
cd ~/.local/share/herdr-customize
sh scripts/fetch-or-build.sh
target/release/herdr-customize app install
```

On Windows, run `scripts\fetch-or-build.ps1` in PowerShell instead. The screen opens with **ctrl+shift+y** (**cmd+shift+y** on macOS). See [installing](docs/install.md) for requirements, updates and removal.

## Herdr TUI

The TUI plugin was retired in 1.0.0. Its last release was v0.10.0; pin it with `herdr plugin install wapenshaw/herdr-customize --ref v0.10.0`.

## Development

[Development](docs/development.md) · [Contributing](CONTRIBUTING.md)

[MIT License](LICENSE).
