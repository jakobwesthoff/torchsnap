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
