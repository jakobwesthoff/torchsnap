# Use spawn_blocking instead of std::thread::spawn for Tokio-dependent threads

## Context

Several plugins use `std::thread::spawn` for background threads that call
`blocking_changed()` or other code relying on `tauri::async_runtime::block_on`.
While this currently works because the global Tauri runtime handle is accessible
from any thread, it's not clean — any future code in those threads that touches
reqwest or other Tokio I/O would panic with "no reactor running".

## Affected code

- `src-tauri/src/plugins/calculator.rs` lines ~496, ~510: settings watcher
  threads and retention thread use `std::thread::spawn` with `blocking_changed()`
- Any other plugin that follows the same pattern

## Recommended fix

Replace `std::thread::spawn` with `tauri::async_runtime::spawn_blocking` for
threads that call into async/Tokio code (like `blocking_changed`). Threads
that are purely synchronous (e.g., `Condvar::wait_timeout` loops with no
async calls) can remain as `std::thread::spawn`.

## Discovered during

Implementation of the `WebsiteMetadataService`, where `std::thread::spawn`
for background favicon fetches panicked because reqwest requires the Tokio
reactor. Fixed there by switching to `spawn_blocking`.
