# Redirect plugin stdout/stderr to the logging system

## Context

WASM plugins that call `println!` or write to stderr have their output
sent directly to the host terminal via `WasiCtxBuilder::inherit_stdout()`
and `inherit_stderr()`. This works during development but means plugin
output bypasses the in-app DevTools log panel entirely.

## Where

`src-tauri/src/wasm/runtime/engine.rs` in the `WasiCtxBuilder` setup
inside the plugin instantiation path (search for `inherit_stdout`).

## What's needed

Replace `inherit_stdout()` / `inherit_stderr()` with custom WASI pipe
writers that forward each line to the existing `log_sender` channel as
`LogLevel::Debug` (stdout) and `LogLevel::Warn` (stderr) entries, tagged
with the plugin ID. Wasmtime's `wasi-common` / `wasmtime-wasi` exposes
`AsyncWriteStream` / `OutputFile` seams for this.

## Notes

- The `log_sender` channel is already threaded through `PluginState` — the
  writer just needs a clone of the sender and the plugin ID at construction time.
- Plugin `logging::log` WIT calls already appear in DevTools; this covers
  raw `println!` / `eprintln!` that bypass the WIT logging interface.
