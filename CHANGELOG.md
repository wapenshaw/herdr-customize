# Changelog

release-please writes this file from the Conventional Commits on `main`;
do not edit it by hand. Versions follow semver and match the `version` field
in `Cargo.toml` and `herdr-plugin.toml`.

## [0.4.0](https://github.com/wapenshaw/herdr-customize/compare/v0.3.0...v0.4.0) (2026-10-25)


### Features

* **ui:** open the popup on settings by default ([be510fb](https://github.com/wapenshaw/herdr-customize/commit/be510fb2c48a336027343d7e1f215f9e237d6574))
* **ui:** put settings first and lead the hints with tab ([cb64679](https://github.com/wapenshaw/herdr-customize/commit/cb6467937526522d74ba414f4b997649fe5632aa))

## [0.3.0](https://github.com/wapenshaw/herdr-customize/compare/v0.2.0...v0.3.0) (2026-10-25)


### Features

* **bind:** bind a key to the popup from the command line ([1028bc0](https://github.com/wapenshaw/herdr-customize/commit/1028bc02391f476a5cc388d91544281ebdbeaf91))


### Bug Fixes

* **scripts:** hash the windows download without get-filehash ([a68fa3c](https://github.com/wapenshaw/herdr-customize/commit/a68fa3cf6e139a33ebf58b0cd6c27fc677902c6e))


### Performance

* **scripts:** hide the windows download progress bar ([867ea5f](https://github.com/wapenshaw/herdr-customize/commit/867ea5f9d5e6a0ac97f006648d940d00a6e58b07))

## [0.2.0](https://github.com/wapenshaw/herdr-customize/compare/v0.1.2...v0.2.0) (2026-10-25)


### Bug Fixes

* **widgets:** keep the selected row when labels repeat ([ad2be4d](https://github.com/wapenshaw/herdr-customize/commit/ad2be4d495696efe60bc791fc3f3ba69f97f6e49))


### Performance

* **plugins:** check for updates on first view, not on open ([01c2d75](https://github.com/wapenshaw/herdr-customize/commit/01c2d75952de036cbf62ca9e76b79985c964e188))
* **settings:** reuse the check that ran during the save ([487d5aa](https://github.com/wapenshaw/herdr-customize/commit/487d5aa13eec68203ae4e3ca1a9cd4de85eea26a))
* **widgets:** rank list filters in process ([0b646a3](https://github.com/wapenshaw/herdr-customize/commit/0b646a3df87c97c686ef9f1f07a1d4e0631840aa))


### Refactoring

* drop unwraps that a pattern makes unneeded ([6406781](https://github.com/wapenshaw/herdr-customize/commit/640678108aa176102b3fba792b4c6efc6a1227ec))
* **fsutil:** format local time instead of exposing its fields ([b9bc052](https://github.com/wapenshaw/herdr-customize/commit/b9bc0525c8510644155d4379eb039cd5e382e1fc))
* **fsutil:** share the backup-then-write sequence ([3b7cc36](https://github.com/wapenshaw/herdr-customize/commit/3b7cc36c7d4608621fa135b45c57e5590af83c9c))
* **proc:** split platform PATH lookup into cfg items ([fb67a44](https://github.com/wapenshaw/herdr-customize/commit/fb67a44d9fa80337815d70d7b770f03a69f8fcbe))
* **schema:** own the installed-version lookup ([a032d99](https://github.com/wapenshaw/herdr-customize/commit/a032d997cd64f03e0613b805fa062408567e05dd))
* **schema:** write the cache atomically ([8bb283e](https://github.com/wapenshaw/herdr-customize/commit/8bb283e4caf9fd1c8e64897ba861a34b30c3c671))
* **settings:** make section headers a row variant ([788f91e](https://github.com/wapenshaw/herdr-customize/commit/788f91e5153762b9899d934ab02b0064755a1c23))
* **settings:** report saves without a relay message ([d673251](https://github.com/wapenshaw/herdr-customize/commit/d673251d2211a29c72a87b86443e452721a5e818))
* **theme:** check the palette size before reading it ([50dd0e4](https://github.com/wapenshaw/herdr-customize/commit/50dd0e49ec5db387dc51d23856e68b7a63f08482))
* **theme:** parse a palette once while resolving it ([7e78ded](https://github.com/wapenshaw/herdr-customize/commit/7e78ded700f0d7c987367c93149f11b82a4461e2))
* **theme:** share one hex color rule ([2b1c075](https://github.com/wapenshaw/herdr-customize/commit/2b1c075d9f3b72e4b50ea84f9394b158d200c383))
* **theme:** write the custom block through Store ([cef42e9](https://github.com/wapenshaw/herdr-customize/commit/cef42e9badc7445a7f250113e4e7c9d9cc3ecb79))
* **ui:** answer confirms in one place ([a73f71b](https://github.com/wapenshaw/herdr-customize/commit/a73f71b555b31a7193ca134c488d948928a84dfd))
* **ui:** borrow what the pages only read ([443ff4a](https://github.com/wapenshaw/herdr-customize/commit/443ff4a00906db1d19cb7dcaea2a9ec24add35d0))
* **ui:** carry anyhow errors through messages ([2596516](https://github.com/wapenshaw/herdr-customize/commit/259651654269e59ae62241694fff7d220c968f7b))
* **ui:** debounce picks through Ctx ([4adcb91](https://github.com/wapenshaw/herdr-customize/commit/4adcb91f6ac737565f11488c15c2f91d243c35e7))
* **ui:** keep only one cursor implementation ([6b3113a](https://github.com/wapenshaw/herdr-customize/commit/6b3113aba24791a9d8ee32e615dd39ea4bb0881d))
* **ui:** move value helpers out of the settings page ([757eba1](https://github.com/wapenshaw/herdr-customize/commit/757eba1762f6aef3cb708efcd8fe29ce554fc7c3))
* **ui:** name the filter groups and command fields as enums ([459d5ca](https://github.com/wapenshaw/herdr-customize/commit/459d5caeaaad9457b8a6c608a15edab62c5e15c4))
* **ui:** nest per-page messages ([e4538c4](https://github.com/wapenshaw/herdr-customize/commit/e4538c486b62e75174b5d16018afb1d966f4e2fe))
* **ui:** order plugin config writes with config saves ([aa95908](https://github.com/wapenshaw/herdr-customize/commit/aa95908798c1f4eb37021b51324320cf1251186a))
* **ui:** run the editor through one Ctx helper ([d506bc7](https://github.com/wapenshaw/herdr-customize/commit/d506bc768566457bc25544016d4c5f29f500024f))
* use language and library features stabilised since 1.89 ([11a4315](https://github.com/wapenshaw/herdr-customize/commit/11a43157a093e6a2b65c6daeab7ff4251ea4e60d))


### Documentation

* say what the plugin saves you from in its description ([4b41d25](https://github.com/wapenshaw/herdr-customize/commit/4b41d25c0ad357b879853f1585e8105b267b3782))

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
