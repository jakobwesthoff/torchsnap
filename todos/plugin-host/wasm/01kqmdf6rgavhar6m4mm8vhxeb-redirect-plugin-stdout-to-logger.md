# Redirect plugin stdout/stderr to the host logging system

Plugin WASM components are instantiated with `.inherit_stdout()` and
`.inherit_stderr()`, so any `println!` / `eprintln!` in a plugin goes
directly to the host terminal. This is convenient during development but
bypasses the structured log pipeline (`log_sender` / `SpanRegistry`) that
the DevTools window and future log exporters consume.

## Intent

Replace `WasiCtxBuilder::inherit_stdout()` / `inherit_stderr()` with a
custom `AsyncWrite` sink (or a `wasi_common` pipe) that forwards each
line as a `LogItem` through the existing `log_sender` channel, attributed
to the originating plugin ID.

## Location

`src-tauri/src/wasm/runtime/engine.rs` — `PluginEngine::load_plugin`,
the `WasiCtxBuilder` block.

## Considerations

- The WASI stdout sink must be non-blocking from the guest's perspective;
  a synchronous write to `log_sender` is acceptable because the channel
  is bounded and plugins are expected to log infrequently.
- Log level can default to `DEBUG` for stdout and `WARN` for stderr.
- `wasmtime-wasi` exposes `pipe()` helpers that make this straightforward.
