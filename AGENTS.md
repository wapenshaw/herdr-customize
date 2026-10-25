# Repository Guidelines

## Project Structure & Module Organization

This branch is the Rust implementation; `main` holds the Go original and
`docs/go-vs-rust.md` compares them.

- `src/main.rs`: CLI entrypoint and command routing.
- `src/ui/`: the ratatui popup. `mod.rs` holds the event loop, `Msg`, `Ctx`
  effects and the `Page` trait; `themes.rs`, `plugins.rs`, `keys.rs`,
  `settings.rs` are the pages, `pluginconfig.rs` the plugin settings screen,
  `widgets.rs` shared styles, input and lists, and `tests.rs` the UI driver.
- `src/config.rs`: in-place `config.toml` edits with `toml_edit`, `herdr config check` gating, backups, and reloads.
- `src/schema/`: Herdr's config reference (bundled `reference.json` plus per-version cache).
- `src/keys.rs`: key parsing, Herdr's binding rules, and conflict detection.
- `src/plugins/`: `herdr plugin` calls, update checks, marketplace search, and detached action runs.
- `src/pluginconfig.rs`: typed reading and editing of any plugin's TOML/`.env` config files, with an optional `config-schema.json`.
- `src/theme/`: palette loading, user themes, `[theme.custom]` writes, apply and sync, and the preview.
- `src/terminal/`: client discovery and OSC color writes/readback; `unix.rs` (ps, `/dev` TTYs, procargs) and `windows.rs` (Toolhelp, the `terminal-sync` console helper).
- `src/herdr.rs`, `src/fsutil.rs`, `src/proc.rs`: Herdr CLI wrapper, atomic writes and locks, and detached helpers.
- `themes/`: bundled Ghostty-format palettes and `index.txt`; `preview.png` is the preview asset.
- `herdr-plugin.toml`: plugin metadata, build command, panes, and actions. `docs/` records persistence edge cases and the language comparison.

## Build, Test, and Development Commands

Use Rust 1.89 or newer (edition 2024); on Windows, the MSVC toolchain with the Visual Studio Build Tools. Interactive development requires Herdr; plugin update checks use `git`. Linux, macOS and Windows are all supported; Windows support is the reason this project exists, so never drop it.

- `cargo build --release`: build `target/release/herdr-customize`, the binary the manifest runs.
- `cargo test`: run all unit and UI tests.
- `cargo clippy --all-targets`: static checks; keep it warning-free.
- `cargo fmt`: format (see `rustfmt.toml`).
- `herdr plugin link "$PWD"` followed by `herdr server reload-config`: link the checkout for local development.
- `./target/release/herdr-customize ui keys`: open the popup on a page (`themes`, `plugins`, `keys`, `settings`).
- `./target/release/herdr-customize preview dracula-default`: render a bundled palette preview.

Check platform-sensitive changes for every target: `cargo clippy --target x86_64-pc-windows-msvc` and `--target x86_64-unknown-linux-gnu` type-check from macOS (`ring` needs the target's C headers, so check a copy with ureq's `tls` feature off if they are missing). Native runtime checks are needed to validate terminal behavior; Windows steps are in `docs/windows-test-plan.md`.

## Coding Style & Naming Conventions

Follow `rustfmt` and clippy. Platform code uses `#[cfg(unix)]`/`#[cfg(windows)]` on items, or separate `unix.rs`/`windows.rs` modules when a whole implementation differs; Windows APIs come from `windows-sys`, Unix ones from `libc`. `snake_case` for functions and modules, `PascalCase` for types. Errors use `anyhow` with context; a type the caller must recognize (such as `CheckError`) is a struct it can `downcast_ref`. UI state changes only in `Page::update`/`key`; side effects go through `Ctx` (`spawn`, `after`, `exec`, `save`) so pages stay testable.

Theme filenames use lowercase hyphenated slugs, such as `catppuccin-mocha`, without extensions. Register bundled additions in `themes/index.txt`; include foreground, background, and all 16 ANSI palette entries.

## Testing Guidelines

Unit tests sit in `#[cfg(test)] mod tests` at the bottom of each module. UI tests live in `src/ui/tests.rs` and drive pages through its `Driver` (keys in, jobs run, screen out as text). No numerical coverage threshold is configured.

Use temporary directories and isolate state and config paths. The UI driver points `HERDR_BIN_PATH` at a scripted fake `herdr` that logs its calls, sets `HERDR_CONFIG_PATH`/`HERDR_PLUGIN_STATE_DIR` into a sandbox, points `HERDR_CUSTOMIZE_GIT_BASE`, `_RAW_BASE` and `_API_BASE` at dead local endpoints, and sets `VISUAL`/`EDITOR` to `false`. Tests that change the environment hold the driver's lock. Tests must avoid recoloring live terminals, reloading real Herdr sessions, installing or changing real plugins, reaching GitHub, or modifying the clipboard. Add regression coverage for changed behavior.

## Commit & Pull Request Guidelines

History commonly uses `feat:`, `fix:`, and optional scopes such as `fix(sync):`. Use concise, imperative subjects. PRs should explain the problem and resulting behavior, link relevant issues, list validation commands and platforms, and include screenshots for UI changes. Update README or persistence notes when behavior changes.
