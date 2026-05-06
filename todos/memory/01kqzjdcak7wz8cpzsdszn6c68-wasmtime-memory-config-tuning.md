# Tune wasmtime `Config` for lower per-instance memory

## Problem

The other todos in this directory address gadget *binary* size,
which translates into compiled-code RAM. Independent of binary size,
each running gadget instance also consumes runtime RAM via
wasmtime's linear-memory machinery and Cranelift's compiled image
caching. Several wasmtime `Config` knobs control this directly and
are currently at their defaults in `WasmRuntime::new()`
(`src-tauri/src/.../engine.rs`).

A separate, already-filed todo
(`01kqz0frsmrsvmmnj6qcm9cbjp-pooling-allocator.md`) covers the
biggest host-side change — switching to
`PoolingAllocationConfig`. This todo covers the *non-pooling* knobs
that are useful regardless of allocation strategy and that should be
evaluated even if the pooling allocator is deferred.

## Goal

Profile current per-instance RSS for a representative set of
running gadgets, then tune `wasmtime::Config` to reduce the steady-
state and idle footprint without regressing instantiation speed
beyond a documented threshold.

## Knobs to evaluate

### `static_memory_maximum_size`

Default on 64-bit hosts: 4 GB of *virtual* address space reserved
per linear memory. This is virtual, not physical, but it does:

- Inflate `VSZ` reporting (cosmetic but confusing in `ps`/Activity
  Monitor).
- Add up across many instances when address space is bounded
  (notably on 32-bit hosts, but also under sandboxes with VM
  ceilings).
- Force wasmtime to map a large region per instance.

Consider:

```rust
config.static_memory_maximum_size(0); // disable static memory
// or
config.static_memory_maximum_size(64 * 1024 * 1024); // 64 MB cap
```

Setting to 0 switches to dynamic memory (smaller reservation,
slightly slower bounds-check codegen). For gadgets, dynamic memory
is almost certainly the right trade.

### `static_memory_guard_size` and `dynamic_memory_guard_size`

Guard pages catch wasm memory accesses that overflow the linear
memory's logical bound. Default sizes are tuned for server
throughput (large guards eliminate explicit bounds checks). For
many small gadgets, smaller guard regions reduce per-instance
address-space cost:

```rust
config.dynamic_memory_guard_size(64 * 1024); // 64 KB
config.static_memory_guard_size(64 * 1024);
```

Smaller guards force more explicit bounds checks in generated code,
slightly inflating compiled size. Measure both directions.

### `memory_init_cow`

Default: enabled. Confirm explicitly that it is on — it copy-on-
write maps the initial memory image from the precompiled artifact
on disk, so multiple instances of the same gadget share pages.
Particularly relevant given the existing on-disk compile cache
(`CachedComponent`, ADR 0044). Verify:

```rust
debug_assert!(config.memory_init_cow_enabled()); // pseudo — check actual API
```

If it's not on, turn it on. Combined with the on-disk compile cache,
this is what makes "instantiate" cheap on the second instance.

### `cranelift_opt_level`

Default: `Speed`. `SpeedAndSize` produces smaller compiled images
at a small runtime cost. Worth measuring for the calculator and
emoji-picker (the heaviest modules) — the compiled native image
size has the biggest effect on RSS for these.

```rust
config.cranelift_opt_level(wasmtime::OptLevel::SpeedAndSize);
```

### `parallel_compilation`

Default: enabled. No effect on RAM directly, but interacts with
startup latency vs. CPU budget. Note for completeness; do not
change without profile data.

### `epoch_interruption` / `consume_fuel`

Currently used (or not) for cooperative scheduling. Each adds a
small per-instance cost. If we don't actively rely on one of them,
disable explicitly so the default doesn't drift.

## Method

1. Add an instrumentation harness that boots N instances of each
   gadget and reports `RSS`/`VSZ` from `getrusage` /
   `mach_task_info` (macOS) and `proc/self/status` (Linux).
2. Capture baseline numbers with the current config.
3. Toggle each knob individually, then in combination, and record
   the deltas in a table.
4. Choose the combination that minimises RSS without regressing
   instantiation latency beyond ~10 % (precise threshold to be
   confirmed; document the chosen budget).
5. Land the config change with the measurement table in the commit
   body.

## Acceptance criteria

- Profiling harness committed (can live under
  `src-tauri/examples/` or behind a `#[cfg(test)]` bench).
- `Config` changes are documented inline with literate comments
  explaining *why* each non-default knob is set, not just *what*
  the value is.
- Measurement table in the commit body: baseline RSS, post-change
  RSS, instantiation-latency baseline and post.
- All existing host integration tests still pass.

## Trade-offs

- Smaller guards / dynamic memory cost a tiny bit of throughput on
  every memory access. Gadgets are not memory-bandwidth bound;
  acceptable.
- `cranelift_opt_level::SpeedAndSize` slows compile-time slightly
  and execution slightly. The on-disk cache amortises compile-time;
  the execution cost on gadgets is probably below the noise floor.
- This todo is largely orthogonal to the binary-size todos — both
  are worth doing. Order: do the binary-size work first (it
  multiplies through every running instance), then revisit this
  with smaller modules in hand to recompute the RAM floor.
- Cross-references:
  - `01kqz0frsmrsvmmnj6qcm9cbjp-pooling-allocator.md` — pooling
    allocator (largest single host-side win).
  - `01kqz0frsmrsvmmnj6qcm9cbjq-idle-instance-eviction.md` — idle
    eviction (orthogonal, addresses the "many idle gadgets"
    scenario rather than per-instance footprint).
  - ADR 0044 — file-backed WASM compile cache, which interacts
    with `memory_init_cow`.
