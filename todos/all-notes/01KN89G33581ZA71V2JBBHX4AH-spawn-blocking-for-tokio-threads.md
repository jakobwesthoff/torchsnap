---
kind: improvement
status: open
---

# Use spawn_blocking instead of std::thread::spawn for Tokio-dependent threads

## Context

Several gadgets use `std::thread::spawn` for background threads that call
`blocking_changed()` or other code relying on `tauri::async_runtime::block_on`.
While this currently works because the global Tauri runtime handle is accessible
from any thread, it's not clean — any future code in those threads that touches
reqwest or other Tokio I/O would panic with "no reactor running".

## Affected code

- The original calculator gadget (deleted in the WASM port) used
  `std::thread::spawn` for its retention cleanup loop. The WASM
  rewrite replaced that with the host-managed scheduler in
  `src-tauri/src/wasm/bridge.rs::scheduler_loop`, which calls the
  guest export inline from the tokio task — see ADR 0032 and the
  comment block in `scheduler_loop` explaining when a future
  pathological task could justify wrapping in `spawn_blocking`.
- Any remaining native gadget that follows the
  `std::thread::spawn` + `blocking_changed()` pattern.

## Recommended fix

Replace `std::thread::spawn` with `tauri::async_runtime::spawn_blocking` for
threads that call into async/Tokio code (like `blocking_changed`). Threads
that are purely synchronous (e.g., `Condvar::wait_timeout` loops with no
async calls) can remain as `std::thread::spawn`.

## Discovered during

Implementation of the `WebsiteMetadataService`, where `std::thread::spawn`
for background favicon fetches panicked because reqwest requires the Tokio
reactor. Fixed there by switching to `spawn_blocking`.
