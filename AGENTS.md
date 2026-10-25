# Repository Guidelines

## Project Structure & Module Organization

The plugin is written in Rust. The first, Go implementation is kept on the
`go` branch, and `docs/go-vs-rust.md` compares the two.

- `src/main.rs`: CLI entrypoint and command routing.
- `src/bind.rs`: `bind`, the first key binding for the popup, since Herdr binds no keys from plugin manifests.
- `src/ui/`: the ratatui popup. `mod.rs` holds the event loop, `Msg`, `Ctx`
  effects and the `Page` trait; `themes.rs`, `plugins.rs`, `keys.rs`,
  `settings.rs` are the pages, `pluginconfig.rs` the plugin settings screen,
  `values.rs` the value editing both settings screens share, `widgets.rs` shared styles, input and lists, and `tests.rs` the UI driver.
- `src/config.rs`: in-place `config.toml` edits with `toml_edit`, `herdr config check` gating, backups, and reloads.
- `src/schema/`: Herdr's config reference (bundled `reference.json` plus per-version cache).
- `src/keys.rs`: key parsing, Herdr's binding rules, and conflict detection.
- `src/plugins/`: `herdr plugin` calls, update checks, marketplace search, and detached action runs.
- `src/pluginconfig.rs`: typed reading and editing of any plugin's TOML/`.env` config files, with an optional `config-schema.json`.
- `src/theme/`: palette loading, user themes, `[theme.custom]` writes, apply and sync, and the preview.
- `src/terminal/`: client discovery and OSC color writes/readback; `unix.rs` (ps, `/dev` TTYs, procargs) and `windows.rs` (Toolhelp, the `terminal-sync` console helper).
- `src/herdr.rs`, `src/fsutil.rs`, `src/proc.rs`: Herdr CLI wrapper, atomic writes and locks, and starting other programs (detached helpers, the browser, the editor).
- `themes/`: bundled Ghostty-format palettes and `index.txt`.
- `scripts/fetch-or-build.{sh,ps1}`: the manifest's build step; downloads the release binary for this version or falls back to `cargo build`.
- `docs/screenshots/`: README images, rendered from a sandboxed run of the popup.
- `herdr-plugin.toml`: plugin metadata, build command, panes, and actions. `docs/` records persistence edge cases and the language comparison.

## Build, Test, and Development Commands

Use Rust 1.99 or newer (edition 2024); on Windows, the MSVC toolchain with the Visual Studio Build Tools. Interactive development requires Herdr; plugin update checks use `git`. Linux, macOS and Windows are all supported; Windows support is the reason this project exists, so never drop it.

- `cargo build --release`: build `target/release/herdr-customize`, the binary the manifest runs. `herdr plugin link` skips the build step, so build before linking.
- `cargo test`: run all unit and UI tests.
- `cargo clippy --all-targets`: static checks; keep it warning-free.
- `cargo fmt`: format (see `rustfmt.toml`).
- `herdr plugin link "$PWD"` followed by `herdr server reload-config`: link the checkout for local development.
- `herdr plugin action invoke <id> --plugin herdr-customize`: run an action. It returns as soon as the action starts, and the action's output reaches only `herdr plugin log list`, so an action that must tell the user something uses `herdr notification show`. Herdr likewise shows a build step's output only when it fails.
- `./target/release/herdr-customize ui keys`: open the popup on a page (`settings`, `keys`, `themes`, `plugins`).
- `./target/release/herdr-customize preview dracula-default`: render a bundled palette preview.

Check platform-sensitive changes for every target: `cargo clippy --target x86_64-pc-windows-msvc` and `--target x86_64-unknown-linux-gnu` type-check from macOS (`ring` needs the target's C headers, so check a copy with ureq's `rustls` feature off if they are missing). Native runtime checks are needed to validate terminal behavior; Windows steps are in `docs/windows-test-plan.md`.

## Language rule (mandatory)

**Everything written into this repository is in English.** Code comments, commit messages, log and error messages, documentation, test names: all English, whatever language the request was made in. Only the conversation with the user follows the user's language.

## Type rule (mandatory)

**A struct field exists only if code reads it.** Herdr's JSON carries far more than this plugin needs; mirroring it in full claims a dependency the code does not have, and every unread field is a promise to keep something working that nothing exercises. Add a field when the code that reads it lands in the same change, and delete it when its last reader goes. The same holds for methods, constants and `Msg` variants.

## Comment rule (mandatory)

**A comment is at most three lines**, and it says what is surprising rather than what is visible. A decision that needs a paragraph goes in `docs/`, with one line in the code pointing at it. Every `unsafe` block carries a `// SAFETY:` line, and every `#[allow]` gives a `reason`. The rule in full, with worked examples, is in [.claude/rules/comments.md](.claude/rules/comments.md).

