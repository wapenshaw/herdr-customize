# Installing

```bash
herdr plugin install wapenshaw/herdr-customize
```

The install downloads the prebuilt binary for your platform from this
repository's GitHub release and checks it against the SHA-256 sums published
with it. Release binaries exist for:

- Linux on x86_64 and arm64 (static, so they run on any distro)
- macOS on Apple Silicon and Intel
- Windows on x64 (ARM64 Windows runs it under emulation)

If the download fails, or there is no release for the version you are
installing, the install builds from source with `cargo` instead. That needs
Rust 1.99 or newer, plus the Visual Studio Build Tools (C++ workload) on
Windows. Set `HERDR_CUSTOMIZE_BUILD_FROM_SOURCE=1` to always build from
source.

On Windows the install step runs in the Windows PowerShell 5.1 that ships
with Windows, so PowerShell 7 is not needed. Herdr started from PowerShell 7
works too.

The Plugins page uses `git` for update checks.

## Bind a key

Herdr does not bind keys from plugin manifests, so bind one once after
installing:

```bash
herdr plugin action invoke bind --plugin herdr-customize
```

That binds `prefix+t` to the popup and reloads Herdr, and the result shows
as a Herdr notification. It changes nothing if one of the plugin's actions
already has a key, and refuses a key another binding uses. For another key,
run `./target/release/herdr-customize bind <key>` in the plugin's checkout,
or change it on the Keys page. The Keys page can add the rest.

To bind it by hand instead, add this to `config.toml` and reload:

```toml
[[keys.command]]
key = "prefix+t"
type = "plugin_action"
command = "herdr-customize.open"
description = "customize herdr"
```

```bash
herdr server reload-config
```

`herdr-customize.open` opens the popup on Settings, and `.keys`, `.themes`
and `.plugins` open it on that page, even when Themes or Plugins is turned off
in Settings.

## Update

Run `herdr plugin update herdr-customize`, or update it from its own Plugins
page.

Since 0.5.0 the Themes and Plugins pages start turned off. Turn them on at the
top of the Settings page; bindings to `.themes` and `.plugins` keep working
either way.

## Coming from herdr-theme-picker or Plugin Manager

This plugin replaces both. The Keys page marks bindings whose action is no
longer installed, such as `herdr-theme-picker.open` or
`ray.plugin-manager.open`. Point them at `herdr-customize.themes` or
`herdr-customize.plugins`, or delete them, then uninstall the old plugins from
the Plugins page. Turn on the Themes and Plugins pages at the top of Settings
to keep them on the tab bar. Themes you added to herdr-theme-picker can be added again
from the clipboard or your editor.

## Commands

The binary also runs outside the popup:

```sh
herdr-customize ui [page]              # the popup: settings, keys, themes or plugins
herdr-customize open [page]            # open the popup inside Herdr from a shell
herdr-customize bind [key]             # bind a key to the popup (default: prefix+t)
herdr-customize apply dracula-default  # apply a theme by name
herdr-customize sync                   # send the applied colors to local clients again
herdr-customize sync --client 12345    # ... to one client PID only
herdr-customize sync --startup         # ... to each client that appears within 10 s (Herdr runs this)
herdr-customize preview dracula-default
```

To run it on a machine that only connects with `herdr --remote`, see
[Themes](themes.md#using-herdr---remote-from-another-machine).
