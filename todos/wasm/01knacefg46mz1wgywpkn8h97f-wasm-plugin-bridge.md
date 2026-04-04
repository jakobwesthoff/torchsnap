# WasmPlugin Bridge

Implement `WasmPluginBridge`: the adapter that wraps a `WasmPluginInstance` and exposes it as whatever interface `PluginHost` needs (the existing `Plugin` trait or a refactored equivalent).

**Strategy doc:** §6.2 (adapter design, note that PluginHost modifications are on the table), §9 (threading: `Mutex<Store>` is permanent, not a compromise)

**Status:** not started

**Depends on:** `01knacefg46mz1wgywpkn8h97d` (wasmtime-runtime), `01knacefg46mz1wgywpkn8h97e` (type-conversions)

**File:** `src-tauri/src/wasm/bridge.rs`

**Discussion needed:** Whether `WasmPluginBridge` implements the existing `Plugin` trait directly, or whether `PluginHost` is refactored to accept a different abstraction. The strategy doc explicitly leaves PluginHost modifications on the table if they simplify integration.

**Notes:** Each bridge instance owns one `WasmPluginInstance` via `Mutex<Store>`. The bridge translates `Plugin` method calls into `call_enable()`, `call_entries()`, etc. on the typed component instance. Implements WIT host imports (logging initially) by delegating to real host APIs.
