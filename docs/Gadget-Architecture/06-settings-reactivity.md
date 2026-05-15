# Settings Reactivity

## Overview

Gadget settings are persisted in the `tauri-plugin-store` JSON file
(`settings.json`). Writes always originate from a frontend webview;
the host listens for the resulting Tauri event and routes the change
through three independent paths: an in-process watch-channel notifier
for non-gadget subsystems, a per-gadget coalescing dispatcher for
gadget reactivity, and a shortcut-reactor signal for global hotkey
re-registration.

Cross-window propagation (ADR 0006), per-gadget coalescing (ADR 0026),
and the WASM `settings` / `lifecycle::on-setting-changed` interfaces
(ADR 0029) are the three load-bearing mechanisms.

## Key Layout

| Key                              | Owner                     | Purpose                              |
| -------------------------------- | ------------------------- | ------------------------------------ |
| `enabled.<gadget-id>`            | host                      | gadget enable toggle                 |
| `gadgets.<gadget-id>.<setting>`  | gadget                    | gadget's own settings namespace      |
| `<top-level-key>`                | host / non-gadget systems | global app settings                  |

The enabled toggle lives at top level (`enabled.<id>`), *outside*
the `gadgets.<id>.*` namespace. Gadgets never observe their own
enabled-state writes — only the host acts on that key.

## Cross-Window Store (ADR 0006)

`src/settingsStore.ts` is a singleton wrapper around
`tauri-plugin-store` shared by every webview (launcher and settings
window). Each webview has its own JS `Store` `rid`; the underlying
Rust store is a singleton per file path, so a write in one window is
already visible to reads in another. Notification is the only piece
the wrapper adds:

1. `setSetting(key, value)` calls `store.set` + `store.save`, then
   `emit("settings-changed", { key })`.
2. `initStore()` registers a `listen("settings-changed", …)` that
   refetches the value from the store, updates an in-memory cache,
   and notifies local subscribers.
3. `getSettingSync<T>(key)` returns the cached value synchronously —
   the store is loaded fully before React mounts so first-render reads
   never see `undefined`.

`useSetting<T>(key)` (`src/hooks/useSetting.ts`) sits on top:
`useState` seeded from the cache, `subscribe` for updates,
`setSetting` for writes. `useGadgetSetting<T>(key)`
(`src/contexts/useGadgetSetting.ts`) prepends `gadgets.<id>.` derived
from the surrounding `GadgetContextProvider`.

## Backend Listener

`src-tauri/src/lib.rs` registers a single listener for
`"settings-changed"`:

```rust
app.listen("settings-changed", move |event| {
    let payload: { key } = ...;
    let value = store.get(&payload.key).unwrap_or(Value::Null);

    notifier.notify(&payload.key, value.clone());
    host.handle_setting_changed(&payload.key, value, &app);

    if host.is_key_watched(&payload.key) {
        host.notify_shortcut_change();
    }
});
```

The three callees are independent:

- `SettingsNotifier::notify` — feeds `tokio::sync::watch` channels
  consumed by non-gadget subsystems via `SettingsWatch<T>`.
- `GadgetHost::handle_setting_changed` — routes to the affected gadget
  through its `CoalescingDispatcher`.
- `notify_shortcut_change` — wakes the shortcut reactor so global
  hotkeys are re-registered.

## Host Routing — `handle_setting_changed`

`src-tauri/src/gadget_host.rs`. Two prefix branches:

**Path 1 — `enabled.<gadget-id>`**

The dispatcher is reused as a serialization point even for the
enable/disable toggle. The callback flips
`GadgetSlot::enabled: AtomicBool` and, on a real transition, calls
`gadget.enable()` or `gadget.disable()`. The `Gadget::enable()`
method takes no arguments — capabilities are available as fields on
`self` (set during construction). Always signals shortcut
re-registration before returning.

**Path 2 — `gadgets.<gadget-id>.<setting-key>`**

Splits at the first `.` after the prefix, looks up the slot, and
enqueues the relative `setting-key` into the slot's
`CoalescingDispatcher`. The callback simply forwards
`(key, value)` to `gadget.setting_changed(key, value)`. Gadgets see
relative keys (`"retentionDays"`), never the full path.

## CoalescingDispatcher (ADR 0026)

`src-tauri/src/coalescing_dispatcher.rs`. One per gadget slot. Two
mutexes:

- `pending: Mutex<Vec<(String, Value)>>` — the queue.
  `enqueue(key, value)` does `retain(|(k,_)| k != &key); push(...)`,
  so same-key updates are deduplicated to the latest value while
  preserving chronological order across distinct keys.
- `work: Mutex<()>` — held for the duration of dispatch.
  `dispatch()` uses `try_lock`; if another dispatch is already
  running, this call returns immediately and the active loop will
  pick up newly enqueued items in its next iteration.

The dispatch loop drains the queue with `mem::take`, processes the
batch, and re-checks for items that arrived during processing.
Threading model:

- `enqueue` is called from the Tauri event listener (main thread),
  briefly holding `pending`.
- `dispatch` runs the callback inline. For Path 2 the callback is
  `gadget.setting_changed(k, v.clone())`, which for native gadgets
  is whatever the gadget implements and for WASM gadgets crosses the
  wasmtime boundary.

`pending` is always released before the callback runs, so the two
mutexes never nest.

