# Contributing to herdr-customize

What the plugin does and how to install it are in the [README](README.md).
This is how to work on it.

## Before you start

- Open an issue first for anything non-trivial, and settle the scope there
  before you spend an evening on it.
- Small fixes go straight to a pull request: a typo, an obvious bug, a
  dependency bump.
- Anything with security consequences goes through
  [SECURITY.md](SECURITY.md), not into an issue.

## Getting set up

You need Rust 1.99 or newer and Herdr. On Windows, use the MSVC toolchain with
the Visual Studio Build Tools.

```sh
git clone https://github.com/wapenshaw/herdr-customize.git
cd herdr-customize
cargo test
cargo build --release
herdr plugin link "$PWD"     # link skips the build step, so build first
herdr server reload-config
```

> [!WARNING]
> A linked checkout edits your **real** `config.toml`, recolors your real
> terminals and installs real plugins. The tests never do: they run against a
> scripted `herdr` in a temporary sandbox. Keep it that way when you add one.

## Working on a change

- Branch from `main`, named `<type>/<kebab-summary>`, such as
  `fix/settings-flicker`.
- **Everything written into this repository is in English**: comments, commit
  messages, log and error text, documentation, test names.
- Commits follow [Conventional Commits](https://www.conventionalcommits.org):
  `<type>(<scope>): <subject>`, imperative and lowercase, no trailing period,
  72 characters at most. Types in use are `feat`, `fix`, `perf`, `refactor`,
  `docs`, `test`, `ci` and `chore`; the scope is a module or area (`themes`,
  `plugins`, `keys`, `settings`, `config`, `terminal`, `sync`), or `deps` for
  a dependency update. The body explains **why**; the diff already says what.
  One logical change per commit, and never a co-author trailer.
- `feat`, `fix`, `perf` and `refactor` commits become the changelog, so write
  their subjects for someone who uses the plugin. A dependency update is
  `chore(deps)` whatever it updates, so it cuts no release on its own.
- Tests land with the behaviour they cover.
- **A struct field exists only if code reads it.** Herdr's JSON (plugin lists,
  the config reference) carries far more than this plugin needs, and an unread
  field is a promise to keep something working that nothing exercises.
- A comment is at most three lines, and it explains what is surprising rather
  than what is visible. A decision that needs a paragraph goes into `docs/`
  with one line in the code pointing at it. The full rule is in
  [.claude/rules/comments.md](.claude/rules/comments.md).
- Never pass a value from a terminal, a theme file or a plugin manifest to a
  shell. Run programs with argument lists, as `src/proc.rs` and `src/herdr.rs`
  do.
- Check Herdr's behaviour instead of assuming it: its source,
  `herdr --help` and the config reference it publishes. Record what you learn
  in `docs/` when it is not obvious from the code.
- Windows, macOS and Linux are all supported, and Windows is the reason this
  project exists. Platform code goes behind `#[cfg(unix)]`/`#[cfg(windows)]`,
  and a change to it type-checks for every target:
  `cargo clippy --all-targets --target x86_64-pc-windows-msvc` and
  `--target x86_64-unknown-linux-gnu`. Terminal behaviour needs a run on the
  OS itself; the Windows steps are in
  [docs/windows-test-plan.md](docs/windows-test-plan.md).

## Pull requests

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and
  `cargo test` pass locally. CI then runs the same on Linux, macOS and
  Windows, plus a build on Rust 1.99.
- CI runs on its own only for the maintainer's pull requests. Anyone else's
  waits for approval: a fork's run is approved from the pull request, and a
  branch in this repository (Renovate's, a collaborator's) waits at the
  `Approval` job for a review of the `ci-approval` environment.
- A pull request needs the approval of a code owner, listed in
  [.github/CODEOWNERS](.github/CODEOWNERS), to merge.
- **Pull requests are merged by rebase.** Merge commits and squashing are
  disabled: a merge commit carries the pull request title into the history,
  so release-please would count every change twice, and a squash collapses a
  pull request into one changelog line.
- Versions, tags and `CHANGELOG.md` belong to release-please, which bumps
  `Cargo.toml`, `Cargo.lock` and `herdr-plugin.toml` in its release pull
  request. Do not edit any of them by hand.
- Keep a pull request to one thing. A refactor, a feature and a formatting
  sweep are three pull requests.
- Fill in every section of the pull request template: a Summary that shows the
  change, Evidence from before and after, and the Merge Danger.

## Code of conduct

Be kind, assume good faith, keep it about the work.

## License

By contributing you agree that your contribution is licensed under the
[MIT License](LICENSE).
