# 15. Add teardown to plugin lifecycle

Date: 2026-03-27

## Status

Accepted

## Context

Plugins currently have no way to release resources when the application
shuts down or when they are disabled. The process exit kills everything,
but this is insufficient for:

- **Background threads** that should be signaled to stop gracefully
  (e.g., the clipboard watcher thread holds a `ShutdownChannel` from
  `clipboard-rs`).
- **Open file handles or database connections** that should be flushed
  and closed cleanly rather than relying on OS cleanup.
- **Future plugin enable/disable at runtime**, where a plugin must
  release resources without the process exiting.

## Decision

Both `CatalogPlugin` and `QueryPlugin` gain a `teardown()` method with
a default no-op:

```rust
fn teardown(&self) {}
```

The host calls `teardown()` when:

- The application is shutting down (via Tauri's `RunEvent::Exit` or
  equivalent).
- A plugin is being disabled at runtime (future capability).

`teardown()` runs on the main thread and must not block indefinitely.
Plugins should signal their background threads to stop and return
promptly. If a thread needs a grace period (e.g., to flush a
write-ahead log), it should be bounded to a few hundred milliseconds
at most.

`CatalogRegistry` gains a `teardown_all()` method that iterates all
registered plugins and calls `teardown()` on each.

## Consequences

- Plugins with background threads (clipboard watcher, future file
  watchers) can shut down cleanly.
- Enables future runtime plugin enable/disable without requiring process
  restart.
- No behavioral change for existing plugins — `teardown()` defaults to
  a no-op.
- The app's shutdown path must be wired to call `teardown_all()`.
