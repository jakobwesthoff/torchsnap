# Clipboard Plugin: Custom Settings UI

Add per-plugin settings for the clipboard manager. This is the first
plugin that needs its own configuration beyond global app settings.

## Scope

- Retention policy (default 30 days, configurable)
- Content type toggles (track text / images / files / rich text)
- Privacy: list of apps to exclude from tracking (by bundle ID)
- Polling interval override (default 500ms)
- Toggle for recording concealed/transient entries

## Resolved

- Generic per-plugin settings infrastructure built: `SettingsInit` for
  defaults, `PluginSettings` for runtime reads, `usePluginSetting` hook
  on the frontend. All namespaced under `plugins.<id>.<key>`.
- Settings UI redesigned with sidebar navigation. Each plugin with a
  settings component gets its own section. Clipboard has a placeholder
  entry — replace it with the actual controls listed above.
