# Gadget asset protocol spawns one OS thread per request

**Kind:** improvement
**Severity:** low
**Area:** src-tauri/src/wasm/protocol.rs

## Problem
The `torchsnap-gadget://` protocol handler spawns a fresh OS
thread for every asset request
(`src-tauri/src/wasm/protocol.rs:62-65`):

```rust
// Spawn blocking because GadgetSource::read_file may
// hold a Mutex (ArchiveSource) and do file I/O.
std::thread::spawn(move || {
    let response = handle_request(&registry, &request);
    responder.respond(response);
});
```

The stated reason (blocking I/O plus a possible Mutex hold) is
valid, but raw `thread::spawn` per request means a custom-UI
gadget that loads many assets at once (JS bundle, CSS, fonts,
image grid) creates that many short-lived OS threads with no
upper bound.

## Impact
Thread-creation overhead and unbounded concurrency on asset
bursts. Not a correctness problem today; it becomes one if
`read_file` on `ArchiveSource` serializes on its Mutex anyway,
in which case the threads mostly queue on the lock.

## Suggested fix
Route the blocking work through a bounded pool instead:
`tauri::async_runtime::spawn_blocking` (tokio's blocking pool)
gives a capped, reusable thread pool with no new infrastructure.
Related existing todo:
`todos/backend/concurrency/01kn89g33581za71v2jbbhx4ah-spawn-blocking-for-tokio-threads.md`
covers the same pattern elsewhere; this file is another call
site for that sweep.
