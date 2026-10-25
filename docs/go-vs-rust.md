# Go vs Rust: the same plugin written twice

herdr-customize was written in Go first (now on the `go` branch) and then
ported to Rust (`main`). The port kept the four pages, every key, the
file formats and the tests' intent, so the two branches are a controlled
comparison: same program, same author, same month. This page records what
was measured and what the two languages felt like, to help choose a language
for the next project.

## What the port kept and what it dropped

Kept: Linux, macOS and Windows; the four pages, fzf ranking with a text fallback, the key sheet,
remote-only detection, `config check` gating, daily backups, atomic writes,
locks, schema fetch per Herdr version, marketplace search, update checks,
plugin settings with `config-schema.json`, the token editor, the Ghostty
fragment, and the detached action runner.

Dropped, because they were not worth carrying:

| Dropped | Why |
| --- | --- |
| Import from herdr-theme-picker (`legacy.go`, `import-themes`) | One-time migration; nobody needs it twice. |
| CLI `add`, `edit`, `delete`, `terminal-sync`, picker aliases | The Themes page does all of it. |
| Hand-written TOML splicer (690 lines) | `toml_edit` preserves formatting and can edit inline tables, which the Go version could not. |

Windows was briefly dropped in the first cut of the port and then restored:
supporting Windows is the reason this project exists, since plugins like
Plugin Manager do not. Apart from the removed commands, the Rust branch does
the same work, plus editing inline tables.

## Numbers

Measured on an Apple Silicon Mac, both stripped release builds, in a
sandbox with a fake `herdr` so no live session was touched. Startup is the
median of 15 alternating runs through a pty harness.

| | Go | Rust | Notes |
| --- | --- | --- | --- |
| First byte on screen | 20 ms | 19 ms | Process start is a wash. |
| Full first frame | 40 ms (33–46) | 32 ms (27–43) | Rust has no runtime to start and no GC heap to size. |
| Resident memory | 18 MB | 7 MB | Go's runtime, GC headroom and goroutine stacks. |
| Binary (stripped) | 8.6 MB | 3.8 MB | Go links its runtime and reflection metadata. |
| Clean release build | 6.0 s | 32.1 s | Rust: LTO + `codegen-units = 1`. |
| Rebuild after a one-line edit | 0.7 s | ~20 s (release) | Rust dev builds are faster but still not Go-fast. |
| Test suite (wall) | 12.6 s | 3.6 s | Both dominated by spawning the fake `herdr` script; Go's suite simply has more subprocess tests. |
| Dependencies | 6 direct, 25 modules | 12 direct, 151 crates across all targets | Rust leans on small crates; Go's stdlib covers more. |
| Cross-compile for Windows from a Mac | `GOOS=windows go build`, nothing else | Type-check only; a build needs the Windows C headers for `ring` | See "Building for Windows" below. |

The slowness that started this experiment was not the language. The Go
popup scanned every process (`ps`) before drawing its first frame; moving
that scan beside the first frame (commit `a3b5884`, the last on the `go` branch) fixed most of
it. After that fix Rust is still faster, but by ~8 ms and 11 MB, not by a
margin a person feels in a popup.

## Code size

Non-blank, non-comment lines outside tests, with the legacy importer
excluded from Go. Both sides include Windows.

| Module | Go lines | Rust lines |
| --- | --- | --- |
| ui | 4,101 | 3,950 |
| theme | 1,713 | 890 |
| config | 758 | 396 |
| pluginconfig | 542 | 390 |
| plugins | 522 | 397 |
| terminal | 436 | 461 |
| keys | 232 | 252 |
| schema | 173 | 117 |
| main, herdr, fsutil, proc, locks | 463 | 379 |
| **Total** | **8,940** | **7,232** |
| Tests | 3,744 | 817 |

Line counts flatter Rust: `rustfmt.toml` allows 200-column lines and Go's
`gofmt` style is taller. Counting non-whitespace characters instead, Go has
195,543 and Rust 200,420, a ratio of 1.02. **The two programs are the same
size.** Rust saved the TOML splicer and spent the savings on types,
ownership plumbing, explicit conversions and `unsafe` FFI blocks.

