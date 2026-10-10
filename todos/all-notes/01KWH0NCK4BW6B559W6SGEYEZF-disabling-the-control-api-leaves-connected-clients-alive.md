---
title: "Disabling the Control API leaves connected clients alive"
kind: bug
component: backend
---
# Disabling the Control API leaves connected clients alive

Turning the Control API off in Settings closes the listening socket, but clients that are already connected keep a working session. `docs/control-api.md` promises that turning the toggle off "closes the socket and disconnects all clients", so the documented contract is not met today.

## Problem

Read in the code. `ControlServer::stop` in `src-tauri/src/control/mod.rs` sends `true` on the shutdown watch channel and removes the socket file:

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

The shutdown signal is only observed by the accept loop (`tokio::select!` over `listener.accept()` and `shutdown_rx.changed()`). Per-connection tasks are spawned detached with `tokio::spawn(handle_connection(...))`, and `handle_connection` loops on `reader.read_line` until client EOF or a write error. No shutdown signal reaches it.

Unlinking the socket path does not close Unix stream file descriptors that were already accepted. Every client connected before the toggle-off keeps a fully working JSON-RPC session (show, hide, query, dismiss and status all keep working) for as long as it holds the connection open.

## Impact

- Turning the Control API off in Settings does not revoke access for clients that are already connected. Only new connections are prevented. A long-lived automation client keeps driving the launcher after the user disabled the feature.
- The behavior contradicts the documented contract in `docs/control-api.md`.
- This is a same-user local socket, so it is a correctness and lifecycle bug rather than a privilege issue. Still, "disable" silently not disabling is surprising.

## Suggested fix

Propagate the shutdown signal into connection tasks: pass a clone of `shutdown_rx` to `handle_connection` and `tokio::select!` it against `read_line`. Breaking the loop drops the stream and closes the connection. Alternatively, track the spawned connection task handles and abort them in `stop()`. Either way, add a test that a connected client's next request fails after the setting is toggled off.
