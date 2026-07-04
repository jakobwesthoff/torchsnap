# Disabling the Control API leaves established client connections alive

**Kind:** bug
**Severity:** medium
**Area:** src-tauri/src/control/mod.rs

## Problem

`docs/control-api.md:17-18` documents the settings toggle as:
"Turning the toggle off closes the socket and disconnects all
clients." The code only does the first half.

`ControlServer::stop` (`src-tauri/src/control/mod.rs:165-172`)
sends `true` on the shutdown watch channel and removes the socket
file:

```rust
fn stop(&mut self) {
    if let Some(tx) = self.shutdown_tx.take() {
        let _ = tx.send(true);
    }
    if let Some(path) = self.socket_path.take() {
        let _ = std::fs::remove_file(&path);
    }
}
```

The shutdown signal is only observed by the *accept loop*
(`mod.rs:139-157`, `tokio::select!` over `listener.accept()` and
`shutdown_rx.changed()`). Per-connection tasks are spawned
detached (`mod.rs:145-147`, `tokio::spawn(handle_connection(...))`)
and `handle_connection` (`mod.rs:182-207`) loops on
`reader.read_line` until client EOF or a write error. No shutdown
signal reaches it.

Unlinking the socket path does not close already-accepted Unix
stream fds, so every client connected before the toggle-off keeps
a fully functional JSON-RPC session (show, hide, query, dismiss,
status all keep working) for as long as it holds the connection
open.

## Impact

- Turning the Control API off in Settings does not revoke access
  for currently connected clients; only *new* connections are
  prevented. A long-lived automation client keeps driving the
  launcher indefinitely after the user disabled the feature.
- Behavior contradicts the documented contract in
  `docs/control-api.md` ("disconnects all clients").
- Same-user local socket, so this is a correctness/lifecycle bug
  rather than a privilege issue, but "disable" silently not
  disabling is surprising.

## Suggested fix

Propagate the shutdown signal into connection tasks: pass a clone
of `shutdown_rx` to `handle_connection` and `tokio::select!` it
against `read_line` (breaking the loop drops the stream and
closes the connection). Alternatively track spawned connection
task handles and abort them in `stop()`. Either way, add a test
that a connected client's next request fails after the setting is
toggled off.