The Rust test suite is smaller because the Go one tests through public
packages under `tests/` with many fixtures, while the Rust port kept the
behaviours that matter (58 tests) without the per-package scaffolding.
That is a choice made during the port, not something Rust forced.

## How the code differs

Both versions use the same architecture: an Elm-style loop where messages
come in, pages update their state, and a view function renders it. The
differences are in how each language lets you express that.

### Messages: open interfaces vs a closed enum

Go (Bubble Tea) passes `tea.Msg`, which is `any`. Each page type-switches on
what it cares about and ignores the rest. Adding a message type touches only
the sender and the receiver; forgetting a receiver compiles fine.

Rust uses one `enum Msg` listing every message. Each page matches on it and
`mem::take`s the payload it owns. Adding a variant is one edit in `ui/mod.rs`
and the compiler points at every `match` that needs a decision. The cost is
that all pages share one big type and see each other's messages.

### Concurrency: goroutines vs threads and a channel

Go returns a `tea.Cmd` (a `func() tea.Msg`) and the runtime runs it on a
goroutine. It is cheap and invisible.

Rust has no runtime, so the port built one in ~60 lines: pages push
`Effect::Spawn(job)` into a `Ctx`, the loop starts each job on an OS thread,
results come back over `mpsc`, and the loop polls at 10 ms while jobs run.
No async runtime was needed; a TUI that runs a handful of subprocesses does
not need tokio. The upside is that the whole scheduler is visible and the
test driver can run it deterministically.

### Errors

Go: `fmt.Errorf("...: %w", err)` and `errors.As(err, &check)` to recover a
`*config.CheckError`. Every call site has `if err != nil { return err }`.

Rust: `anyhow::Result`, `.context("...")?`, and `e.downcast_ref::<CheckError>()`
for the same recovery. The `?` operator removes the boilerplate; the
equivalent of `%w` wrapping is `.context()`. Behaviour is the same, the code
is shorter at each call site, and errors print with their whole chain via
`{e:#}`.

### The borrow checker shaped the design

Three places where Go's "just take a pointer" did not translate:

1. **Side effects.** Go pages call helpers that both mutate the page and
   start commands. In Rust a page method holds `&mut self`, so it cannot also
   borrow the app to start a job. Pages instead push `Effect`s into a `Ctx`
   and the app applies them after the page returns. This turned out to be a
   better design: the test driver can assert "no program was run on the
   terminal" by inspecting the effects.
2. **Confirmation prompts.** Go stores a closure that captures the page
   pointer. Rust cannot store a closure that borrows the page inside the
   page, so `Confirm<P>` stores `Box<dyn FnOnce(&mut P, &mut Ctx)>` and
   receives the page as an argument when it runs.
3. **Lists.** Scrolling needs `&mut` and drawing rows needs to read the page
   while the list is borrowed. The list was split into `window(height)`
   (mutable, adjusts scroll) and `render(rows, draw)` (immutable).

Each fix took longer to find than the Go original took to write. None of
them were bugs in the Go code; they were aliasing that Go allows and Rust
makes you spell out.

### Libraries

The single biggest code difference is TOML. Go's `go-toml` decodes but does
not preserve formatting, so the Go version carries a 690-line byte-splicing
editor that finds spans and rewrites them, and gives up on inline tables.
Rust's `toml_edit` is format-preserving, so the Rust config module is half
the size and handles more cases. It had one quirk: comments at the end of a
file are document trailing decor, so new tables were inserted above them;
`Doc::adopt_trailing` moves them.

Go's stdlib did more elsewhere: HTTP with TLS, JSON, process control and
`os/exec` all come built in. Rust needed `ureq`, `serde_json`, `libc` and
`regex`, which is why it has twice the direct dependencies and five times
the transitive ones.

### Platform code and Windows

