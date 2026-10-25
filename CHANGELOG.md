# Changelog

All notable changes to this project are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com); versions follow semver and
match the `version` field in `herdr-plugin.toml`.

## [0.1.0] - Unreleased

- First release as `herdr-customize`, a standalone repository continuing
  [qintmb/herdr-theme-picker](https://github.com/qintmb/herdr-theme-picker)
  (its Go rewrite) under a new plugin id.
- Theme picker: 112 bundled palettes, live preview, user themes from the
  clipboard or an editor, `[theme.custom]` writes and terminal recoloring on
  Linux, macOS and Windows.
- State lives in the `herdr-customize` plugin state directory. Themes added
  with herdr-theme-picker are not imported automatically; copy its `themes/`
  folder and `index.txt` into the new state directory to keep them.
- The client override variable is now `HERDR_CUSTOMIZE_CLIENT_PID`.
