# Development

How changes are made here (branches, commits, pull requests and releases) is
in [CONTRIBUTING.md](../CONTRIBUTING.md).

```sh
cargo test
cargo clippy --all-targets
cargo fmt
cargo build --release
```

`herdr plugin link` skips the build step, so build before linking a checkout:

```bash
git clone https://github.com/wapenshaw/herdr-customize.git
cd herdr-customize
cargo build --release
herdr plugin link "$PWD"
herdr server reload-config
```

## Layout

- `src/main.rs`: CLI routing.
- `src/bind.rs`: `bind`, which adds the first key binding for the popup.
- `src/ui/`: the popup. `mod.rs` holds the event loop, `Msg`, `Ctx` and the
  `Page` trait. The four pages, the plugin settings screen and shared widgets
  sit beside it, along with `tests.rs`, a driver that runs the popup without a
  terminal.
- `src/config.rs`: in-place `config.toml` edits with `toml_edit`, checks,
  backups and reloads.
- `src/schema/`: Herdr's config reference, bundled plus a per-version cache.
- `src/keys.rs`: key parsing, Herdr's binding rules and conflicts.
- `src/plugins/`: `herdr plugin` calls, update checks and marketplace search.
- `src/pluginconfig.rs`: typed reading and editing of plugin TOML and `.env`
  files.
- `src/theme/`: palettes, your themes, `[theme.custom]` writes, apply, sync
  and the preview.
- `src/terminal/`: finding Herdr clients and recoloring them. `unix.rs` writes
  to the client's TTY; `windows.rs` runs a helper that attaches to the
  client's console.
- `src/herdr.rs`, `src/fsutil.rs`, `src/proc.rs`: the Herdr CLI, atomic
  writes and locks, and detached helpers.
- `src/app/`: the Herdr desktop app's theme files, `keys.json` and mods,
  and `api.rs`, the JSON commands the app's screen reads and writes through.
- `app/`: the app's screen, a mod drawn with the app's own controls, laid
  out as a package other people's agents can add; `app install` embeds it.
- `scripts/fetch-or-build.*`: the install step that downloads a release
  binary or builds from source.

## One binary for Herdr and the app

Herdr in the terminal and the Herdr desktop app share one executable; the
app's commands sit under `herdr-customize app`. Both use the same palettes,
key rules and marketplace code, and one binary keeps a single release,
one set of assets for `scripts/fetch-or-build.*` and one version. The
palettes ship beside the binary, which both sides need.

The two sides stay apart in the code: `src/app/` never writes Herdr's
`config.toml`, and nothing outside it touches the app's folder. The app's
screen keeps no logic of its own beyond what it shows, so the tests in
`api.rs` cover it. Splitting the binary is worth it only if the
app gets an install channel that cannot run a command and keep a folder.

## Tests

Tests must not recolor live terminals, reload a real Herdr session or touch
the real clipboard. The UI tests point every path into a temporary sandbox and
run a scripted `herdr` (a shell script, or a batch file on Windows). Terminal
emulator and SSH behavior still needs checking by hand on each OS; the Windows
steps are in [the Windows test plan](windows-test-plan.md).

## Releases

[release-please](https://github.com/googleapis/release-please) cuts releases
from the Conventional Commits on `main`. It keeps a release pull request open
that bumps the version in `Cargo.toml`, `Cargo.lock` and `herdr-plugin.toml`
and writes `CHANGELOG.md`. Merging that pull request tags `vX.Y.Z` and opens
a draft release. The release workflow then builds the binaries listed in
[Installing](install.md), attaches them with `SHA256SUMS` and publishes the
draft, so an install never finds a release without its binaries.

Running the release workflow by hand builds every target without releasing
anything. Given the tag of an existing release, it rebuilds that release's
binaries and replaces them.

## Go version

herdr-customize was first written in Go. That version is kept on the
[`go`](https://github.com/wapenshaw/herdr-customize/tree/go) branch, and
[go-vs-rust.md](go-vs-rust.md) compares the two implementations: startup
time, memory, binary size, build times and how the code differs.
