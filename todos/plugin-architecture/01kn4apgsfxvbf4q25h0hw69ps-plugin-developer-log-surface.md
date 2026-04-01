# Plugin developer log / error surface

When plugins move to WASM modules, plugin authors need a way to see
diagnostic messages from their plugins — warnings, errors, and
informational logs the host emits on their behalf.

## Motivation

Today, messages like the "dropping CustomUI in non-prefix mode" warning
go to `eprintln!` where only someone watching the Tauri process stderr
can see them. For internal plugins that's fine, but third-party plugin
developers won't have convenient access to process output.

## Requirements

- Surface plugin-specific warnings and errors (e.g., CustomUI
  downgrade, channel failures, capability denials) in an area visible
  to plugin developers inside the application.
- Include host-generated messages (the host logging *about* a plugin)
  and plugin-emitted logs (the plugin calling a log API).
- Logs should be scoped per plugin so developers can filter to their
  own plugin's output.

## Open questions

- **UI location**: Developer tools panel? A log viewer in the settings
  page per plugin? A separate debug window?
- **Log levels**: Mirror standard levels (debug, info, warn, error)?
  Should plugins control their own verbosity or does the host decide?
- **Persistence**: In-memory ring buffer? Persisted to disk? Both?
- **Production vs development**: Should end users ever see plugin logs,
  or is this strictly a developer tool gated behind a debug flag?
