# Gadget Messaging

## Overview

Gadget messaging is the custom RPC channel between a gadget's frontend
view and its backend handler. It runs *parallel* to the search pipeline:
a search query never travels through this path, and a custom message
never travels through the search path.

End-to-end shape:

```
gadget frontend
    │   sendMessage(method, payload, onMessage?)
    ▼
@torchsnap/gadget-sdk/hooks (shim → window.__torchsnap)
    │
    ▼
src/lib/gadgetMessage.ts → invoke("gadget_message", { source, method, payload, channel })
    │
    ▼
src-tauri commands::gadget_message  (Tauri command, async, spawn_blocking)
    │
    ▼
GadgetHost::handle_message(source, method, payload, channel)
    │   match by gadget id (source)
    ▼
Gadget::handle_message(method, payload, channel)
    │
    ├── native impl  → may use channel for streaming
    └── WasmGadgetBridge::handle_message  → discards channel,
                                            calls WIT messaging::handle-message
```

## Tauri Command

`src-tauri/src/commands/mod.rs::gadget_message`:

```rust
#[tauri::command]
pub async fn gadget_message(
    source: String,
    method: String,
    payload: serde_json::Value,
    channel: tauri::ipc::Channel<serde_json::Value>,
    state: State<'_, Arc<GadgetHost>>,
) -> Result<serde_json::Value, String>
```

Async because gadget handlers run on `tokio::task::spawn_blocking`
(handlers may issue `reqwest` HTTP calls that need a Tokio runtime
context). Only the outermost error is formatted into the rejection
string. The full `anyhow` chain stays in host logs.

## Host Routing

`GadgetHost::handle_message` (`gadget_host.rs`) walks the registered
`GadgetSlot`s and dispatches to the slot whose `gadget.id() == source`.
Unknown ids return `anyhow!("unknown gadget source: {source}")`.

The host does no payload validation. Both `payload` and the return
value are arbitrary `serde_json::Value`; meaning is opaque to the
host.

## Gadget Trait

`Gadget::handle_message` in `src-tauri/src/gadgets/mod.rs`:

```rust
fn handle_message(
    &self,
    _method: &str,
    _payload: serde_json::Value,
    _channel: tauri::ipc::Channel<serde_json::Value>,
) -> anyhow::Result<serde_json::Value> {
    anyhow::bail!("gadget does not handle custom messages")
}
```

The default rejects everything; gadgets opt in by overriding. The
return value is the resolved Promise on the frontend side. The
`channel` allows pushing zero or more streamed updates *before* the
return value resolves (native gadgets only).

## WASM Bridge

`WasmGadgetBridge::handle_message` (`src-tauri/src/wasm/bridge.rs`)
re-encodes the `serde_json::Value` payload as a JSON string, calls the
guest's `messaging::handle-message` export, and parses the JSON-string
response back to `Value`. The streaming `_channel` is intentionally
ignored.

Errors split into three categories with explicit identities in logs:

- **Gadget-reported**: inner `err(string)` arm of the WIT `result`.
- **Bridge-level**: wasmtime trap, payload serialization, malformed
  response JSON. Wrapped via `anyhow::Context`.
- **Disabled-gadget guard**: `handle_message()` called while the
  instance slot is empty. Logged via `log_dispatched_while_disabled`,
  returns an error.

### WIT contract

```wit
interface messaging {
  handle-message: func(method: string, payload: string)
      -> result<string, string>;
}
```

`payload` and the success arm are JSON-encoded strings, the same
convention as `settings::get`. The guest parses with
`serde_json::from_str` and serializes the response with
`serde_json::to_string`.

### Why no streaming for WASM

WASM guests run inside wasmtime with no Tauri runtime access — they
cannot hold a `Channel<Value>` across async boundaries. The bridge
silently drops the channel rather than forwarding it; gadgets that
genuinely need streaming must stay native or wait for an additive
`messaging-stream` sub-interface.

## Rust SDK helpers

`gadgets/gadget-sdk/src/messaging.rs`:

