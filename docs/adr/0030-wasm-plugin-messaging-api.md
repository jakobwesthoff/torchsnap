# 30. WASM plugin messaging API

Date: 2026-04-08

## Status

Accepted

## Context

WASM plugins need a way for their React frontends to call back into
the plugin's Rust code on demand. The native plugin trait already has
this capability:

```rust
fn handle_message(
    &self,
    method: &str,
    payload: serde_json::Value,
    channel: tauri::ipc::Channel<serde_json::Value>,
) -> anyhow::Result<serde_json::Value>;
```

The frontend uses the existing
`sendMessage(source, method, payload, onMessage?)` SDK helper (now
exposed via `usePluginRuntime().sendMessage`); the host routes the
call through `PluginHost::handle_message` to the per-plugin slot,
and the plugin's `handle_message` trait implementation does the
work. Native plugins like `clipboard` use this for live data
streams.

The `WasmPluginBridge` previously **never overrode** `handle_message`,
so when the host routed a message to a WASM plugin the call fell
through to the default `bail!("plugin does not handle custom messages")`.
Every `sendMessage` from a WASM plugin's frontend failed with that
error string.

WIT also had no messaging interface — there was no guest export for
the bridge to forward into.

## Decision

Add a standalone `messaging` guest interface to the WIT world and
override `WasmPluginBridge::handle_message` to dispatch into it.

**WIT additions** (`plugins/plugin-sdk/wit/torchsnap-plugin.wit`):

```wit
interface messaging {
  /// Handle a custom message from the plugin's frontend.
  handle-message: func(method: string, payload: string)
      -> result<string, string>;
}

world plugin {
  import logging;
  import settings;
  export lifecycle;
  export search;
  export messaging;     // NEW
}
```

**JSON-encoded strings cross the boundary** in both directions:
the bridge re-serializes the incoming `serde_json::Value` payload
and the plugin parses it on its side; the plugin's response is a
JSON string that the bridge parses back into a `serde_json::Value`
for the Tauri command return. Same encoding convention as
`settings::get` — keeps the WIT surface minimal and avoids
inventing a dedicated value variant.

**`err(string)` propagates as a Tauri command error** to the
frontend's `sendMessage` promise. The bridge wraps plugin-reported
errors as `"plugin error: <string>"` so logs can distinguish them
from bridge-level failures (linker, serialization, store lock).

**WASM plugins cannot stream.** The trait method's
`channel: Channel<Value>` parameter is intentionally ignored by
the bridge — the WIT export is strictly request/response. Native
plugins still get the full streaming `Channel` via the underlying
trait. If a future WASM plugin genuinely needs streaming, a
`messaging-stream` sub-interface can be added without breaking
this one.

**Bridge wiring** (`src-tauri/src/wasm/bridge.rs`):

```rust
fn handle_message(
    &self,
    method: &str,
    payload: serde_json::Value,
    _channel: tauri::ipc::Channel<serde_json::Value>,
) -> anyhow::Result<serde_json::Value> {
    let payload_json =
        serde_json::to_string(&payload).context("serialize handle_message payload")?;

    let result_json = self
        .instance
        .handle_message(method, &payload_json)
        .context("invoke guest handle-message")?
        .map_err(|e| anyhow::anyhow!("plugin error: {e}"))?;

    serde_json::from_str(&result_json).context("parse guest handle-message response")
}
```

`WasmPluginInstance::handle_message` is the new typed wrapper
around the WIT guest export, mirroring the existing wrappers for
`enable`/`disable`/`entries`/etc. The new `messaging` export is
picked up by `bindings::Plugin::add_to_linker(...)` automatically
— no per-interface plumbing needed.

**No SDK helper crate yet.** Plugins parse / serialize JSON
inline via `serde_json::from_str` and `serde_json::to_string`.
Helper functions like `parse_payload<T: DeserializeOwned>` would
trim two lines per method but aren't worth a dedicated crate
until there are multiple WASM plugins to share them.

(Trigger has since fired: the `torchsnap-plugin-sdk` crate under
`plugins/plugin-sdk/` now exposes `messaging::parse_payload<T>` /
`messaging::to_response<T>`, and the `impl_noop_messaging!` macro
covers the stub case for plugins that declare no frontend RPC
methods.)

## Alternatives considered

- **Folding `handle-message` into `lifecycle`** — rejected.
  Lifecycle is for plugin-state transitions; messaging is a
  separate concern with its own future evolution path
  (streaming, server-sent events, etc.). Standalone interface
  per the WIT type-organization convention.
- **Inventing a WIT value variant for the payload / response**
  — would double the binding surface for no real win; the
  store and frontend already speak JSON.
- **Streaming via `Channel` exposed to WIT** — out of scope.
  WASM plugins can't trivially hold a Tauri `Channel` (no
  Tauri runtime in the wasmtime sandbox), and the existing
  channel-lifecycle todo (see `todos/01kn30th27x0arv9a5cekkwgpw…`)
  needs to land first to define the lifecycle semantics. Adding
  a `messaging-stream` sub-interface later is purely additive.
- **A proc macro for dispatching by method name on the plugin
  side** — premature. Plain `match` works fine for the v1
  template. A macro can come later when real plugins
  demonstrate the ergonomic case.

## Consequences

- WASM plugins get frontend ↔ backend RPC parity with native
  plugins for the request/response shape (which is the vast
  majority of `sendMessage` use cases).
- Same Tauri command (`plugin_message`) routes both native and
  WASM plugins. Frontend code is unchanged.
- Adding a new lifecycle method (`handle-message`) is a
  WIT-breaking change for any existing guest. Every shipped
  plugin must implement it (even as a no-op or `Err(unknown)`)
  or fail to compile against the new world. The `template`,
  `hello-world`, and untracked `calculator` template-copy
  crates were updated in the same commit.
- Errors propagate naturally: WIT `Result<String, String>` →
  bridge `anyhow::Result<Value>` → Tauri `Result<Value, String>`
  → JS Promise rejection. Identical UX to native plugin
  errors.
- The `_channel` parameter being ignored is documented in the
  bridge — silent acceptance would be confusing if a future
  contributor added streaming support and forgot to wire it.
