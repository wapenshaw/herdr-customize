## Summary

<!--
The smallest view that makes the point: pseudocode, a call tree, a file
tree, a diff sketch or a screenshot of the popup, each beside one or two
lines of why. If it settles something about Herdr's CLI or config, say what
was checked.
-->

Closes #

## Evidence

<!--
A before and an after: a failing then passing test, the popup before and
after, a run on Windows or Linux. The checks below are the floor; show what
you looked at beyond them.
-->

- **Before:**
  **After:**

## Merge Danger

<!--
A two-way door is cheap to walk back; a one-way door is not: a write to the
user's config.toml, a theme pushed to their terminals, a plugin installed or
removed, a release asset name the install scripts depend on.
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
      `herdr-plugin.toml` are untouched; they belong to release-please.
- [ ] This pull request is one thing.
