---
description: How to write comments, and what never to comment
---

# Comments

Rust, and this repository's Rust. Comments here are in English, like everything
else written into the repository, whatever language the request was made in.

## Rules

- These rules cover every kind of comment: module docs (`//!`), doc comments
  (`///`) on types, functions, fields and constants, and inline `//` ones.
- Comment only where it is genuinely needed: a declaration or a block whose
  intent is not obvious at a glance. Obvious code gets none, public or not:
  `Driver::new` returning a `Driver` needs no doc comment, and a comment that
  only expands the identifier's name is worse than silence.
- **A comment is at most three lines.** A hard cap. Anything longer is
  explaining something the code cannot hold, and that explanation belongs in
  [docs/](../../docs/) with one line here pointing at it.
- A doc comment reads as a sentence about the item it sits on, in the third
  person: `/// Splits a Windows command line as the C runtime builds argv.`
  Inline comments inside a function need not be full sentences.
- One module doc per module, at the top of its file.
- Explain the gist of the thing the comment sits on: what it does, plus the why
  when that is not obvious. Never who calls it or when it runs: call sites
  move and the comment then lies.
- Never record provenance: which issue, which measurement session, which
  commit. That is what `git log`, `AGENTS.md` and `docs/` are for. A
  *measured value* is worth stating; the session that measured it is not.
- Never justify code somewhere else. A contract is stated where it is relied
  on, never where a caller might one day lean on it.
- Keep a comment that stops a reader "fixing" the code and breaking it: a
  measured constant, a constraint Herdr or an OS imposes, an ordering that
  matters, a case that looks unhandled and is not.
- Plain language, plain words. If a sentence needs a second read, rewrite it
  shorter.
- Do not restate a line the code already states (`// increment the counter`
  above `n += 1`).
- An `#[allow(...)]` names its lint and gives its reason in the attribute:
  `#[allow(clippy::useless_conversion, reason = "time_t is 32-bit on some targets")]`.
- A `// SAFETY:` comment sits on every `unsafe` block and says why the call is
  sound, not what it does.
- In tests, a comment says what the case protects (the behaviour that would
  otherwise be lost), never what the assertion does.
- An item deleted takes its comment with it.

## Examples

Bad, restates the code and names the caller:

```rust
/// Called by App::dispatch for every message.
/// It updates the page from the message.
fn update(&mut self, msg: &mut Msg, ctx: &mut Ctx) { ... }
```

Good, what it does and the why the signature cannot state:

```rust
/// Recolors this machine's Herdr client terminals (only client_pid's, when
/// set) and refreshes the user's Ghostty fragment if it exists. Fails with
/// terminal::Unavailable when no client was found.
fn sync_terminal_colors(pal: &Palette, palette_path: &Path, client_pid: &str) -> Result<()> { ... }
```

Bad, a rationale essay with its provenance:

```rust
// While testing on the Windows laptop we found that writing the OSC sequence
// from the popup's own process did nothing, because the popup runs in a pane
// whose console is Herdr's pseudo console and not the user's terminal. After
// trying several approaches we settled on a helper that frees its console and
// attaches to the client's, which is why the process is spawned here.
let status = Command::new(exe).args(["terminal-sync", &pid]).status()?;
```

Good, the same thing in three lines, with the paragraph left in the docs:

```rust
// Only a process attached to the client's console can write to its terminal,
// so a helper frees its own console and attaches there; exit code 3 means
// the client has no console to attach to.
let status = Command::new(exe).args(["terminal-sync", &pid]).status()?;
```

Good, a why the code cannot state:

```rust
// Record the selection with the config so a later sync or failed reload
// can never pair the new UI theme with the previous terminal palette.
fsutil::atomic_write(&applied_file(), format!("{slug}\n").as_bytes(), 0o644)?;
```

Good, none at all:

```rust
fn sanitize(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().trim().to_string()
}
```

## Comment pass

Write the code first. As the last step before reporting, check the comments
this change added or touched against the rules above. Edit comments only, never
code.
