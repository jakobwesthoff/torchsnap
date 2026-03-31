# Settings Reactivity

## Overview

Plugin settings are stored via `tauri-plugin-store` (JSON file). The frontend
writes settings, and the backend reacts to changes via a watch-based
notification system.

## Flow

1. Frontend writes to the Tauri store
2. The `"settings-changed"` Tauri event fires
3. `lib.rs` listener calls `notifier.notify(key, value)`
4. All `tokio::sync::watch` receivers for that key receive the new value

## SettingsWatch<T>

`src-tauri/src/settings_notifier.rs`

Wraps a `watch::Receiver<Value>`, deserializes on `.get()`. Provides:
- `.get()` — current value
- `.changed()` — async wait for next change
- `.blocking_changed()` — blocking wait (for `spawn_blocking` threads)

## PluginSettingsNotifier

Scoped per-plugin, automatically prepends `plugins.<id>.` to all key lookups.
Plugins call:

```rust
ctx.notifier.watch::<bool>("enabled")
```

and get back a `SettingsWatch<bool>` for `plugins.<plugin_id>.enabled`.

## Known Issues

The calculator plugin uses raw `*const AtomicBool` pointers cast through
`usize` to make them `Send` across thread boundaries for its settings watch
threads. The safety argument (the `Arc`-ed plugin outlives the thread) is
sound, but the pattern is unusual and fragile.
