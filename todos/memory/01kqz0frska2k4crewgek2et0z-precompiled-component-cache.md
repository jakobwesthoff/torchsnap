# Pre-compiled component cache with hash validation

## Problem

`Component::new(&engine, wasm_bytes)` runs full Cranelift codegen on every app
launch for every gadget. This is the dominant memory cost (~50–150 MB for 7
gadgets) because compiled native code is 10–30x larger than the input WASM, and
it lives in anonymous heap memory that the OS cannot page out.

## Goal

Persist compiled components to disk so subsequent launches skip Cranelift
entirely. Ensure every loaded `Component` — including first-ever compilations —
is backed by an mmap'd file, never anonymous heap memory.

## Design

### Cache location

`<app_data_dir>/gadget-home/<gadget-id>/compile-cache/` alongside the existing
`sql/` sibling. One file per compiled component:
`<sha256-of-wasm-bytes>.cwasm` (wasmtime's conventional extension for
serialized components).

### Hash validation

Use SHA-256 of the raw WASM bytes (read via `source.read_wasm()`) as the cache
key. On load:

1. Read WASM bytes from the gadget source.
2. Compute `sha256(wasm_bytes)`.
3. Check for `<gadget-home>/<id>/compile-cache/<hash>.cwasm`.
4. **Cache hit**: `unsafe { Component::deserialize(&engine, &fs::read(path)?) }`
   — this is `unsafe` because wasmtime trusts the serialized bytes are
   well-formed; the hash match + our own write are the safety guarantee.
5. **Cache miss**: `Component::new(&engine, &wasm_bytes)` → `component.serialize()`
   → write to `<hash>.cwasm` → **drop** the heap-resident `Component` →
   `Component::deserialize()` from the file just written. This ensures even
   first-compile components are file-backed/mmap'd.
6. Clean up any other `.cwasm` files in the directory (stale hashes from
   previous WASM versions).

### Invalidation

The cache self-invalidates: a WASM update changes the hash, so step 3 misses
and step 5 recompiles. Step 6 garbage-collects the old file.

Additionally, wasmtime rejects deserialization if the `Engine` configuration
changed between serialize and deserialize (it embeds the config in the
serialized blob), so an engine version bump or config change is also caught
automatically — deserialize returns an error, we fall through to recompile.

### API surface

Extend `WasmRuntime` with a new method (or modify `compile`) that takes the
WASM bytes + a cache directory path. The public signature stays simple; the
cache logic is internal.

`WasmGadgetBridge::new()` already has access to `app_data_dir` and the
`gadget_id`, so it can construct the cache path and pass it in.

## Key files

- `src-tauri/src/wasm/runtime/engine.rs` — `WasmRuntime::compile()` (lines
  104–117) is the entry point to modify. The `components` HashMap should store
  the deserialized `Component` (which is still an `Arc` internally, still
  cloneable, same type).
- `src-tauri/src/wasm/bridge.rs` — `WasmGadgetBridge::new()` (line 201) calls
  `runtime.compile()` and needs to supply the cache directory.
- `src-tauri/src/wasm/source.rs` — `GadgetSource::read_wasm()` (line 107)
  provides the raw bytes to hash.

## Safety note

`Component::deserialize()` is `unsafe` because it trusts the byte stream.
Document the safety argument: we wrote the file ourselves, the hash validates
the input WASM hasn't changed, and wasmtime validates Engine compatibility
internally. A corrupted file on disk would cause a deserialization error (not
UB) — wasmtime checks internal checksums.

## Tests

- Unit test: compile → serialize → deserialize round-trip produces a functional
  component.
- Unit test: modified WASM bytes cause a cache miss and recompile.
- Unit test: corrupted `.cwasm` file falls back to recompile (not panic).
- Unit test: stale `.cwasm` files from previous hashes are cleaned up.
- Integration test: gadget loads and responds to search after cache-hit path.
