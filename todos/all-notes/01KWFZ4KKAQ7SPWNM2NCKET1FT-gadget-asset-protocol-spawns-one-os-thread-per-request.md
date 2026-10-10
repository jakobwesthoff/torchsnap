---
title: "Gadget asset protocol spawns one OS thread per request"
kind: improvement
component: gadget-host
impact: low
---
# Gadget asset protocol spawns one OS thread per request

The `torchsnap-gadget://` protocol handler spawns a fresh OS thread for every asset request. A custom-UI gadget that loads many assets at once therefore creates that many short-lived threads, with no upper bound. Offloading is right, since the handler does blocking I/O and may hold a Mutex, but a raw spawn per request has no limit.

## Problem
The handler in `src-tauri/src/wasm/protocol.rs` (around line 62) spawns a thread per request. The comment there gives the reason:

```rust
// Spawn blocking because GadgetSource::read_file may
// hold a Mutex (ArchiveSource) and do file I/O.
std::thread::spawn(move || {
    let response = handle_request(&registry, &request);
    responder.respond(response);
});
```

The stated reason (blocking I/O plus a possible Mutex hold) is valid. Raw `thread::spawn` per request is the problem: a gadget that loads many assets at once (JS bundle, CSS, fonts, image grid) creates that many OS threads.

## Impact
Thread-creation overhead and unbounded concurrency on asset bursts. Not a correctness problem today. It becomes one if `read_file` on `ArchiveSource` serializes on its Mutex anyway, in which case the threads mostly queue on the lock.

## Proposal
Route the blocking work through a bounded pool: `tauri::async_runtime::spawn_blocking` (tokio's blocking pool) gives a capped, reusable thread pool with no new infrastructure. The same pattern is covered by a separate todo; this file is another call site for that sweep.

## Relations

- Relates to: [Use spawn_blocking instead of std::thread::spawn for Tokio-dependent threads](01KN89G33581ZA71V2JBBHX4AH-use-spawnblocking-instead-of-stdthreadspawn-for-tokio-dependent-threads.md), the same pattern at another call site.
