---
title: "Use spawn_blocking instead of std::thread::spawn for Tokio-dependent threads"
kind: improvement
component: backend
origin: implementation
tags: [concurrency]
---
# Use spawn_blocking instead of std::thread::spawn for Tokio-dependent threads

Some background threads are started with `std::thread::spawn` and then call `blocking_changed()` or other code that relies on `tauri::async_runtime::block_on`. This works today because the global Tauri runtime handle is reachable from any thread, but it is not clean. Any future code in those threads that touches reqwest or other Tokio I/O panics with "no reactor running". The pattern was found while implementing `WebsiteMetadataService`, where background favicon fetches on `std::thread::spawn` panicked for that reason and were fixed by switching to `spawn_blocking`.

## Affected code

- The original calculator gadget (deleted in the WASM port) used `std::thread::spawn` for its retention cleanup loop. The WASM rewrite replaced that with the host-managed scheduler in `src-tauri/src/wasm/bridge.rs` (`scheduler_loop`), which calls the guest export inline from the tokio task. See ADR 0032 and the comment block in `scheduler_loop` that explains when a future pathological task could justify wrapping in `spawn_blocking`.
- `src-tauri/src/network/website_metadata/mod.rs`: the retention thread started with `std::thread::spawn` calls `ttl_watch.blocking_changed()`. This is the one remaining case found on 2026-10-21. `std::thread::spawn` in `src-tauri/src/wasm/protocol.rs` was not checked for async calls.

## Recommended fix

Replace `std::thread::spawn` with `tauri::async_runtime::spawn_blocking` for threads that call into async or Tokio code (such as `blocking_changed`). Threads that are purely synchronous (for example `Condvar::wait_timeout` loops with no async calls) can stay as `std::thread::spawn`.
