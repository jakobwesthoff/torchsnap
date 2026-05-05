# 14. Pass app handle to plugin setup

Date: 2026-03-27

## Status

Superseded by [25. Host-managed plugin enable/disable lifecycle](0025-host-managed-plugin-enable-disable-lifecycle.md)

## Context

Plugin `setup()` currently takes only `&self`. Plugins that need access
to Tauri APIs during initialization — resolving storage paths, accessing
managed state, or spawning background threads that interact with the
Tauri runtime — have no way to obtain the `AppHandle`. The only method
that currently receives it is `execute()`, which runs much later and
only in response to user actions.

The clipboard manager plugin is the first concrete case: its `setup()`
must spawn a watcher thread that needs the app handle to resolve the
data directory for clipboard history storage. Future plugins (file
search, contact search) will have similar needs.

## Decision

Both `CatalogPlugin::setup()` and `QueryPlugin::setup()` gain an
`AppHandle` parameter:

```rust
fn setup(&self, app: &tauri::AppHandle) {}
```

Plugins that need the handle store `app.clone()` or derive what they
need during setup. Plugins that don't need it ignore the parameter.

`CatalogRegistry::setup_all()` receives the `AppHandle` and forwards it
to each plugin.

## Consequences

- All existing plugins need a trivial signature update (`_app:
  &tauri::AppHandle`). No behavioral change.
- `setup_all()` and the call site in `lib.rs` need the handle threaded
  through.
- Plugins can now resolve paths, access Tauri-managed state, and obtain
  handles for background threads during initialization — without
  workarounds like storing the handle at construction time.
