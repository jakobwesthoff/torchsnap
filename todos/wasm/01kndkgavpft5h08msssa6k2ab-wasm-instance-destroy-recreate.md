# WASM Instance Destroy/Recreate on Disable

When a WASM plugin is disabled, the instance should be fully destroyed (dropped)
to free memory (e.g., 50k petnames in hello-world). On re-enable, the instance
is recreated from the source and `enable()` is called again.

## Current State

- `WasmPluginBridge` holds `WasmPluginInstance` directly (always present)
- On disable: `disable()` is called on the WIT side, `AtomicBool` gates queries
- Instance stays alive consuming memory even when disabled

## Target Architecture

- `WasmPluginBridge.instance` becomes `Mutex<Option<WasmPluginInstance>>`
- Bridge holds `Arc<WasmRuntime>` and `Arc<Vec<u8>>` (WASM bytes) for
  re-instantiation
- On disable: call `instance.disable()`, then drop → `None`
- On enable: `runtime.instantiate(id, &wasm_bytes)` → `Some(instance)`,
  call `enable()`

## Optimization: Compiled Component Caching

`WasmRuntime` should cache the compiled `wasmtime::Component` per plugin
(keyed by plugin ID). Compilation is expensive (~100ms+), instantiation
is fast. This avoids recompiling on every re-enable cycle. Related to
the wasmtime-precompilation todo.

## Refactoring Required

- `WasmRuntime` needs to be `Arc`-shared (currently consumed during loading)
- WASM bytes need retention (currently read and dropped)
- All query methods already check `is_enabled()` — no change needed there
- `setup()` watch loop needs the disable path to drop the instance and
  the enable path to recreate it
