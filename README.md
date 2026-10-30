# herdr-customize

Settings and key bindings for [Herdr](https://herdr.dev) in one popup instead
of hand-editing `config.toml`, with optional Themes and Plugins pages. Runs on
Linux, macOS and Windows.

This is a personal plugin. I wrote it for my own Herdr setup, and to
experiment and learn Rust along the way. The Themes and Plugins pages are Rust
ports of two community plugins (see [Credits](#credits)), brought into one
popup for my convenience. Use it if it helps you too.

![The Settings page: every Herdr setting grouped by section, with the allowed values on each row](docs/screenshots/settings.png)

## Features

- **Settings**: every setting your Herdr version has, grouped as in its docs,
  with the allowed values right on the row. Flip through them with ←/→.
- **Keys**: Herdr's actions, your custom commands and plugin actions that
  have no key yet, in one list. Press a key to record it; conflicts are
  flagged before you save.
- **Themes** (optional): 112 palettes with a live preview. One key recolors
  Herdr and the terminal it runs in, and the colors come back when Herdr
  restarts. Add your own from the clipboard.
- **Plugins** (optional): update, install, enable, remove and configure your
  plugins, read their logs, and browse the marketplace.
- **Safe edits**: your comments and layout stay put. Herdr checks every change
  before it is written, the original is backed up daily, and Herdr reloads
  by itself.

The popup starts with Settings and Keys. Turn on Themes and Plugins at the top
of the Settings page when you want them.

| Keys | Themes | Plugins |
| --- | --- | --- |
| ![Keys page flagging a conflict](docs/screenshots/keys.png) | ![Themes page with a live preview of the selected palette](docs/screenshots/themes.png) | ![Plugins page with an update available](docs/screenshots/plugins.png) |

## Install

```bash
herdr plugin install wapenshaw/herdr-customize
herdr plugin action invoke bind --plugin herdr-customize
```

The first command installs the prebuilt binary for your platform. The second
binds `prefix+t` to the popup. [Installing](docs/install.md) covers other
keys, binding by hand and building from source.

## Use

- **prefix+t** opens the popup.
- **Tab** and **Shift+Tab** switch pages, as do the number keys.
- **Themes page** and **Plugins page**, at the top of Settings, add those
  pages to the popup.
- **?** lists every key on a page, and **Esc** closes the popup.

To update, run `herdr plugin update herdr-customize`.

Coming from herdr-theme-picker or Plugin Manager? This plugin replaces both;
[here is how to switch](docs/install.md#coming-from-herdr-theme-picker-or-plugin-manager).

## Documentation

- [Installing](docs/install.md): platforms, key bindings, updating and the
  command line.
- [Using the popup](docs/guide.md): every page and its keys.
- [Themes](docs/themes.md): where a theme applies, terminal colors,
  `herdr --remote` and adding palettes.
- [Configuration and state](docs/configuration.md): how edits, checks,
  backups and locks work.
- [For plugin authors](docs/plugin-authors.md): describe your settings with
  `config-schema.json`.
- [Development](docs/development.md) and [Contributing](CONTRIBUTING.md).

## Ideas

Sidebar row layouts with a preview, saved pane layouts, tab bar and window
title builders, sounds and notifications, and restoring config backups.

## Credits

- The Themes page is a Rust port of
  [qintmb/herdr-theme-picker](https://github.com/qintmb/herdr-theme-picker).
  Thanks to its contributors.
- The Plugins page is a Rust port of
  [speardragon/herdr-plugin-manager](https://github.com/speardragon/herdr-plugin-manager)
  (MIT).

## License

MIT; see [LICENSE](LICENSE).
