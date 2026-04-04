# Type Conversions

Implement `From`/`Into` conversions between the wasmtime-generated types (produced by `bindgen!`) and the native `search::types` used throughout the host.

**Strategy doc:** §5.5

**Status:** not started

**Depends on:** `01knacefg46mz1wgywpkn8h97d` (wasmtime-runtime — `bindgen!` must run first to produce the generated types)

**File:** `src-tauri/src/wasm/bindings.rs`

**Key mappings:**
- `wasm::CatalogEntry` ↔ `search::types::CatalogEntry`
- `wasm::PostAction` ↔ `search::types::PostAction`
- `wasm::ActionId` ↔ `search::types::ActionId`
- `wasm::EntryIcon` ↔ `search::types::EntryIcon`
- `wasm::Action` ↔ `search::types::Action` (keybinding is `None` — host-managed)

**Notes:** Native types without WIT equivalents yet (e.g., `PostAction::ShowCustomUI`, `ActionKeybinding`) are handled as WIT is extended for real plugins. Keep the conversion layer in `bindings.rs` rather than scattering `From` impls across the codebase.
