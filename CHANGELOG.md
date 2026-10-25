# Changelog

All notable changes to this project are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com); versions follow semver and
match the `version` field in `herdr-plugin.toml`.

## [0.1.0] - Unreleased

- First release as `herdr-customize`, a standalone repository continuing
  [qintmb/herdr-theme-picker](https://github.com/qintmb/herdr-theme-picker)
  (its Go rewrite) under a new plugin id.
- One Bubble Tea popup with four pages, replacing the fzf picker; fzf is no
  longer needed. Panes and actions open it on any page.
- Themes: 112 bundled palettes, live preview, user themes from the clipboard
  or an editor, a token editor with live preview, `[theme.custom]` writes and
  terminal recoloring on Linux, macOS and Windows.
- Plugins: a Go port of
  [speardragon/herdr-plugin-manager](https://github.com/speardragon/herdr-plugin-manager):
  update checks, single and bulk updates through Herdr's trust prompt,
  install, enable/disable, uninstall/unlink, marketplace, command logs, and
  running actions after the popup closes.
- Plugin settings: **s** on a plugin edits the TOML and `.env` files in its
  config directory with types inferred from values and commented-out
  defaults, or taken from a `config-schema.json` the plugin ships.
- Keys: built-in bindings, custom `[[keys.command]]` entries and unbound
  plugin actions in one list, with key recording, conflict detection and
  Herdr's binding rules.
- Settings: an editor generated from Herdr's config reference for the
  installed version, plus checked whole-file editing.
- Config edits keep comments and layout, are refused when `herdr config check`
  reports a new problem, back up once a day, and reload Herdr.
- State lives in the `herdr-customize` plugin state directory. User themes
  from herdr-theme-picker can be imported from the Themes page or with
  `herdr-customize import-themes`.
- The client override variable is now `HERDR_CUSTOMIZE_CLIENT_PID`.