## Native Gadget API

`Gadget` trait (`src-tauri/src/gadgets/mod.rs`):

```rust
fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit { settings }
fn setting_changed(&self, _key: &str, _value: serde_json::Value) {}
```

`initialize_settings` runs once at startup, before `enable()`. It
receives a `SettingsInit` already loaded from the store under the
gadget's prefix; the gadget chains `.ensure(key, default)` calls to
fill missing keys without overwriting user values.

`setting_changed` is the runtime hook, called by the dispatcher with
relative keys.

## WASM Gadget API (ADR 0029)

### Bridge wiring

`WasmGadgetBridge` (`src-tauri/src/wasm/bridge.rs`):

- `initialize_settings` reads `[settings]` from the manifest
  (`manifest.toml`) and calls `settings.ensure(key, json_value)` for
  every entry, converting TOML values to JSON.
- `enable()` clones `ctx.settings` (a `GadgetSettings`) and stashes
  it on the wasmtime store data via `instance.set_settings(...)`
  *before* invoking the guest's `enable()`, so the guest can call
  `settings::get` from within its own initialization.
- `disable()` calls `instance.clear_settings()` so subsequent host
  imports see the unset/no-op behavior.
- `setting_changed` re-encodes the `serde_json::Value` to a JSON
  string and calls `instance.on_setting_changed(key, &json)`. By the
  time this fires, the `CoalescingDispatcher` has already collapsed
  rapid same-key writes.

`GadgetSettings` (`src-tauri/src/settings.rs`) carries the
`Arc<Store>` plus a `gadgets.<id>.` prefix. `get_raw(key)` returns the
raw JSON-encoded string for the WIT crossing; guests cannot escape
their bucket regardless of the key string they pass.

### WIT contracts

```wit
interface settings {
  get: func(key: string) -> option<string>;
}

interface lifecycle {
  enable: func() -> result<_, string>;
  disable: func();
  on-setting-changed: func(key: string, value: string);
}
```

Values cross the boundary as JSON-encoded strings (`"true"`, `"42"`,
`"\"hello\""`, `"{\"a\":1}"`) — WIT has no opaque value type. `none`
means the key has never been set in the store, neither by manifest
defaults nor by a runtime write. `enable` returns a `result` — an
`err(string)` disables the gadget and the string is logged.

### SDK helpers

`gadgets/gadget-sdk/src/settings.rs`:

```rust
pub fn get<T: DeserializeOwned>(key: &str) -> Option<T>
pub fn get_or<T: DeserializeOwned>(key: &str, default: T) -> T
pub fn get_or_else<T: DeserializeOwned>(key: &str, f: impl FnOnce() -> T) -> T
```

`get` returns `None` for both "unset" and "present but failed to
deserialize"; callers that need to distinguish use the lower-level
`settings_host::get` re-export.

Typical guest code:

```rust
use torchsnap_gadget_sdk::prelude::*;

let manual: String = settings::get_or_else("manualToken", String::new);
```

### Reacting to changes

```rust
impl LifecycleGuest for ZeroTierGadget {
    fn on_setting_changed(key: String, _value: String) {
        if key == "manualToken" {
            // re-resolve token, invalidate caches, etc.
        }
    }
}
```

`value` is the new JSON-encoded string. Gadgets typically re-read
through the typed `settings::get_or` helper rather than parsing
`value` inline — the WIT argument exists so the guest doesn't have to
cross the boundary again, but type-safe access is more ergonomic.

## Manifest Defaults

`gadgets/<id>/manifest.toml`:

```toml
[settings]
manualToken = ""
retentionDays = 30
```

The bridge converts each value with `serde_json::to_value(toml_value)`
and feeds it to `SettingsInit::ensure`. Defaults never overwrite
existing user values.

## SettingsWatch (non-gadget only)

`src-tauri/src/settings_notifier.rs` keeps `SettingsWatch<T>`, a typed
wrapper over `tokio::sync::watch::Receiver<Value>`. Subscribers obtain
one via `SettingsNotifier::watch_with_initial(key, initial)` and call
`.get()`, `.changed().await`, or `.blocking_changed()`.

This is **not** part of the gadget-facing API; gadgets use
`setting_changed` / `on-setting-changed`. `SettingsWatch` survives
for non-gadget subsystems that observe global settings:

- `FrecencyStore` — watches its own retention / enable settings.
- `control` — watches the `controlChannel.*` keys to start/stop
  the Unix socket server.
- `WebsiteMetadataService` — watches cache / network settings.

## Frontend Settings UI

The Settings window builds gadget panels by importing each gadget's
React settings component. Components consume `useGadgetSetting<T>`:

```tsx
const [retentionDays, setRetentionDays] = useGadgetSetting<number>("retentionDays");
const [bringToFront, setBringToFront]  = useGadgetSetting<boolean>("bringToFrontOnPaste");
```

Writes go through `setSetting` → `emit("settings-changed", { key })` →
the backend listener fires the three-path fan-out described above.
The originating window receives the same event (no special-casing of
self-emitted events), so its own `useSetting` cache stays consistent
with the rest of the app through one code path.

The host also routes the `OpenSettings` action variant: when a search
result triggers `ActionId::OpenSettings`, `GadgetHost::execute`
emits `"open-gadget-settings"` with the originating gadget id so the
settings window can jump straight to that gadget's panel.
