# Repository Guidelines

## Project Structure & Module Organization

- `cmd/herdr-customize/`: CLI entrypoint and command routing.
- `internal/ui/`: Bubble Tea popup with the Themes, Plugins, Keys, and Settings pages.
- `internal/config/`: in-place `config.toml` edits, `herdr config check` gating, backups, and reloads.
- `internal/schema/`: Herdr's config reference (embedded copy plus per-version cache).
- `internal/keys/`: key parsing, Herdr's binding rules, and conflict detection.
- `internal/plugins/`: `herdr plugin` calls, update checks, marketplace search, and detached action runs.
- `internal/pluginconfig/`: typed reading and editing of any plugin's TOML/`.env` config files, with an optional `config-schema.json`.
- `internal/theme/`: palette loading, user themes, `[theme.custom]` persistence, and terminal color sync.
- `internal/terminal/`: client discovery, OSC color writes/readback, and sync tracking.
- `internal/herdr/`, `internal/fsutil/`, `internal/proc/`, `internal/instance/`: Herdr CLI wrapper, atomic writes, detached helpers, and file locking.
- `tests/{config,plugins,pluginconfig,schema,theme,terminal,instance}/`: public API and regression tests; `tests/testutil/` provides fake subprocesses, including a scripted `herdr`.
- `themes/`: bundled Ghostty-format palettes and `index.txt`; `preview.png` is the preview asset.
- `herdr-plugin.toml`: plugin metadata, build commands, panes, and actions. `docs/` records persistence edge cases.

## Build, Test, and Development Commands

Use Go 1.24 or newer. Interactive development requires Herdr; plugin update checks use `git`.

- `go build -o herdr-customize ./cmd/herdr-customize`: build the plugin binary; use `herdr-customize.exe` on Windows.
- `go test -v ./...`: run all unit and regression tests.
- `go vet ./...`: run Go static checks.
- `go fmt ./...`: format Go packages.
- `herdr plugin link "$PWD"` followed by `herdr server reload-config`: link the checkout for local development.
- `./herdr-customize ui keys`: open the popup on a page (`themes`, `plugins`, `keys`, `settings`).
- `./herdr-customize preview dracula-default`: render a bundled palette preview.

Cross-build platform-sensitive changes for Linux, macOS, and Windows; native runtime checks are needed to validate terminal behavior.

## Coding Style & Naming Conventions

Follow standard Go formatting with tab indentation and `gofmt`. Use descriptive package-level names, `PascalCase` for exported identifiers, and `camelCase` for private identifiers. Keep OS-specific implementations in files such as `files_windows.go` and `files_unix.go`, preserving build constraints.

Theme filenames use lowercase hyphenated slugs, such as `catppuccin-mocha`, without extensions. Register bundled additions in `themes/index.txt`; include foreground, background, and all 16 ANSI palette entries.

## Testing Guidelines

Use Go's standard `testing` package: name files `*_test.go` and functions `TestSomething`. Place private helper tests beside their packages and public API/integration tests under `tests/`. No numerical coverage threshold is configured.

Use `t.TempDir()`, `t.Setenv()`, local HTTP servers, and `tests/testutil` subprocess helpers (`FakeHerdr` scripts `herdr` responses and logs calls). Isolate state and config paths; tests must avoid recoloring live terminals, reloading real Herdr sessions, installing or changing real plugins, reaching GitHub, or modifying the clipboard. Point `plugins.GitBase`, `RawBase`, and `APIBase` at local repositories or servers. UI tests drive pages through the `driver` in `internal/ui/app_test.go`. Add regression coverage for changed behavior.

## Commit & Pull Request Guidelines

History commonly uses `feat:`, `fix:`, and optional scopes such as `fix(sync):`. Use concise, imperative subjects. PRs should explain the problem and resulting behavior, link relevant issues, list validation commands and platforms, and include screenshots for UI changes. Update README or persistence notes when behavior changes.
