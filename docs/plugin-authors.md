# Describing your plugin's settings

The Plugins page edits any plugin's settings by reading the files in its
config directory (see [Plugin settings](guide.md#plugin-settings)). To give
your settings descriptions, defaults and allowed values, ship
`config-schema.json` in the plugin root. It uses the format of Herdr's config
reference, plus an optional `file` (default `config.toml`). Listed keys show
even before the file exists:

```json
{
  "file": "config.toml",
  "sections": [{ "id": "general", "title": "General", "keys": [
    { "key": "toggle_placement", "type": "enum", "default": "\"split\"",
      "values": ["split", "popup", "tab"], "description": "Where the panel opens." },
    { "key": "index_on_startup", "type": "boolean", "default": "true",
      "description": "Index sessions when Herdr starts." }
  ]}]
}
```

Types are `boolean`, `integer`, `float`, `string`, `enum` and `array`, and
`default` is a TOML value.
