---
title: "Control query and dismiss report success when the frontend command is dropped"
kind: bug
component: backend
status: needs-discussion
---
# Control query and dismiss report success when the frontend command is dropped

Control `query` and `dismiss` return `ok: true` even when the command never reaches the launcher frontend. `ControlChannelState::send` discards the command silently when no subscriber exists yet or the webview is gone, so the client is told the query was set while nothing was delivered.

## Problem

Read in the code. Handlers that need the launcher frontend push a `ControlCommand` through `ControlChannelState::send` in `src-tauri/src/control/mod.rs`:

```rust
pub fn send(&self, command: ControlCommand) {
    let mut guard = self.channel.lock().expect("channel lock not poisoned");
    if let Some(ch) = guard.as_ref()
        && ch.send(command).is_err()
    {
        // Channel closed — webview was destroyed. ...
        *guard = None;
    }
}
```

`send` has no return value. Both drop cases are silent:

- the channel is `None` (no `control_subscribe` has happened yet, or a previous send already detected a closed channel), or
- `ch.send()` fails because the webview was destroyed. The command that triggered the detection is itself lost.

`QueryHandler` (`src-tauri/src/control/handlers/query.rs`) and `DismissHandler` (`src-tauri/src/control/handlers/launcher.rs`) call `send` and then return `Ok(json!({"ok": true}))` unconditionally.

The subscription only exists after the launcher webview's React tree mounts. `useControlChannel` (`src/launcher/hooks/useControlChannel.ts`) creates the channel and invokes `control_subscribe` inside a mount `useEffect`. So from app start, and from any future webview destroy and recreate, every `query` or `dismiss` state clear is dropped while reporting `ok: true`.

The shipped documentation works around this without saying so: the scripting example in `docs/control-api.md` inserts `sleep 0.5` between `show` and `query`.

## Impact

- `echo show; echo query` style automation intermittently loses the query with no error signal, so scripts must rely on sleeps.
- After a webview-destroy event, the first control command is always swallowed (it is the send that detects the closed channel), even though the handler reports success.
- `dismiss` degrades silently too. The window still hides, because that part goes through `request_launcher_dismiss`, but the query and selection reset is lost. The next show can display stale state, which is what `dismiss` is documented to prevent (`docs/control-api.md`).

## Suggested fix

Make `ControlChannelState::send` return a `Result` (delivered or no subscriber) and have `QueryHandler` and `DismissHandler` map the failure to a JSON-RPC error. The documented `-1` invalid state code fits: "a precondition is not met".

Alternatively, buffer the last undelivered command and flush it on `control_subscribe`. That would also close the show-then-query race without client-side sleeps.

Decide which contract the doc should promise, then align doc and code.
