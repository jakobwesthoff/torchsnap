---
kind: feature
status: open
---

# Pooling allocator for wasmtime Engine

## Problem

Each `WasmGadgetInstance` allocates its own linear memory, VMContext,
trampolines, and resource tables via the default on-demand allocator. With 7+
gadgets, the per-instance overhead and heap fragmentation adds up. The default
allocator makes no attempt to reuse memory across disable/re-enable cycles —
every `instantiate()` allocates fresh, and `disable()` frees everything back to
the OS (which may or may not actually reclaim it due to allocator fragmentation).

After the Winch + compile cache changes (ADR 0043, `CachedComponent`), the
`torchsnap` process idle RSS is ~52 MB — only ~7 MB above the pre-WASM
baseline. The remaining overhead is primarily per-instance Store allocations,
not compiled code. The pooling allocator would reduce this further but the
payoff is smaller from this baseline.

## Goal

Switch to wasmtime's `PoolingAllocationConfig` to pre-allocate a fixed pool of
instance slots with bounded linear memory. This reduces per-instance overhead,
eliminates fragmentation across enable/disable cycles, and puts an upper bound
on total guest memory consumption.

## Design

### Configuration

Add `PoolingAllocationConfig` to `WasmRuntime::new()` in `engine.rs`. Key
parameters to tune:

```rust
let mut pool = PoolingAllocationConfig::default();
pool.total_component_instances(32);
pool.max_component_instance_size(1 << 20);  // 1 MiB VMContext per instance
pool.total_memories(32);
pool.max_memory_size(16 << 20);             // 16 MiB linear memory cap
pool.memory_pages(256);                     // 256 × 64 KiB = 16 MiB
pool.total_tables(32);
pool.table_elements(10_000);
```

The exact numbers need empirical tuning against the current gadget set.

### Impact on existing code

- `WasmRuntime::instantiate()` — no changes needed. `Store::new()` and
  `Gadget::instantiate()` automatically use the pooling allocator when
  configured on the `Engine`.
- `WasmGadgetBridge::disable()` — dropping the `Store` returns the instance
  slot to the pool instead of freeing to the OS. Re-enabling reuses a slot.
- The pooling allocator uses `mmap` + `mprotect` to reserve virtual address
  space up front. On macOS this is cheap (virtual != physical).

### Failure mode

If all pool slots are exhausted, `instantiate()` returns an error. The pool
size (32) should be well above the current gadget count (7 built-in + WASM).

## Key files

- `src-tauri/src/wasm/runtime/engine.rs` — `WasmRuntime::new()`. The only file
  that needs structural changes.
- `src-tauri/Cargo.toml` — verify the `wasmtime` feature flags. The pooling
  allocator may need the `pooling-allocator` feature (check wasmtime 43 docs).

## Tests

- Existing integration tests should pass unchanged (pooling is transparent).
- Add a stress test: instantiate and drop N instances sequentially, verify RSS
  stays bounded.
- Add a test that exceeds the pool limit and verify the error is actionable.
- Measure RSS before/after with the benchmark script.

## Priority

Low — the compile cache already brought idle RSS to ~52 MB (~7 MB above
pre-WASM baseline). This optimization targets per-instance allocation
overhead, which is a smaller contribution at this point.
