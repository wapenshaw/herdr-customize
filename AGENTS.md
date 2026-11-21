# Repository Guidelines

The program is written in Rust. The first, Go implementation is kept on the
`go` branch, and `docs/go-vs-rust.md` compares the two.

- `src/main.rs`: CLI entrypoint, `env()` and command routing.
- `src/app/`: the app's themes, keys, mods and layouts, app and core settings,
  and the JSON API used by the Customize screen; see `docs/app.md`.
- `src/app/market.rs`: GitHub topic search.
- `src/theme.rs`: bundled palette loading from `themes/` and `themes/index.txt`.
- `src/fsutil.rs`: atomic writes and locks.
- `src/proc.rs`: `output_within` and `look_path`.
- `src/testenv.rs`: the isolated test sandbox and fake `herdr-app`.
- `app/`: the Customize screen as a mod package; `app install` embeds its
  files and fills in the binary's path.
- `themes/`: bundled Ghostty-format palettes and `index.txt`.
- `scripts/fetch-or-build.{sh,ps1}`: downloads the release binary for this
  checkout's version or falls back to `cargo build`.
- `docs/screenshots/`: screenshots of the app screen.
- `docs/`: app usage, installation, development and history.

## Build, Test, and Development Commands

Use Rust 1.99 or newer (edition 2024); interactive development requires
`herdr-app`. Windows needs the MSVC toolchain only when building from source.
Git 2.25 or newer is used for repository operations. Linux, macOS and Windows
are all supported; Windows support is the reason this project exists, so
never drop it.

- `cargo build --release`: build `target/release/herdr-customize`.
- `cargo test`: run unit and app API tests in the isolated sandbox.
- `cargo clippy --all-targets`: static checks; keep it warning-free.
- `cargo fmt`: format (see `rustfmt.toml`).
- `./target/release/herdr-customize app install`: install the screen into the
  configured app home.
- For app mods, `herdr-app check <mod folder>` checks a package, and
  `herdr-app status` reports the running app.

Check platform-sensitive changes for every target:
`cargo clippy --target x86_64-pc-windows-msvc` and
`--target x86_64-unknown-linux-gnu` type-check from macOS (`ring` needs the
target's C headers, so check a copy with ureq's `rustls` feature off if they
are missing).

## Language rule (mandatory)

**Everything written into this repository is in English.** Code comments, commit messages, log and error messages, documentation, test names: all English, whatever language the request was made in. Only the conversation with the user follows the user's language.

## Type rule (mandatory)

**A struct field exists only if code reads it.** The app's JSON carries far
more than this program needs; mirroring it in full claims a dependency the
code does not have, and every unread field is a promise to keep something
working that nothing exercises. Add a field when the code that reads it lands
in the same change, and delete it when its last reader goes. The same holds
for methods and constants.

## Comment rule (mandatory)

**A comment is at most three lines**, and it says what is surprising rather than what is visible. A decision that needs a paragraph goes in `docs/`, with one line in the code pointing at it. Every `unsafe` block carries a `// SAFETY:` line, and every `#[allow]` gives a `reason`. The rule in full, with worked examples, is in [.claude/rules/comments.md](.claude/rules/comments.md).

## Coding Style & Naming Conventions

Follow `rustfmt` and clippy. Platform code uses `#[cfg(unix)]`/`#[cfg(windows)]`
on items, or separate `unix.rs`/`windows.rs` modules when a whole
implementation differs. The standard library covers every platform call the
program makes today; add `windows-sys` or `libc` only for one it lacks.
`snake_case` for functions and modules, `PascalCase` for types. Errors
use `anyhow` with context; a type the caller must recognize is a struct it can
`downcast_ref`. Keep app reads and writes behind the app API so they stay
testable.

Theme filenames use lowercase hyphenated slugs, such as `catppuccin-mocha`,
without extensions. Register bundled additions in `themes/index.txt`; include
foreground, background and all 16 ANSI palette entries.

## Testing Guidelines

Unit tests sit in `#[cfg(test)] mod tests` at the bottom of each module. App
tests use `crate::testenv::isolated()`, which creates a temporary sandbox and
a fake `herdr-app` script selected through `HERDR_APP_BIN`. It points `HOME`,
`XDG_CONFIG_HOME` and `APPDATA` into the sandbox, sets
`HERDR_CUSTOMIZE_GIT_BASE`, `_RAW_BASE` and `_API_BASE` to dead endpoints, and
sets `VISUAL` and `EDITOR` to `false`.

Tests must avoid touching the real app home, reaching GitHub or using the
clipboard. Add regression coverage for changed behavior.

## Releases

release-please (`release-please-config.json`, `.release-please-manifest.json`)
owns versions, tags and `CHANGELOG.md`: it bumps `Cargo.toml` and `Cargo.lock`
in its release pull request, so never edit them by hand. Its pull request is
opened by `github-actions[bot]`, so its CI waits for the maintainer to approve
the run, and the maintainer merges it through the admin bypass. Merging that
pull request tags `vX.Y.Z` and opens a draft release; `.github/workflows/release.yml`
builds the binaries, attaches them with `SHA256SUMS` and publishes the draft.
`scripts/fetch-or-build.*` download the asset for the checkout's version and
verify it against `SHA256SUMS`; asset names are `herdr-customize-<target>[.exe]`,
so keep the scripts and the workflow matrix in sync. Test the scripts with
`HERDR_CUSTOMIZE_RELEASE_BASE` pointing at a local server and a fake `cargo`
on `PATH`. Run the Windows script in Windows PowerShell 5.1 and pwsh 7, and
also in 5.1 with pwsh 7's `PSModulePath` inherited: 5.1 can fail to load
`Microsoft.PowerShell.Utility`, leaving `Get-FileHash` unavailable. pwsh 7
resets that variable for a child it starts itself, so start that case through
`System.Diagnostics.ProcessStartInfo`.

CI runs on its own only for the maintainer's (`wapenshaw`) pull requests; everyone else's waits for approval, at the fork-run prompt or at the `Approval` job's `ci-approval` environment. GitHub Actions minutes are limited. Never start a workflow (a manual run, a pushed tag, or a pull request) unless the maintainer asks for it; check locally first.

## Commit & Pull Request Guidelines

Follow [CONTRIBUTING.md](CONTRIBUTING.md); these rules are mandatory.

- Never commit to `main`. Branch as `<type>/<kebab-summary>`.
- Conventional Commits: `<type>(<scope>): <subject>`, imperative, lowercase, no trailing period, 72 characters at most. Types: `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `ci`, `chore`; scope is a module or area, or `deps`. The body says why. One logical change per commit, never a co-author trailer, and no "Generated with" line in commits or pull request bodies.
- Pull requests are rebased, never squashed or merged, so each commit subject becomes a changelog line; `feat`, `fix`, `perf` and `refactor` are the visible ones.
- One thing per pull request. The body follows `.github/pull_request_template.md`: Summary (the smallest view that shows the change), Evidence (before and after), Merge Danger (one-way or two-way door, blast radius), and the checklist with only true items ticked. Include screenshots for UI changes.
- Update the guide in `docs/` when behavior changes, and the README only when a feature or the install steps change. The README stays a short landing page: features, install, a few keys and links; leave `CHANGELOG.md` and versions to release-please.

