# Windows test plan

Native checks for Windows. The code cross-compiles and the Windows-only tests
compile on macOS, but nothing below has run on Windows yet.

Reference setup: a Windows laptop running `herdr --remote <mac>` in Windows
Terminal, with the Mac as the Herdr server.

## 0. Setup

```powershell
git clone https://github.com/wapenshaw/herdr-customize.git
cd herdr-customize
go version          # 1.24 or newer
git --version       # for plugin update checks
go build -o herdr-customize.exe ./cmd/herdr-customize
```

Run every command below from the checkout so the binary finds `themes\`.
Note your config path before starting: `HERDR_CONFIG_PATH` if set, else
`%XDG_CONFIG_HOME%\herdr\config.toml` if `XDG_CONFIG_HOME` is set, else
`%APPDATA%\herdr\config.toml`.

## 1. Automated tests

```powershell
go vet ./...
go test -count=1 ./...
```

Expected: all `ok`. These run only on Windows:

| Test | Checks |
| --- | --- |
| `TestHelperWritesToAnIsolatedClientConsole` | The helper attaches to a hidden fake `herdr.exe` console and writes to `CONOUT$`. |
| `TestExactHelperDoesNotFallBackToAnotherHerdr` | A non-Herdr PID reports unavailable instead of attaching to a real client. |
| `TestCLICommandsNeverReceiveHostQueries` | `herdr server stop`, `status`, `--help`, and similar are never targeted. |
| `TestClientDiscoveryPreservesSessionArguments` | `--session "work projects"` survives Windows command-line parsing. |
| `TestInvalidExplicitPIDIsRejectedBeforeHelper` | An invalid explicit PID errors without spawning the helper. |
| `TestStandaloneStateWindowsCompatibility` | `%LOCALAPPDATA%` state paths and the `applied` marker location. |
| `TestWindowsEditorShim` | Editor launch through the Windows shim. |

The tests use a temporary home/config/state and a fake `herdr`. They must not
change your real config or recolor your terminal; check both afterwards.

## 2. Picker on the laptop (`--remote` client)

In a Windows Terminal tab **outside** Herdr, while another tab runs
`herdr --remote <mac>`:

```powershell
.\herdr-customize.exe ui themes
```

| Check | Expected |
| --- | --- |
| The popup opens on the Themes page | Tabs at the top; `✓` marks the current theme; **/** searches. |
| Preview | Swatches render beside the list and update as you move. Box-drawing and `█` characters are not garbled. |
| Press Enter on a theme with clearly different colors | The popup closes. A warning says reload failed (no local server) and suggests `prefix+shift+r`. Exit code is 0. |
| Config | Your config now ends with `[theme.custom]`; the rest is untouched. On the first change of the day, `config.toml.bak-YYYYMMDD` exists. If the config was missing, it was created. |
| Herdr window before reload | Pane text, prompt colors, background, and foreground already show the new palette (OSC 4/10/11 through the console helper). |
| Press `prefix+shift+r` in the Herdr window | The Herdr UI (sidebar, tabs, borders) switches to the new theme. |
| Picker tab afterwards | No flashing window. Keys and the cursor still work; the console mode was restored. |
| Other Windows Terminal tabs without Herdr | Unchanged. |
| Mac | Its theme and terminal are unchanged. |

If pane colors do not change, run step 3 to separate discovery from the write.
Then report whether Windows Terminal ignored the sequences or the helper failed
(any stderr text).

## 3. `sync` and client discovery

```powershell
Get-Process herdr | Select-Object Id, ProcessName, StartTime
.\herdr-customize.exe sync
.\herdr-customize.exe sync --client <herdr --remote PID>
.\herdr-customize.exe sync --client $PID      # this PowerShell
```

| Check | Expected |
| --- | --- |
| `sync` | Exit 0; the `herdr --remote` window gets the applied colors again. |
| `sync --client <client PID>` | Exit 0; only that window recolors. |
| `sync --client` with the PowerShell PID | `PID … is not a Herdr client`; exit 1. |
| `sync` with no Herdr window open | `No Herdr client terminal found on this machine.`; exit 1. |
| A new Windows Terminal window running `herdr --remote <mac>` | Starts with Windows Terminal's own colors. Running `sync` applies the theme. |
| Two `herdr --remote` windows, then apply a theme | Both recolor. |

## 4. Theme management on Windows

| Action | Expected |
| --- | --- |
| Copy a Ghostty palette, press **c**, enter a name | Read via `Get-Clipboard`; saved, applied, and marked `★`. |
| **a** (add with editor), with `$env:EDITOR` unset | Notepad opens a template; saving and closing asks for a name and adds the theme. |
| `$env:EDITOR = 'code --wait'`, then **a** | VS Code opens and the popup waits, then redraws cleanly. |
| **e** on a `★` theme, **Enter** on a token, type a hex, **Enter**, then **s** | The preview changes live; the override is saved, then applied. |
| **d** on a `★` theme, answer `y` | The theme is removed. The bundled themes are still there. |
| Apply an unknown slug: `.\herdr-customize.exe apply nonexistent` | Fails with message containing terminalcolors.com link; exit 1. |

## 5. Config edge cases

Back up your config first, or point `$env:HERDR_CONFIG_PATH` at a copy.

| Setup | Expected after applying twice |
| --- | --- |
| Config with `auto_switch = true` and `[theme.custom.light]` / `[theme.custom.dark]` | Both layers survive unchanged. |
| CRLF line endings, as written by Notepad | Still CRLF; no mixed endings. |
| Applying the same theme twice | The second apply leaves the file byte-identical (`Get-FileHash`). |
| `$env:XDG_CONFIG_HOME = "$env:TEMP\xdg"` | The config is written under `$env:TEMP\xdg\herdr\config.toml`, matching Herdr. |

## 6. Optional: Herdr server on Windows

Only if you also run a local Herdr server on Windows (`herdr` without
`--remote`):

```powershell
herdr plugin link "$PWD"
herdr server reload-config
```

| Check | Expected |
| --- | --- |
| `prefix+t` in the local Windows Herdr | The popup opens. Enter applies and reloads with no warning; the window recolors. |
| Customize: sync terminal colors action | Recolors the local Herdr window. |
| `Get-Process herdr-customize` while idle | Nothing running; no background daemon. |
| Mac attached to this Windows server via `--remote`, no local Windows client | `prefix+t` from the Mac shows "Themes belong to your client machine". |

## 7. Plugins, Keys and Settings (local Windows server)

These need a local Herdr server on Windows with the plugin linked (step 6).
Point `$env:HERDR_CONFIG_PATH` at a copy of your config first, or back it up.

| Action | Expected |
| --- | --- |
| Plugins page | Installed plugins are listed; after a moment each GitHub plugin shows `●` or `↑` (needs `git` on PATH). |
| **Enter** on a plugin, **Enter** on one of its actions | The popup closes, then the action runs (for example its own popup opens). No console window flashes. |
| **i**, type `owner/repo`, **Enter** | The popup gives way to `herdr plugin install` with its trust prompt; answering and pressing Enter returns to the list. |
| **o** on a GitHub plugin | The repository opens in the default browser. |
| **c** | `%APPDATA%\herdr\plugins.json` (or the file beside your config) opens in your editor. |
| **m**, then **/** and a word | Marketplace results load and filter; **Enter** on one asks before installing. |
| Keys page, **Enter** on `zoom`, **ctrl+r**, press `ctrl+alt+z`, **Enter** | `config.toml` gets `zoom = "ctrl+alt+z"` under `[keys]`; CRLF endings are kept; Herdr reloads. |
| Keys page, **n**, key `x`, **Enter** | Refused: a plain key would intercept typing. |
| Settings page, **Enter** on a boolean | Toggles and saves; `herdr config check` runs first. |
| Settings page, **E**, break the TOML, save and close | Not saved; the status says why. **E** reopens your edited copy. |

## What to report back

- The `go test` output if anything fails.
- For each failed row: the command, the full stderr, and a screenshot if colors
  look wrong.
- Windows Terminal version (`wt --version`, or Settings → About) and the
  PowerShell version.
