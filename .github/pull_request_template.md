## Summary

<!--
The smallest view that shows the change: app-side pseudocode, a call tree, a
file tree, a diff sketch or a screenshot of the Customize screen, each beside
one or two lines of why. If it settles something about the app's CLI or data,
say what was checked.
-->

Closes #

## Evidence

<!--
A before and an after: a failing then passing test, the screen before and
after, a run on Windows or Linux. The checks below are the floor; show what
you looked at beyond them.
-->

- **Before:**
  **After:**

## Merge Danger

<!--
A two-way door is cheap to walk back; a one-way door is not: a write to the
user's app home or herdr-core `config.toml`, a mod installed or removed, or a
release asset name that `scripts/fetch-or-build.*` depends on.
-->

**Door:** <!-- one-way or two-way -->

**Blast Radius:** <!-- one word: what could break, and for whom -->

## Before merging

- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and
      `cargo test` pass locally.
- [ ] Platform code type-checks for Windows and Linux
      (`cargo clippy --target x86_64-pc-windows-msvc`,
      `--target x86_64-unknown-linux-gnu`).
- [ ] One logical change per commit, Conventional Commits, no co-author
      trailer. **PRs are rebased, never squashed or merged**, so the commit
      messages are what lands in the history and in the changelog.
- [ ] Tests land with the behaviour they cover.
- [ ] `CHANGELOG.md`, the tags and the versions in `Cargo.toml` and
      `Cargo.lock` are untouched; they belong to release-please.
- [ ] Something new learned about the app's CLI or data is recorded in
      `docs/` or `AGENTS.md`.
- [ ] This pull request is one thing.
