# CLAUDE.md

@AGENTS.md

The canonical onboarding is **AGENTS.md**, imported above. Only what is specific
to Claude Code is below.

## Tooling

- `validate-agent-work`: run it after any change before handing work back, and
  again before opening a pull request. It runs the cargo gate, checks the other
  platforms and reads the whole branch against the mandatory rules in
  AGENTS.md, which no linter checks.
- `pr`: write every pull request body with it, before `gh pr create` or
  `gh pr edit --body`. It fills the template AGENTS.md makes mandatory.
- Delegate a broad multi-file search to the **Explore** agent and keep the
  conclusion, not the file dumps.

## macOS edit note

BSD `sed -i` differs from GNU sed; prefer `perl -pi -e 's/old/new/g' file` for
in-place substitution.
