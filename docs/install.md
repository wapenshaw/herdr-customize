# Installing

Customize is a screen for the Herdr desktop app (`herdr-app`).

## Requirements

- `herdr-app` installed.
- Git 2.25 or newer.
- Either a release binary for your platform or Rust and Cargo to build one.
  Windows needs the MSVC build tools only when building from source.

Release binaries are available for Linux on x86_64 and arm64, macOS on Apple
Silicon and Intel, and Windows on x64. The Windows build runs on ARM64 through
emulation. The fetch-or-build script downloads the binary for the checkout's
version and verifies its SHA-256 checksum; if no matching release is available,
it builds with Cargo. Set `HERDR_CUSTOMIZE_BUILD_FROM_SOURCE=1` to always
build.

## Install

On Linux and macOS:

```sh
git clone --depth 1 https://github.com/wapenshaw/herdr-customize ~/.local/share/herdr-customize
cd ~/.local/share/herdr-customize
sh scripts/fetch-or-build.sh
target/release/herdr-customize app install
```

On Windows, use Windows PowerShell 5.1 or PowerShell 7:

```powershell
git clone --depth 1 https://github.com/wapenshaw/herdr-customize "$HOME\.local\share\herdr-customize"
Set-Location "$HOME\.local\share\herdr-customize"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\fetch-or-build.ps1
.\target\release\herdr-customize.exe app install
```

`app install` writes the screen and its key to the app, plus the featured
bundled palettes. It does not take a key already in use. The default key is
**ctrl+shift+y** (**cmd+shift+y** on macOS); pass another key to `app install`
if needed. See [the app guide](app.md) for the pages and commands.

## Update

Update the checkout, fetch or build its matching binary, and run `app install`
again to update the screen:

```sh
git pull
sh scripts/fetch-or-build.sh
target/release/herdr-customize app install
```

Use `powershell -File scripts\fetch-or-build.ps1` instead of the shell script
on Windows. To rewrite the featured palettes separately, run
`herdr-customize app themes`; `--force` replaces existing palette files.

## Uninstall

Remove the Customize screen from **Mods > Installed**; the [Mods section of
the app guide](app.md#mods) describes what removal keeps. Palette files remain
in the app's `themes` folder.

## Herdr TUI

The TUI plugin was retired in 1.0.0. Its last release was v0.10.0; pin it with
`herdr plugin install wapenshaw/herdr-customize --ref v0.10.0`.