## Coding Style & Naming Conventions

Follow `rustfmt` and clippy. Platform code uses `#[cfg(unix)]`/`#[cfg(windows)]` on items, or separate `unix.rs`/`windows.rs` modules when a whole implementation differs; Windows APIs come from `windows-sys`, Unix ones from `libc`. `snake_case` for functions and modules, `PascalCase` for types. Errors use `anyhow` with context; a type the caller must recognize (such as `CheckError`) is a struct it can `downcast_ref`. UI state changes only in `Page::update`/`key`; side effects go through `Ctx` (`spawn`, `after`, `exec`, `save`) so pages stay testable.

Theme filenames use lowercase hyphenated slugs, such as `catppuccin-mocha`, without extensions. Register bundled additions in `themes/index.txt`; include foreground, background, and all 16 ANSI palette entries.

## Testing Guidelines

Unit tests sit in `#[cfg(test)] mod tests` at the bottom of each module. UI tests live in `src/ui/tests.rs` and drive pages through its `Driver` (keys in, jobs run, screen out as text). No numerical coverage threshold is configured.

Use temporary directories and isolate state and config paths. The UI driver points `HERDR_BIN_PATH` at a scripted fake `herdr` that logs its calls, sets `HERDR_CONFIG_PATH`/`HERDR_PLUGIN_STATE_DIR` into a sandbox, points `HERDR_CUSTOMIZE_GIT_BASE`, `_RAW_BASE` and `_API_BASE` at dead local endpoints, and sets `VISUAL`/`EDITOR` to `false`. Tests that change the environment hold the driver's lock. Tests must avoid recoloring live terminals, reloading real Herdr sessions, installing or changing real plugins, reaching GitHub, or modifying the clipboard. Add regression coverage for changed behavior.

## Releases

release-please (`release-please-config.json`, `.release-please-manifest.json`) owns versions, tags and `CHANGELOG.md`: it bumps `Cargo.toml`, `Cargo.lock` and `herdr-plugin.toml` in its release pull request, so never edit any of them by hand. Its pull request is opened by `github-actions[bot]`, so its CI waits for the maintainer to approve the run, and the maintainer merges it through the admin bypass. Merging that pull request tags `vX.Y.Z` and opens a draft release; `.github/workflows/release.yml` builds the binaries, attaches them with `SHA256SUMS` and publishes the draft. `scripts/fetch-or-build.*` download the asset for the checkout's version and verify it against `SHA256SUMS`; asset names are `herdr-customize-<target>[.exe]`, so keep the scripts and the workflow matrix in sync. Test the scripts with `HERDR_CUSTOMIZE_RELEASE_BASE` pointing at a local server and a fake `cargo` on `PATH`. Run the Windows one in Windows PowerShell 5.1 and in pwsh 7, and once in 5.1 with pwsh 7's `PSModulePath` inherited, as Herdr started from pwsh 7 runs it: there 5.1 picks pwsh 7's `Microsoft.PowerShell.Utility`, cannot load it, and `Get-FileHash` is missing. pwsh 7 resets that variable for a `powershell` it starts itself, so start that case through `System.Diagnostics.ProcessStartInfo`.

CI runs on its own only for the maintainer's (`wapenshaw`) pull requests; everyone else's waits for approval, at the fork-run prompt or at the `Approval` job's `ci-approval` environment. GitHub Actions minutes are limited. Never start a workflow (a manual run, a pushed tag, or a pull request) unless the maintainer asks for it; check locally first.

## Commit & Pull Request Guidelines

Follow [CONTRIBUTING.md](CONTRIBUTING.md); these rules are mandatory.

- Never commit to `main`. Branch as `<type>/<kebab-summary>`.
- Conventional Commits: `<type>(<scope>): <subject>`, imperative, lowercase, no trailing period, 72 characters at most. Types: `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `ci`, `chore`; scope is a module or area, or `deps`. The body says why. One logical change per commit, never a co-author trailer, and no "Generated with" line in commits or pull request bodies.
- Pull requests are rebased, never squashed or merged, so each commit subject becomes a changelog line; `feat`, `fix`, `perf` and `refactor` are the visible ones.
- One thing per pull request. The body follows `.github/pull_request_template.md`: Summary (the smallest view that shows the change), Evidence (before and after), Merge Danger (one-way or two-way door, blast radius), and the checklist with only true items ticked. Include screenshots for UI changes.
- Update README or persistence notes when behavior changes; leave `CHANGELOG.md` and versions to release-please.

