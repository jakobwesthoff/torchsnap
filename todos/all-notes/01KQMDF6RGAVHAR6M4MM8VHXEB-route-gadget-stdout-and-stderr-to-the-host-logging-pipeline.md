---
title: "Route gadget stdout and stderr to the host logging pipeline"
kind: feature
component: gadget-host
tags: [logging]
---
# Route gadget stdout and stderr to the host logging pipeline

Gadget WASM components are instantiated with `.inherit_stdout()` and `.inherit_stderr()`, so any `println!` or `eprintln!` in a gadget goes straight to the host terminal. That is convenient during development, but it bypasses the structured log pipeline (`log_sender` and `SpanRegistry`) that the DevTools window and future log exporters consume.

## Intent

Replace `WasiCtxBuilder::inherit_stdout()` and `inherit_stderr()` with a custom `AsyncWrite` sink (or a `wasi_common` pipe) that forwards each line as a `LogItem` through the existing `log_sender` channel, attributed to the originating gadget ID.

## Location

`src-tauri/src/wasm/runtime/engine.rs`: `WasmRuntime::instantiate`, the `WasiCtxBuilder` block.

## Considerations

- The WASI stdout sink must be non-blocking from the guest's perspective. A synchronous write to `log_sender` is acceptable, because the channel is bounded and gadgets are expected to log infrequently.
- The log level can default to `DEBUG` for stdout and `WARN` for stderr.
- `wasmtime-wasi` exposes `pipe()` helpers that make this straightforward.