Go splits platforms by filename (`lock_unix.go`, `lock_windows.go`). Rust
uses `#[cfg(unix)]` / `#[cfg(windows)]` on items, or whole modules
(`terminal/unix.rs`, `terminal/windows.rs`). Both work; Go's is easier to
scan, Rust's keeps both sides of a small function next to each other.

Rust's standard library covered more of the Windows work than Go's did:

| Need | Go | Rust |
| --- | --- | --- |
| Lock a file | `LockFileEx` in `lock_windows.go`, `flock` in `lock_unix.go` | `File::lock()`, the same on every OS |
| Replace a file atomically | `MoveFileEx` in `replace_windows.go` | `fs::rename` already does this on Windows |
| Detached helper | `SysProcAttr.CreationFlags` | `CommandExt::creation_flags` |
| Process list, command lines, console attach | `golang.org/x/sys/windows` | `windows-sys` |

The low-level calls look alike in both. The difference is that Go's
`x/sys/windows` wraps them in functions that return `error`, while
`windows-sys` exposes raw C signatures: every call is `unsafe`, returns `0`
or a null handle on failure, and needs `io::Error::last_os_error()` and a
`Drop` guard for handles. The Rust Windows module is a little longer than
Go's (199 lines vs 169), mostly because each `unsafe` block carries a
comment saying why it is sound.

One piece came out better in Rust: Go calls `CommandLineToArgvW` (through
`DecomposeCommandLine`) to split a process's command line, which only runs on
Windows. The Rust port splits it in plain Rust, so the test runs on a Mac.

### Building for Windows

This is the biggest practical difference for a Herdr plugin, because Herdr
builds plugins from source on the user's machine.

- **Go:** a Windows user needs only Go. From a Mac,
  `GOOS=windows go build` produces a working `.exe` with no other setup.
- **Rust:** a Windows user needs Rust *and* the Visual Studio Build Tools
  (several GB) for the MSVC linker and for `ring`, the C/assembly crypto
  library behind TLS. From a Mac, `cargo clippy --target
  x86_64-pc-windows-msvc` type-checks everything except `ring`, but a real
  build needs a Windows C toolchain (or `cargo-xwin`), so the Windows
  binary was never built here. Only CI on `windows-latest` and a real
  machine can confirm it.

If Windows users matter most, this is a point for Go, or for shipping
prebuilt Rust binaries instead of building on install.

### Tests

Go's `t.Setenv` and `t.TempDir` make isolated tests easy and parallel by
default. In Rust 2024 `std::env::set_var` is `unsafe` because another thread
might read the environment, so the UI tests set the sandbox up once and run
one at a time behind a mutex. Unit tests live beside the code in
`#[cfg(test)] mod tests`, which keeps private helpers testable without
exporting them.

### Build and iteration

Go's compile speed is the thing missed most during the port. A one-line
change rebuilds in under a second; a Rust release build with LTO takes 20 s
and a clean one 32 s. In practice: iterate with `cargo test` and dev builds,
and keep LTO for the binary that ships. Go also caches by content hash, so
switching branches rarely rebuilds everything; Cargo keys on mtimes.

## What to take into the next project

**Choose Go when** users build your code themselves on Windows (Go needs
nothing else installed), iteration speed matters more than the last few
milliseconds or megabytes, the program is mostly calling other programs and
the network, you want the stdlib to cover HTTP/JSON/processes, or other
people need to read and change the code without learning ownership. For this
plugin, Go was fast enough once the startup bug was fixed.

**Choose Rust when** you ship prebuilt binaries or users already have a C
toolchain, memory or startup is a real constraint (many instances, small
machines, something that runs on every keystroke), a library like
`toml_edit` gives you a capability the Go ecosystem lacks, or the data model
is a closed set of states that an `enum` captures better than interfaces.
Expect longer builds and expect the borrow checker to redesign anything that
shares mutable state; the redesigns are usually improvements, but they cost
time up front.

**Not a deciding factor here:** code size (equal), correctness (both
versions pass the same behaviours), and raw CPU speed (this program spends
its time waiting on `herdr`, `git` and the terminal).
