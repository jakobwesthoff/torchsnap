# Clipboard Plugin: Custom Settings UI

Add per-plugin settings for the clipboard manager. This is the first
plugin that needs its own configuration beyond global app settings.

## Remaining

- Content type toggles (track text / images / files / rich text)
- Privacy: list of apps to exclude from tracking (by bundle ID)
  - Depends on source app identification (separate todo)
- Toggle for recording concealed/transient entries

## Resolved

- Generic per-plugin settings infrastructure built: `SettingsInit` for
  defaults, `PluginSettings` for runtime reads, `usePluginSetting` hook
  on the frontend. All namespaced under `plugins.<id>.<key>`.
- Settings UI redesigned with sidebar navigation. Each plugin with a
  settings component gets its own section.
- Reactive settings notifier: `SettingsNotifier` with typed
  `SettingsWatch<T>` channels, `PluginSettingsNotifier` scoped wrapper,
  `PluginContext` bundling settings + notifier for plugin setup.
- Enable/disable toggle: `enabled` setting with full watcher start/stop
  lifecycle. When disabled, watcher and retention threads stop entirely,
  plugin vanishes from catalog.
- Retention policy: `retentionDays` setting (default 30, range 1–365)
  with a reusable Slider component. Dedicated retention cleanup thread
  runs every 30 minutes, reads the setting each cycle.
- Statistics panel: entry counts by format and total storage size,
  fetched on settings page mount.
- Clear history button with confirmation.
- Polling interval override dropped — `clipboard-rs` does not expose a
  configurable polling interval.
