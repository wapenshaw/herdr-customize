---
name: validate-agent-work
description: Use when finishing any change to this repository, or when the user says "validate", "check my work", "run checks", or "verify changes". Also use automatically after adding a feature, fixing a bug, or editing Rust, the install scripts, workflows or documentation here.
---

# Validate Agent Work

The rules live in [AGENTS.md](../../../AGENTS.md) and
[CONTRIBUTING.md](../../../CONTRIBUTING.md); this is the order to check them in
before handing work back.

## Steps

1. **See what changed**: the branch against `main`, and whatever is not
   committed yet:

   ```bash
   git diff --name-only main...HEAD
   git status --short
   ```

   Stage nothing: every check below runs over the whole crate, and the index
   is the user's to decide.

2. **Run the gate:**

   ```bash
   cargo fmt
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```

   `cargo fmt` rewrites files, so re-read one before editing it again.

3. **Check the other platforms** when the change touches anything behind
   `#[cfg]`, `src/terminal/`, `src/proc.rs`, `src/fsutil.rs` or a path:

   ```bash
   cargo clippy --all-targets --target x86_64-pc-windows-msvc -- -D warnings
   cargo clippy --all-targets --target x86_64-unknown-linux-gnu -- -D warnings
   ```

   If `ring` fails for want of the target's C headers, run them on a copy with
   ureq's TLS feature off, as AGENTS.md describes. Windows support is
   never dropped to make a check pass.

4. **Install scripts and workflows**, when they changed: run
   `scripts/fetch-or-build.sh` against a local server through
   `HERDR_CUSTOMIZE_RELEASE_BASE` with a fake `cargo` on `PATH`, and run
   `actionlint` over `.github/workflows/`. Keep asset names in step between
   the scripts and the release matrix.

5. **Comment pass**: the `## Comment pass` in
   [`.claude/rules/comments.md`](../../rules/comments.md), over the comments
   this change added or touched.

6. **Read the diff against the mandatory rules in AGENTS.md**: the language,
   commit, type and comment rules. Clippy checks none of them. The type rule
   needs a search rather than a read: a field, method or constant added
   without a reader in the same change.

7. **Check nothing was left behind**: no popup, `terminal-sync` helper or
   test sandbox still running, and nothing written to the real Herdr config
   or state.

8. **Before opening a pull request**, write its body with the `pr` skill and
   check it against
   [.github/pull_request_template.md](../../../.github/pull_request_template.md):
   every section filled, no HTML comment left, no attribution line. Open it
   only when the user asks, since CI runs on it.

9. **Fix and re-run**: fix what a step found, then run that step again to
   confirm it. If a failure is not yours to fix, say so rather than working
   around it.

10. **Report**: always say what happened:

    - Clean: a brief confirmation.
    - Found and fixed, one line each:

      ```
      Validation found and fixed 3 issues:
      - clippy: needless borrow in plugins::market::search_at
      - Comments: trimmed a four-line doc comment in theme/apply.rs
      - Type rule: removed Plugin::homepage, which nothing read
      ```

    - Still broken: what is wrong and why you could not fix it.

These steps are the whole gate on this machine, but CI also runs them natively
on Linux, macOS and Windows, plus a build on Rust 1.99; see CONTRIBUTING's
"Pull requests". The user should never be surprised by a failure that could
have been caught here.
