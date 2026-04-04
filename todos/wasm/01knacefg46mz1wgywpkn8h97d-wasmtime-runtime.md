# wasmtime Runtime

Implement the core wasmtime integration: a shared `WasmRuntime` holding one `Engine` for all plugins, and a per-plugin `WasmPluginInstance` holding the `Store` and typed component bindings.

**Strategy doc:** §5.6 (binding mechanics), §5.8 (hello-world WIT, `PluginState` + logging host impl), §5.4 (WASI context setup, stdout/stderr redirect), §9 (threading model, `Mutex<Store>`)

**Status:** not started

**Files:**
- `src-tauri/src/wasm/bindings.rs` — `bindgen!` macro invocation
- `src-tauri/src/wasm/runtime.rs` — `WasmRuntime`, `WasmPluginInstance`, `PluginState`, `logging::Host` impl

**Notes:** One shared `Engine` per host process; per-plugin `Store` wrapped in `Mutex`. WASI context via `WasiCtxBuilder`: no filesystem, no network, no env vars; stdout→`info`, stderr→`warn` tagged with plugin ID. Start with `async: false` in `bindgen!`.
