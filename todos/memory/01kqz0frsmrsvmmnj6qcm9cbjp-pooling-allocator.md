# Pooling allocator for wasmtime Engine

## Problem

Each `WasmGadgetInstance` allocates its own linear memory, VMContext,
trampolines, and resource tables via the default on-demand allocator. With 7+
gadgets, the per-instance overhead and heap fragmentation adds up. The default
allocator also makes no attempt to reuse memory across disable/re-enable cycles
— every `instantiate()` allocates fresh, and `disable()` frees everything back
to the OS (which may or may not actually reclaim it due to allocator
fragmentation).

## Goal

Switch to wasmtime's `PoolingAllocationConfig` to pre-allocate a fixed pool of
instance slots with bounded linear memory. This reduces per-instance overhead,
eliminates fragmentation across enable/disable cycles, and puts an upper bound
on total guest memory consumption.

## Design

### Configuration

Add `PoolingAllocationConfig` to `WasmRuntime::new()` in `engine.rs` (line
68–72). Key parameters to tune:

```rust
let mut pool = PoolingAllocationConfig::default();
pool.total_component_instances(32);   // headroom for future gadgets
pool.max_component_instance_size(1 << 20);  // 1 MiB VMContext per instance
pool.total_memories(32);
pool.max_memory_size(16 << 20);       // 16 MiB linear memory cap per gadget
pool.memory_pages(256);               // 256 × 64 KiB = 16 MiB
pool.total_tables(32);
pool.table_elements(10_000);

let mut config = Config::new();
config.wasm_component_model(true);
config.allocation_strategy(InstanceAllocationStrategy::Pooling(pool));
```

The exact numbers need empirical tuning against the current gadget set. Start
conservative (above values), then measure RSS and adjust.

### Impact on existing code

- `WasmRuntime::instantiate()` — no changes needed. The `Store::new()` and
  `Gadget::instantiate()` calls automatically use the pooling allocator when
  the `Engine` is configured with one.
- `WasmGadgetBridge::disable()` — dropping the `Store` returns the instance
  slot to the pool instead of freeing to the OS. Re-enabling reuses a slot.
- The pooling allocator uses `mmap` + `mprotect` to reserve virtual address
  space up front. On macOS this is cheap (virtual != physical) but the total
  reservation should be documented.

### Failure mode

If all pool slots are exhausted, `instantiate()` returns an error. The pool
size (32) should be well above the current gadget count (7 built-in + WASM).
Log the pool exhaustion clearly so it's diagnosable if a future gadget
explosion hits the limit.

## Key files

- `src-tauri/src/wasm/runtime/engine.rs` — `WasmRuntime::new()` lines 63–79.
  The only file that needs structural changes.
- `src-tauri/Cargo.toml` — verify the `wasmtime` feature flags. The pooling
  allocator may need the `pooling-allocator` feature (check wasmtime 43 docs;
  it may be included by default).

## Tests

- Existing integration tests should pass unchanged (pooling is transparent to
  callers).
- Add a stress test: instantiate and drop N instances sequentially, verify RSS
  stays bounded (no leak from pool fragmentation).
- Add a test that exceeds the pool limit and verify the error message is
  actionable.
- Measure RSS before/after with the benchmark script to quantify the
  improvement.

## Sequencing

This can be done independently of the pre-compiled cache todo. However, the
two interact well: mmap'd compiled components (cache todo) + pooled instance
memory (this todo) together mean almost all WASM-related memory is either
file-backed or pool-managed, with minimal anonymous heap pressure.
