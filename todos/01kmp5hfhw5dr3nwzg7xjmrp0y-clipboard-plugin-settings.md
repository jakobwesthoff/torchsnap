# Clipboard Plugin: Custom Settings UI

Add per-plugin settings for the clipboard manager. This is the first
plugin that needs its own configuration beyond global app settings.

## Scope

- Retention policy (default 30 days, configurable)
- Content type toggles (track text / images / files / rich text)
- Privacy: list of apps to exclude from tracking (by bundle ID)
- Polling interval override (default 500ms)
- Toggle for recording concealed/transient entries

## Open questions

- Do we need a generic per-plugin settings infrastructure, or is it
  fine to hardcode clipboard settings into the existing settings store
  under a namespaced key (e.g., `plugins.clipboard.*`)?
- Settings UI: separate settings pane per plugin, or a single settings
  view with plugin sections?