```rust
pub fn parse_payload<T: DeserializeOwned>(payload: &str) -> Result<T, String>
pub fn to_response<T: Serialize>(value: &T) -> Result<String, String>
```

Both fold the `serde_json` diagnostic into the error string so log
readers can distinguish a malformed payload from a gadget-level
rejection.

Gadgets that don't expose RPC implement the noop stub via
`impl_noop_messaging!(MyGadget)`. The stub returns
`Err(format!("gadget does not handle messages: {method}"))` so
misrouted calls still surface in logs.

### Gadget-side example

From `gadgets/zerotier/src/lib.rs`:

```rust
impl MessagingGuest for ZeroTierGadget {
    fn handle_message(method: String, payload: String) -> Result<String, String> {
        match method.as_str() {
            "refresh"  => { /* ... */ Ok(r#"{"ok":true}"#.into()) }
            "reimport" => { /* ... */ }
            "forget"   => {
                #[derive(serde::Deserialize)] struct Req { id: String }
                let req: Req = serde_json::from_str(&payload)
                    .map_err(|e| format!("parse payload: {e}"))?;
                /* ... */
            }
            other => Err(format!("unknown method: {other}")),
        }
    }
}
```

## Frontend Side

### `sendGadgetMessage` (`src/lib/gadgetMessage.ts`)

```ts
export function sendGadgetMessage<TPayload, TResult, TStream = never>(
  source: string,
  method: string,
  payload: TPayload,
  onMessage?: (msg: TStream) => void,
): Promise<TResult>;
```

Always allocates an internal `Channel<TStream>` (Tauri requires the
parameter even when unused) and wires `channel.onmessage` to
`onMessage` when provided. Calls the `gadget_message` Tauri command.

### Gadget runtime hook

Gadget frontends do not call `sendGadgetMessage` directly. They consume
`GadgetRuntime` from `@torchsnap/gadget-sdk/hooks`:

```ts
const { sendMessage, logger } = useGadgetRuntime();
await sendMessage<RefreshReq, RefreshResp>("refresh", { force: true });
```

`sendMessage` is bound to the gadget id of the surrounding
`GadgetContextProvider`, so gadgets cannot address each other. The
provider constructs the bound function by partial-applying
`sendGadgetMessage` with the context's gadget id. See
[04-frontend-reception.md](04-frontend-reception.md#gadget-component-contract-adr-0028)
for the full provider and hook contract.

The `onMessage` parameter is part of the type signature for symmetry
with native gadgets, but the WASM bridge silently drops channel pushes.
Frontend code that runs against a WASM gadget must treat the resolved
Promise as the only data path.

### `useGadgetStream` (`src/hooks/useGadgetStream.ts`)

Wraps `sendMessage` in `useSyncExternalStore` and returns three
strictly separate states:

```ts
{ established: boolean,
  result: TResult | undefined,
  snapshot: TSnapshot | undefined }
```

- `snapshot` updates on every channel push (native streaming gadgets).
- `result` updates once when the Promise resolves.
- `established` flips true alongside `result`.

Re-issues the command when `method` or `payload` identity changes; on
unmount the channel is dropped and the backend is expected to detect
the close and stop pushing. With WASM gadgets `snapshot` stays
`undefined` for the lifetime of the call.

## Long-lived subscriptions (native only)

The native clipboard gadget reuses the channel as a long-lived
subscription: its `"search"` method stashes the frontend's `Channel`
in `SharedState::ActiveQuery`. When clipboard contents change, the
gadget re-runs the stashed query and pushes results through the
stored channel — one channel covers an entire session of result
updates. This pattern depends on holding `Channel<Value>` across
async events and is therefore not available to WASM gadgets.

## Control Channel (out of scope)

The Unix-domain `control.sock` JSON-RPC server in
`src-tauri/src/control/` is unrelated to gadget messaging. It exists
for external automation (CLI driving the launcher) and routes
`Dismiss` / `SetQuery` commands through a separate Tauri channel
consumed by the launcher's `useControlChannel` hook.
