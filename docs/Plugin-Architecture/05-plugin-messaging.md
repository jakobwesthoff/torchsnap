# Plugin Messaging (Custom IPC)

## Overview

Beyond the search pipeline, plugins can communicate bidirectionally with their
frontend views via `handle_message()`. This is a separate channel from the
search result flow.

## Backend

### Tauri Command

`search/mod.rs` exposes:
```rust
pub fn plugin_message(source, method, payload, channel: Channel<Value>, state)
    -> Result<Value, String>
```

Routes to the matching plugin's `handle_message(method, payload, channel)`.
The method returns a `Value` (the response), and the `channel` allows
streaming push updates before the promise resolves.

### Plugin Trait Method

```rust
fn handle_message(&self, method: &str, payload: Value, channel: Channel<Value>)
    -> Result<Value, Error>
```

Default implementation returns `json!({})`.

## Frontend

### sendPluginMessage

`src/lib/pluginMessage.ts`

```typescript
function sendPluginMessage<TPayload, TResult, TStream>(
  source: string,
  method: string,
  payload: TPayload,
  onMessage?: (msg: TStream) => void,
): Promise<TResult>
```

Creates a `Channel<TStream>` internally. If `onMessage` is provided, it
becomes `channel.onmessage`. Returns a promise that resolves with the Tauri
command's return value.

### usePluginStream Hook

`src/hooks/usePluginStream.ts`

Wraps `sendPluginMessage` in `useSyncExternalStore`. Exposes:

```typescript
{ established: boolean, result: TResult | null, snapshot: TStream | null }
```

- `snapshot` — updates via channel messages (streaming)
- `result` — updates when the promise resolves (final)

These two data paths are strictly separated.

## WASM Plugins (ADR 0030)

WASM plugins reach the same Tauri command (`plugin_message`) as
native plugins, but the bridge to the actual handler runs through
the WIT `messaging` guest export instead of the trait method:

```wit
interface messaging {
  /// Handle a custom message from the plugin's frontend.
  handle-message: func(method: string, payload: string)
      -> result<string, string>;
}
```

`WasmPluginBridge::handle_message` overrides the default trait
impl, re-encodes the incoming `serde_json::Value` payload as a
JSON string, calls the guest export, and parses the response
back into a `Value` for the Tauri command return. The native
trait's `channel: Channel<Value>` parameter is intentionally
ignored — **WASM plugins are strictly request/response**.

### Error propagation

Errors flow through three layers, each with a clear identity in
logs:

- **Plugin-reported errors** — the inner `err(string)` arm of
  the WIT `result`. The bridge wraps these with the prefix
  `"plugin error: "` so log readers can distinguish them from
  bridge-level failures.
- **Bridge-level failures** — wasmtime traps, payload
  serialization issues, malformed JSON in the plugin's response.
  Wrapped via `anyhow::Context` with explicit context strings
  (`serialize handle_message payload`,
  `invoke guest handle-message`,
  `parse guest handle-message response`).
- **Trait-level failures** — bubble up to the Tauri command and
  reject the frontend's `sendMessage` promise.

### Why WASM cannot stream

WASM plugins live inside a wasmtime sandbox with no access to
the Tauri runtime, which means they can't hold a `Channel<Value>`
across multiple async events. The `_channel` parameter on the
bridge's override is intentionally ignored, not silently
forwarded — accidental forwarding would crash on the first send.

If a future plugin genuinely needs streaming, the path is to add
a separate `messaging-stream` sub-interface to the WIT world,
ideally piggybacking on whatever the channel-lifecycle todo
(see `todos/01kn30th27x0arv9a5cekkwgpw…`) lands as the general
solution. Adding such an interface is purely additive and
breaks nothing.

### No SDK helper crate yet

Plugins parse / serialize JSON inline via `serde_json::from_str`
and `serde_json::to_string`. Helper functions like
`parse_payload<T: DeserializeOwned>` and `to_response<T: Serialize>`
would trim two lines per method but aren't worth a dedicated
crate until there are multiple WASM plugins to share them.

## Reactive Subscription Pattern (Clipboard)

The clipboard plugin uses `handle_message` to implement a long-lived reactive
subscription. The `"search"` method stores the frontend's `Channel` as the
`ActiveQuery` in `SharedState`. When clipboard data changes (watcher captures
new content, user deletes/pastes/clears), `refresh_active_query()` re-runs the
stored query and pushes new results through the stored channel.

This means one `Channel` lives for the duration of a frontend search session,
receiving multiple updates — unlike the typical request/response pattern.

## Control Channel (External IPC)

`src-tauri/src/control/mod.rs`

A separate Unix domain socket server (`control.sock`) speaks JSON-RPC 2.0 over
newline-delimited JSON. Gated by `controlChannel.enabled` setting.

Commands:
```rust
pub enum ControlCommand {
    Dismiss,
    SetQuery { text: String },
}
```

The frontend `useControlChannel` hook receives these via a Tauri channel and
calls `resetState()` or `setQuery()` accordingly. This allows CLI or external
automation to drive the launcher.
