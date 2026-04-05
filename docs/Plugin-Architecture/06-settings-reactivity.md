# Settings Reactivity

## Overview

Plugin settings are stored via `tauri-plugin-store` (JSON file). The frontend
writes settings; the backend reacts through two separate paths depending on
which key changed.

## Flow

1. Frontend writes to the Tauri store.
2. The `"settings-changed"` Tauri event fires.
3. The listener in `lib.rs` does two things in parallel:
   - Calls `notifier.notify(key, value)` — propagates to internal
     `SettingsWatch` subscribers (see below).
   - Calls `host.handle_setting_changed(key, value, app)` — routes the change
     to the affected plugin.

### Host routing

`handle_setting_changed` inspects the key prefix:

- **`enabled.<plugin-id>`** — toggles `PluginSlot.enabled` (an `AtomicBool`),
  then calls `plugin.enable(ctx)` or `plugin.disable()` as appropriate.

- **`plugins.<plugin-id>.<key>`** — passes the change to the plugin's
  `CoalescingDispatcher`, which eventually calls
  `plugin.setting_changed(relative_key, value)`.

## Plugin-facing API: `setting_changed()`

Plugins react to settings changes by implementing:

```rust
fn setting_changed(&self, key: &str, value: &Value) { … }
```

`key` is the relative key within the plugin's namespace — e.g. `"retentionDays"`,
not `"plugins.clipboard.retentionDays"`. The plugin does not receive changes
to `enabled.<plugin-id>`; those are handled entirely by the host.

### CoalescingDispatcher

Setting changes are routed through a `CoalescingDispatcher` per plugin before
reaching `setting_changed()`:

- **Deduplication** — if the same key is written multiple times before the
  dispatcher has a chance to deliver it, only the latest value is delivered.
  This prevents redundant work when the frontend writes rapidly (e.g. a slider
  being dragged).
- **Ordering** — changes to *different* keys are delivered in the order they
  arrived, so relative ordering across distinct settings is preserved.

## SettingsWatch\<T\> — Internal Infrastructure Only

`src-tauri/src/settings_notifier.rs` still contains `SettingsWatch<T>`, which
wraps a `tokio::sync::watch::Receiver<Value>` and deserialises on `.get()`.

This is **no longer part of the plugin-facing API**. Plugins use
`setting_changed()` instead. `SettingsWatch<T>` remains in use by internal
subsystems that need to observe settings outside the plugin host:

- `FrecencyStore` — watches its own persistence settings
- Control socket — watches the socket path setting
- `WebsiteMetadataService` — watches cache / network settings

If you see `SettingsWatch` in the codebase, it belongs to one of these
non-plugin subsystems, not to plugin settings reactivity.

## Enable / Disable Key Placement

The enabled toggle lives at `enabled.<plugin-id>` — a top-level key, not
under `plugins.<id>.*`. This means it is outside the namespace that
`setting_changed()` receives, and plugins cannot accidentally react to their
own enable state. Only the host acts on that key.
