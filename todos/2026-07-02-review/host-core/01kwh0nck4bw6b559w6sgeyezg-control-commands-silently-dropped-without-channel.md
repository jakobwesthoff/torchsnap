# Control `query`/`dismiss` report success while the frontend command is silently dropped

**Kind:** bug
**Severity:** medium
**Area:** src-tauri/src/control/mod.rs, src-tauri/src/control/handlers/query.rs

## Problem

Control handlers that need the launcher frontend push a
`ControlCommand` through `ControlChannelState::send`
(`src-tauri/src/control/mod.rs:84-94`):

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

- channel is `None` (no `control_subscribe` has happened yet, or
  a previous send already detected a closed channel), or
- `ch.send()` fails (webview destroyed) — the command that
  triggered the detection is itself lost.

`QueryHandler` (`handlers/query.rs:35-38`) and `DismissHandler`
(`handlers/launcher.rs:122-129`) call `send` and then return
`Ok(json!({"ok": true}))` unconditionally. The external client is
told the query was set / state was cleared even when nothing was
delivered.

The subscription only exists after the launcher webview's React
tree mounts: `useControlChannel`
(`src/launcher/hooks/useControlChannel.ts:47-69`) creates the
channel and invokes `control_subscribe` inside a mount
`useEffect`. So there is a window from app start (and from any
future webview destroy/recreate) during which every `query` /
`dismiss` state-clear is dropped while reporting `ok: true`.

The shipped documentation works around this without saying so:
the scripting example in `docs/control-api.md:219-225` inserts
`sleep 0.5` between `show` and `query`.

## Impact

- `echo show; echo query` style automation intermittently loses
  the query with no error signal; scripts must cargo-cult sleeps.
- After a webview-destroy event, the *first* control command is
  always swallowed (it is the send that detects the closed
  channel), even though the handler reports success.
- `dismiss` degrades silently too: the window still hides (that
  part goes through `request_launcher_dismiss`), but the
  query/selection reset is lost, so the next show can display
  stale state — exactly what `dismiss` is documented to prevent
  (`docs/control-api.md:127-131`).

## Suggested fix

Make `ControlChannelState::send` return a `Result` (delivered /
no-subscriber) and have `QueryHandler`/`DismissHandler` map the
failure to a JSON-RPC error (fits the documented `-1` invalid
state code: "a precondition is not met"). Alternatively buffer
the last undelivered command and flush it on `control_subscribe`,
which would also close the show→query race without client-side
sleeps. Decide which contract the doc should promise, then align
doc + code.
