# 29. WASM plugin settings API

Date: 2026-04-08

## Status

Accepted

## Context

WASM plugins need the same settings access pattern that native plugins
already enjoy:

- **Read** their own settings on demand (e.g. during `enable()` to
  initialize cached state, or inside a search hot path to check a
  feature flag).
- **React** to user changes in near-real-time without polling.

The host has a fully working pipeline for both halves on the native
side:

- `PluginSettings` (`src-tauri/src/settings.rs`) wraps the app-wide
  settings store with a per-plugin namespace prefix
  (`plugins.<id>.`). It exposes a typed `get<T>(key)` accessor that
  deserializes from the stored JSON.
- `PluginContext.settings` is a `PluginSettings` handle handed to
  every plugin's `enable(app, ctx)`.
- `PluginHost::handle_setting_changed()` routes
  `plugins.<id>.<key>` writes through a per-plugin
  `CoalescingDispatcher` (ADR 0026) that deduplicates rapid same-key
  writes, then invokes `Plugin::setting_changed()` on the trait.

The WASM bridge previously dropped the `PluginContext` parameter on
the floor and never overrode `setting_changed()`. The trait method
existed but the bridge didn't forward it. WIT had no settings
interface at all — neither a host import for reads nor a guest
export for change notifications.

## Decision

**WIT additions** (`plugins/plugin-sdk/wit/torchsnap-plugin.wit`):

```wit
interface settings {
  /// Returns the JSON-encoded value for `key`, or `none` if unset.
  /// `key` is relative to the plugin's namespace.
  get: func(key: string) -> option<string>;
}

interface lifecycle {
  enable: func();
  disable: func();
  /// Called by the host whenever a setting in this plugin's
  /// namespace changes.
  on-setting-changed: func(key: string, value: string);
}

world plugin {
  import logging;
  import settings;     // NEW
  export lifecycle;
  export search;
}
```

`on-setting-changed` lives in the existing `lifecycle` guest
interface — WIT interfaces are unidirectional (each is either
imported or exported, not both), so the natural split is:
`interface settings { get: ... }` for the host import,
`interface lifecycle { enable, disable, on-setting-changed }` for
the guest exports. Creating a one-function `settings-events`
interface just to keep namespacing pure was rejected as noise
without benefit.

**Values cross the WIT boundary as JSON-encoded strings**
(`true`, `42`, `"hello"`, `{"a":1}`) instead of inventing a
dedicated WIT value variant. This keeps the boundary minimal and
matches how the rest of the host already encodes settings (the
underlying store is JSON-backed).

**Host wiring:**

1. `PluginState` (in `runtime.rs`) gains a
   `settings: Option<PluginSettings>` field. The bridge stashes a
   handle on `enable()` via the new `WasmPluginInstance::set_settings`
   wrapper and clears it on `disable()` via `clear_settings`.
2. `bindings::torchsnap::plugin::settings::Host` is implemented for
   `PluginState`. `get(key)` delegates to a new
   `PluginSettings::get_raw(key)` sibling that returns the
   JSON-encoded value as `Option<String>` (the existing `get<T>`
   deserializes; we want the raw JSON for the WIT crossing).
3. `WasmPluginBridge::enable()` stops ignoring `_ctx`. It clones
   `ctx.settings` and stashes it on the instance's `PluginState`
   *before* calling the guest's `enable()` so the guest can read
   settings during its own initialization.
4. `WasmPluginBridge::setting_changed()` overrides the trait
   default (which was a no-op). The override re-encodes the
   `serde_json::Value` to a JSON string and dispatches into the
   guest via `WasmPluginInstance::on_setting_changed`. The host's
   `CoalescingDispatcher` runs *before* the bridge gets called, so
   no extra throttling is needed at the bridge level.
5. The `settings` import is added to the linker automatically by
   `bindings::Plugin::add_to_linker(...)` — the existing call in
   `runtime.rs` picks up every interface in the world without
   per-interface plumbing.

**Plugin-side defaults remain in `manifest.toml`'s `[settings]`
table.** `WasmPluginBridge::initialize_settings()` already wires
those into the store via `settings.ensure(key, json_value)`. The
WIT additions only cover runtime reads and reactive updates.

**No SDK helper crate yet.** Plugins call `settings::get(key)`
directly and parse the returned JSON via `serde_json::from_str`.
A Rust SDK crate with typed wrappers
(`get_setting<T: DeserializeOwned>(key) -> Option<T>`) is a near-
term follow-up. The calculator port (which landed shortly after
this ADR) is the second user of the per-setting boilerplate; the
trigger for the SDK crate has fired and it should be created
before a third plugin lands and forces a churn-heavy retrofit.

(Trigger has since fired: the `torchsnap-plugin-sdk` crate under
`plugins/plugin-sdk/` now exposes `settings::get<T>` /
`settings::get_or<T>` / `settings::get_or_else<T>`, and every
bundled plugin consumes those helpers in place of the raw
`serde_json::from_str` dance described above.)

## Consequences

- WASM plugins now have read+react parity with native plugins on
  settings.
- Plugins are confined to their own namespace at the host side —
  the `plugins.<id>.` prefix is added by `PluginSettings::get_raw`,
  so guests cannot escape their bucket no matter what key they
  pass.
- The `CoalescingDispatcher` deduplication runs once on the host
  side and serves both native and WASM plugins identically. No
  per-bridge throttle, no duplicate logic.
- Re-enabling after disable hands a fresh `PluginContext` and
  re-stashes the settings handle, so plugin state stays consistent
  across enable cycles.
- Adding a new lifecycle method (`on-setting-changed`) is a
  WIT-breaking change for any existing guest. Every shipped plugin
  must implement it (even as a no-op) or fail to compile against
  the new world. The `template`, `hello-world`, and untracked
  `calculator` template-copy crates were updated in the same
  commit.

## Alternatives considered

- **A new `PluginSettingsHook` host import per setting** — too
  fine-grained, and forces the host to track per-key listeners.
- **Polling (`settings::get` from inside `search()`)** — works for
  read but misses change notifications, forcing every plugin to
  refetch on every keystroke.
- **A dedicated WIT value variant** instead of JSON strings —
  doubles the binding surface for no real win, since the store is
  already JSON-backed and `serde_json` is in every plugin's
  dependency closure anyway.
- **Folding `on-setting-changed` into a new `settings-events`
  guest interface** — pure namespacing with no functional benefit;
  rejected as overhead.
