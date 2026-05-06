# 44. File-backed WASM compile cache

Date: 2026-05-06

## Status

Accepted

## Context

After switching to Winch (ADR 0043), the torchsnap process idle RSS was ~110 MB
with 7 gadgets loaded. The compiled native code produced by Winch lived in
anonymous heap memory that the OS could not page out.

wasmtime's `Component::serialize()` produces a byte blob that can be written to
disk and later loaded via `Component::deserialize_file()`. The deserialized
component is backed by an mmap'd file rather than anonymous heap memory. This
means the OS manages the compiled code pages: they are paged in on demand when
the gadget is used, and can be reclaimed under memory pressure without any
explicit action from the application.

## Decision

Introduce `CachedComponent` (`src-tauri/src/wasm/runtime/cached_component.rs`)
as the per-gadget compiled component owner. Each `WasmGadgetBridge` holds a
`CachedComponent` instead of interacting with `WasmRuntime` for component
storage.

On first use (`acquire()`), the component is compiled from the gadget source's
WASM bytes, serialized to disk, the heap-allocated compiled image is explicitly
dropped, and the component is re-loaded via `deserialize_file` so it is
file-backed. On subsequent launches, the cached `.cwasm` file is deserialized
directly, skipping compilation entirely.

`release()` drops the in-memory component while keeping the cache file. The
next `acquire()` re-deserializes from disk without re-reading WASM source or
re-hashing.

### Cache layout

`<app_data_dir>/gadget-home/<gadget-id>/compile-cache/<blake3>.<engine_hash>.cwasm`

The filename encodes the BLAKE3 hash of the raw WASM bytes and the engine
compatibility hash (from `Engine::precompile_compatibility_hash()`). This
ensures automatic invalidation when either the WASM content or the engine
configuration changes. Stale `.cwasm` files are pruned during `acquire()`.

### WasmRuntime refactoring

`WasmRuntime` was refactored from a component registry (compile + store in
HashMap + instantiate by id) into a stateless compile/instantiate service:

- `compile(wasm_bytes) -> Component` — returns the component, no storage.
- `instantiate(gadget_id, &component) -> WasmGadgetInstance` — takes a
  component reference, builds Linker + Store + WasiCtx.
- `deserialize_component(path) -> Component` — wraps the unsafe
  `Component::deserialize_file`.
- `serialize_component(&component) -> Vec<u8>` — wraps `Component::serialize`.

The `components: HashMap<String, Component>` and `metadata_service` fields were
removed. The metadata service moved to `WasmGadgetBridge` as a direct field,
matching the pattern of all other capability services.

### Benchmark data (2026-05-06)

All measurements at the "app started (idle)" phase with 7 WASM gadgets loaded.

| Configuration              | `torchsnap` idle RSS | Total (all processes) |
|----------------------------|---------------------:|----------------------:|
| Pre-WASM baseline          |              45.4 MB |              92.1 MB  |
| Cranelift (default)        |             242.4 MB |             293.3 MB  |
| Winch (ADR 0043)           |             109.5 MB |             160.8 MB  |
| **Winch + compile cache**  |          **52.3 MB** |          **96.5 MB**  |

Session stability (delta from first to last phase snapshot):

| Configuration             | `torchsnap` Δ |
|---------------------------|---------------:|
| Cranelift (default)       |      -100.6 MB |
| Winch                     |        -1.5 MB |
| Winch + compile cache     |        -1.3 MB |

## Consequences

Idle RSS dropped from ~110 MB (Winch only) to ~52 MB (Winch + compile cache),
bringing the total application footprint to ~96.5 MB — within ~4 MB of the
pre-WASM baseline of 92.1 MB.

Subsequent app launches skip Winch compilation entirely for unchanged gadgets,
improving startup time.

The cache files are per-gadget under `gadget-home/<id>/compile-cache/`.
Uninstalling a gadget removes the entire `gadget-home/<id>/` subtree including
the cache. Engine configuration changes (e.g. wasmtime version bumps) are
caught by the engine compatibility hash in the filename — stale entries are
pruned automatically.
