# Development

Branches, commits, pull requests and releases are described in
[Contributing](../CONTRIBUTING.md).

```sh
cargo test
cargo clippy --all-targets
cargo fmt
cargo build --release
```

The binary's user-facing commands are under `app`; see [the app guide](app.md).
To install the screen from a local release build into the configured app home:

```sh
./target/release/herdr-customize app install
```

## Layout

- `src/main.rs`: CLI routing.
- `src/app/`: app themes, keys, mods and layouts, core settings, and the JSON
  API the screen uses.
- `src/app/market.rs`: GitHub topic search.
- `src/theme.rs`: bundled palette loading.
- `src/testenv.rs`: isolated test sandbox, including a fake `herdr-app`.
- `src/fsutil.rs`: atomic writes and locks.
- `src/proc.rs`: `output_within` and `look_path`.
- `app/`: the Customize screen as a mod package; `app install` embeds its
  files and fills in the binary's path.
- `themes/`: bundled palettes and `index.txt`.
- `scripts/fetch-or-build.*`: downloads the release binary for this checkout's
  version or builds it from source.

## App screen pages

`app/mods/herdr-customize/pages.js` is the page registry: add a page module
and one numbered re-export there. Exports sort by name, determining tab order.
Each factory receives the per-view `app` and returns an object with `id`,
`title`, `init(cx)`, `render()` and `focus()`; `show(cx)` and `key(event, cx)`
are optional. `init` runs once per view. Add the module to `POPUP_FILES` in
`src/app/mod.rs` so `app install` embeds it.

Pages created with `page(app, instance)` from `lib.js` keep their own state on
the instance and inherit the shell's helpers, including `call`, `load`, `task`,
the drawing helpers, `focusPage`, and `closeAndRun`. `busy`, `error` and
`notice` are shared with the shell. Never retain `cx`; use the context passed
to a callback. Keep shared constants and pure helpers in `lib.js`.

For APIs absent from older app releases, use a namespace import and detect the
export before calling it. In particular, `checking()` falls back to a missing
window on older releases so the checker never starts the binary.

## Bundled palettes

Palettes in `themes/` use the Ghostty palette format and are registered in
`themes/index.txt`. They ship beside the binary, so keep that folder with the
checkout. The bundled set comes from [terminalcolors.com](https://terminalcolors.com)
and works offline. A palette has `background`, `foreground` and `palette`
entries 0 through 15 as six-digit hex colors; names are lowercase hyphenated
slugs.

## Tests

Unit tests sit in `#[cfg(test)] mod tests` at the bottom of each module. App
tests use `crate::testenv::isolated()`: it creates a temporary sandbox, a fake
`herdr-app` script selected through `HERDR_APP_BIN`, and points `HOME`,
`XDG_CONFIG_HOME` and `APPDATA` into the sandbox. It sets
`HERDR_CUSTOMIZE_GIT_BASE`, `_RAW_BASE` and `_API_BASE` to dead endpoints, and
sets `VISUAL` and `EDITOR` to `false`.

Tests must not touch the real app home, reach GitHub or use the clipboard.
When driving the real app on Windows, use `herdr-app check <mod folder>` to
check the screen and `herdr-app status` to inspect the running app.

## Release scripts

`release-please` cuts releases from Conventional Commits on `main`, bumps
`Cargo.toml` and `Cargo.lock`, and writes `CHANGELOG.md`. Merging its pull
request tags `vX.Y.Z` and opens a draft release. The release workflow builds
the binaries listed in [Installing](install.md), attaches them with
`SHA256SUMS` and publishes the draft. `scripts/fetch-or-build.*` download the
asset for the checkout's version and verify its checksum. Asset names are
`herdr-customize-<target>[.exe]`; keep them in sync with the scripts.

Test the scripts with `HERDR_CUSTOMIZE_RELEASE_BASE` pointing at a local
server and a fake `cargo` on `PATH`. Run the PowerShell script in Windows
PowerShell 5.1 and pwsh 7, and in 5.1 with pwsh 7's `PSModulePath` inherited.
Herdr started from pwsh 7 invokes the script in 5.1; that inherited path can
make 5.1 fail to load `Microsoft.PowerShell.Utility`, leaving `Get-FileHash`
unavailable. pwsh 7 resets the variable for a child it starts itself, so
start the 5.1 case with `System.Diagnostics.ProcessStartInfo`.

Running the release workflow by hand builds every target without releasing
anything. Given an existing release tag, it rebuilds that release's binaries
and replaces them.

## Go version

herdr-customize was first written in Go. That version is kept on the
[`go`](https://github.com/wapenshaw/herdr-customize/tree/go) branch, and
[go-vs-rust.md](go-vs-rust.md) compares the implementations.
