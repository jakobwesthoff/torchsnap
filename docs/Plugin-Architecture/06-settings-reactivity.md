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

## WASM Plugins (ADR 0029)

WASM plugins use the same pipeline as native plugins — the host's
`CoalescingDispatcher` (above) sits in front of every plugin slot
regardless of whether it's a native or a WASM bridge. The bridge
just adapts the trait method into a call across the wasmtime
boundary.

### Reading settings: `settings::get`

WASM plugins import the `settings` interface from the WIT world:

```wit
interface settings {
  /// Returns the JSON-encoded value for `key`, or `none` if unset.
  /// `key` is relative to the plugin's namespace.
  get: func(key: string) -> option<string>;
}
```

The host implementation routes the call through the per-plugin
`PluginSettings` handle that the bridge stashes on the wasmtime
store data when `enable()` is called. The handle adds the
`plugins.<id>.` namespace prefix, so guests cannot escape their
own bucket no matter what key string they pass.

Values cross the boundary as JSON-encoded strings (`true`, `42`,
`"hello"`, `{"a":1}`) — WIT has no opaque value type, so the
plugin parses the result with `serde_json::from_str` on its side.
A typical read looks like:

```rust
let verbose: bool = serde_json::from_str(
    &settings::get("verbose").unwrap_or_else(|| "false".into()),
).unwrap_or(false);
```

`settings::get` returns `none` only when the key has never been
set in the store — neither by the manifest's `[settings]` defaults
nor by a runtime write. Plugins typically fall back to a hard-coded
default in that case (the same default that lives in
`manifest.toml`).

### Reacting to changes: `lifecycle::on-setting-changed`

The lifecycle interface gains a third method:

```wit
interface lifecycle {
  enable: func();
  disable: func();
  on-setting-changed: func(key: string, value: string);
}
```

The host's `WasmPluginBridge::setting_changed` override re-encodes
the `serde_json::Value` from the dispatcher to a JSON string and
calls the guest export. By the time the bridge fires, the
`CoalescingDispatcher` has already deduplicated rapid same-key
writes — plugin code never sees flapping intermediate values
during a slider drag.

### Bridge wiring

`WasmPluginBridge::enable()` clones `ctx.settings` and stashes it
on `PluginState` *before* invoking the guest's `enable()` so that
the guest can call `settings::get` from inside its own
initialization. `WasmPluginBridge::disable()` clears the stashed
handle so subsequent host imports revert to the "unset" no-op
behavior.

### Manifest defaults

`[settings]` defaults declared in `manifest.toml` are wired into
the store by `WasmPluginBridge::initialize_settings()` via
`settings.ensure(key, json_value)` — same path as before D2 of
the migration. The new WIT interfaces only cover **runtime reads**
and **reactive updates**.
