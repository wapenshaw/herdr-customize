# Changelog

All notable changes to this project are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com); versions follow semver and
match the `version` field in `herdr-plugin.toml`.

## [0.1.0] - 2026-10-25

- First release as `herdr-customize`, a standalone repository continuing
  [qintmb/herdr-theme-picker](https://github.com/qintmb/herdr-theme-picker)
  under a new plugin id. Written in Rust (ratatui, crossterm, toml_edit); the
  first Go implementation is kept on the `go` branch, and
  [docs/go-vs-rust.md](docs/go-vs-rust.md) compares them.
- Linux, macOS and Windows. Installs download a checksummed release binary
  for the platform and fall back to `cargo build` when there is none.
- One popup with four pages, opened on any page from panes and actions; fzf
  is optional and only ranks searches.
- Themes: 112 bundled palettes, live preview, your own themes from the
  clipboard or an editor, a token editor with live preview, `[theme.custom]`
  writes and terminal recoloring. Themes are grouped into in use, yours, dark
  and light with a color strip each; searching starts as you type.
- Plugins: a port of
  [speardragon/herdr-plugin-manager](https://github.com/speardragon/herdr-plugin-manager):
  update checks, single and bulk updates through Herdr's trust prompt,
  install, enable/disable, uninstall/unlink, marketplace, command logs, and
  running actions after the popup closes, in a list with a panel per plugin.
- Plugin settings: **s** on a plugin edits the TOML and `.env` files in its
  config directory, typed from their values and commented-out defaults, or
  from a `config-schema.json` the plugin ships.
- Keys: built-in bindings, custom `[[keys.command]]` entries and unbound
  plugin actions in one list, with key recording, conflict detection and
  Herdr's binding rules.
- Settings: an editor generated from Herdr's config reference for the
  installed version, plus checked whole-file editing.
- Config edits keep comments and layout (inline tables and dotted keys
  included), are refused when `herdr config check` reports a new problem,
  back up once a day, and reload Herdr.
- State lives in the `herdr-customize` plugin state directory. The client
  override variable is `HERDR_CUSTOMIZE_CLIENT_PID`.
