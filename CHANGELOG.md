# Changelog

release-please writes this file from the Conventional Commits on `main`;
do not edit it by hand. Versions follow semver and match the `version` field
in `Cargo.toml` and `herdr-plugin.toml`.

## [0.1.2](https://github.com/wapenshaw/herdr-customize/compare/v0.1.1...v0.1.2) (2026-10-25)


### Bug Fixes

* **pluginconfig:** decide a file's format before reading it ([abff8eb](https://github.com/wapenshaw/herdr-customize/commit/abff8eb3b98dee82fb234408fe7ea1b972512f3a))
* **ui:** install the terminal panic hook once ([6c8ea3d](https://github.com/wapenshaw/herdr-customize/commit/6c8ea3d0339dbb31a07e9fc73f490235514f9b2c))


### Performance

* **ui:** decode the config once per page rebuild ([8fd509e](https://github.com/wapenshaw/herdr-customize/commit/8fd509e2c08f1f7ad6c8d21d6b234fa673c6de7a))


### Refactoring

* **config:** share the keep-the-decor value replacement ([7b5d457](https://github.com/wapenshaw/herdr-customize/commit/7b5d457f83a99656a242a1e07d881dea02450ce7))
* **keys:** share the indexed suffix and clash lookup ([bba0ffb](https://github.com/wapenshaw/herdr-customize/commit/bba0ffbacee7e9ba0d080ede8d5606107f8d34d2))
* **pluginconfig:** merge entries in place and share the quote check ([16d5620](https://github.com/wapenshaw/herdr-customize/commit/16d5620502b3a0b13844cb46097f28569ac22870))
* **plugins:** share the JSON envelope and the GitHub URL ([9161497](https://github.com/wapenshaw/herdr-customize/commit/916149788756505267170a18c3212414a53a6315))
* **settings:** derive the pending path from its key ([b129d49](https://github.com/wapenshaw/herdr-customize/commit/b129d49e2fb166999a278194094492feab8ee812))
* **terminal:** replace dead-code allows with test cfgs ([f3f50d0](https://github.com/wapenshaw/herdr-customize/commit/f3f50d0b6bfb0dc9935cccb2f0c1affeb433b0ba))
* **theme:** keep token order, origin and role in one table ([c4362dd](https://github.com/wapenshaw/herdr-customize/commit/c4362ddae2d8ab2fb84991491a44b26d68ccecbd))
* **theme:** load a palette through one helper ([acbb14e](https://github.com/wapenshaw/herdr-customize/commit/acbb14ea470228e3bb62548c3587952b396339f7))
* **ui:** drop a pass-through, a dead branch and a reparse ([b4077f8](https://github.com/wapenshaw/herdr-customize/commit/b4077f8d6e3fab5aa3aadd825c628df6a705d1e4))
* **ui:** return the confirm directly from Confirm::new ([a8692d1](https://github.com/wapenshaw/herdr-customize/commit/a8692d1a6eba7a03bbb64bd40f68ad4389ab8210))
* **ui:** share the page dispatch and timer code ([c280db0](https://github.com/wapenshaw/herdr-customize/commit/c280db0ba60dfb9ac6a127d686533a5f9f73c33a))
* **ui:** take message payloads instead of cloning them ([d4a0f31](https://github.com/wapenshaw/herdr-customize/commit/d4a0f31af920687b90d3d99b4c77359c0b3d918f))
* **widgets:** share the column cut between clip and clip_str ([36c1559](https://github.com/wapenshaw/herdr-customize/commit/36c1559ed7fbb25f078cd0e34d6e4a6033a221ed))

## [0.1.1](https://github.com/wapenshaw/herdr-customize/compare/v0.1.0...v0.1.1) (2026-10-25)


### Refactoring

* **fsutil:** let localtime_r pick the time_t type ([be66fc9](https://github.com/wapenshaw/herdr-customize/commit/be66fc94a9e2c1c8c1e871d93cb2160d6679e343))

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
