# Configuration and state

## The config file

The plugin finds `config.toml` the same way Herdr does: `HERDR_CONFIG_PATH`,
then `$XDG_CONFIG_HOME/herdr/config.toml`, then `%APPDATA%\herdr\config.toml`
on Windows or `~/.config/herdr/config.toml` elsewhere. A missing config is
created.

Edits change only the lines they touch, so comments, ordering and formatting
stay as they were. This works for keys inside inline tables
(`ui = { sidebar_width = 30 }`) and dotted keys too. Unrelated TOML,
permissions and symlinks are kept.

Before writing, the plugin checks the new file with `herdr config check`. If
the change adds a problem, the plugin refuses it and leaves the file alone.
Problems the file already had do not block unrelated edits. A check that does
not answer within 10 seconds also leaves the file alone, and the save can be
retried.

The first change each day backs up the original file to
`config.toml.bak-YYYYMMDD`. Writes are atomic, and Herdr reloads after each
save. Writers take turns through a lock file in the state folder, so two
popups never undo each other's edits. Closing the popup waits for its saves;
**ctrl+c** closes it without waiting.

Plugin settings files get the same in-place edits, daily backup and lock, but
no check: Herdr does not read them.

## State

State is stored in `HERDR_PLUGIN_STATE_DIR` when it is set, and otherwise in
`$XDG_STATE_HOME/herdr/plugins/herdr-customize` (by default
`~/.local/state/...`, or `%LOCALAPPDATA%\herdr\plugins\herdr-customize` on
Windows). Standalone runs and the installed plugin therefore share your themes,
the record of the applied one, and the write locks.

`pages` in the state folder lists the optional pages turned on, one per line
(`themes`, `plugins`). Delete it to hide both again.
