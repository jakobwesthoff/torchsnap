---
title: "Tune wasmtime `Config` for lower per-instance memory"
kind: improvement
component: gadget-host
tags: [memory, performance]
---
# Tune wasmtime `Config` for lower per-instance memory

Each running gadget instance uses RAM through wasmtime's linear-memory handling and its compiled-image caching, and several `Config` knobs control that. They are at their defaults in `WasmRuntime::new()`. Profiling them and tuning the ones that matter should lower steady-state and idle memory without slowing instantiation beyond a set budget.

## Problem

The other todos in this directory address gadget *binary* size,
which translates into compiled-code RAM. Independent of binary size,
each running gadget instance also consumes runtime RAM via
wasmtime's linear-memory machinery and Cranelift's compiled image
caching. Several wasmtime `Config` knobs control this directly and
are currently at their defaults in `WasmRuntime::new()`
(`src-tauri/src/.../engine.rs`).

The separate todo Switch the wasmtime engine to the pooling allocator
covers the biggest host-side change, `PoolingAllocationConfig`. This todo covers the *non-pooling* knobs
that are useful regardless of allocation strategy and that should be
evaluated even if the pooling allocator is deferred.

## Goal

Profile current per-instance RSS for a representative set of
running gadgets, then tune `wasmtime::Config` to reduce the steady-
state and idle footprint without regressing instantiation speed
beyond a documented threshold.

## Knobs to evaluate

### `memory_reservation`

Default on 64-bit hosts: 4 GiB of *virtual* address space reserved
per linear memory (10 MiB on 32-bit). This is virtual, not physical,
but it does:

- Inflate `VSZ` reporting (cosmetic but confusing in `ps`/Activity
  Monitor).
- Add up across many instances when address space is bounded
  (notably on 32-bit hosts, but also under sandboxes with VM
  ceilings).
- Force wasmtime to map a large region per instance.

Consider:

```rust
config.memory_reservation(0); // reserve nothing up front
// or
config.memory_reservation(64 * 1024 * 1024); // 64 MiB cap
```

Wasmtime's own documentation names minimising allocated virtual
memory as a reason to lower this, and notes that a memory whose
initial size exceeds the reservation is allocated at its minimum
size plus `memory_reservation_for_growth` instead. The cost of a
small reservation is that growth past it relocates the memory and
copies the existing contents.

### `memory_reservation_for_growth`

Sizes the extra virtual space reserved *after* a linear memory has
been relocated, i.e. the headroom a memory grows into once it has
outgrown `memory_reservation`. Relevant only in combination with a
lowered `memory_reservation`; tune the two together.

### `memory_guard_size`

Guard pages catch wasm memory accesses that overflow the linear
memory's logical bound, and a large enough guard lets the compiler
drop bounds checks entirely. Default: 32 MiB on 64-bit hosts
(64 KiB on 32-bit). Smaller guard regions reduce per-instance
address-space cost:

```rust
config.memory_guard_size(64 * 1024); // 64 KiB
```

Smaller guards force more explicit bounds checks in generated code,
slightly inflating compiled size. Measure both directions. Wasmtime
documents guard size as only one input to whether bounds checks can
be elided (`memory_reservation`, the memory's index type, its page
size and `signals_based_traps` all take part), and advises profiling
locally rather than changing the default blind.

### `memory_init_cow`

Default: enabled. Confirm explicitly that it is on. It copy-on-
write maps the initial memory image from the precompiled artifact
on disk, so multiple instances of the same gadget share pages.
Particularly relevant given the existing on-disk compile cache
(`CachedComponent`, ADR 0044). Verify:

```rust
debug_assert!(config.memory_init_cow_enabled()); // pseudo: check actual API
```

If it's not on, turn it on. Combined with the on-disk compile cache,
this is what makes "instantiate" cheap on the second instance.

### `cranelift_opt_level`

Default: `Speed`. `SpeedAndSize` produces smaller compiled images
at a small runtime cost, and compiled native image size is what
dominates RSS for the heaviest modules (calculator, emoji-picker).

```rust
config.cranelift_opt_level(wasmtime::OptLevel::SpeedAndSize);
```

This knob configures the Cranelift backend. `WasmRuntime::new()`
selects `Strategy::Winch` (ADR 0043), so it does not describe the
compiler the engine currently uses. ADR 0043 also records that under
Cranelift, dropping to `OptLevel::None` moved idle RSS only from
242.4 MB to 227.5 MB, while switching backend to Winch took it to
109.5 MB. Evaluate this knob only if the Winch decision is revisited.

### `parallel_compilation`

Default: enabled. No effect on RAM directly, but interacts with
startup latency vs. CPU budget. Note for completeness; do not
change without profile data.

### `epoch_interruption` / `consume_fuel`

Currently used (or not) for cooperative scheduling. Each adds a
small per-instance cost. If we don't actively rely on one of them,
disable explicitly so the default doesn't drift.

### Not a knob here: wasm GC

`src-tauri/Cargo.toml` builds wasmtime with `default-features =
false`, which drops wasmtime's `gc`, `gc-copying`, `gc-drc` and
`gc-null` features. No collector is compiled in and
`Config::collector` does not exist in this build, so wasmtime 46's
change of default collector does not apply. Gadget state lives in
linear memory: rustc has no wasm-gc target, and all gadgets are Rust
on `wasm32-wasip2`.

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

- Smaller guards and dynamic memory cost a tiny bit of throughput on
  every memory access. Gadgets are not memory-bandwidth bound;
  acceptable.
- `cranelift_opt_level::SpeedAndSize` slows compile-time slightly
  and execution slightly. The on-disk cache amortises compile-time;
  the execution cost on gadgets is probably below the noise floor.
- This todo is largely orthogonal to the binary-size todos, and both
  are worth doing. Order: do the binary-size work first (it
  multiplies through every running instance), then revisit this
  with smaller modules in hand to recompute the RAM floor.
- ADR 0044: file-backed WASM compile cache, which interacts
  with `memory_init_cow`.

## Relations

- Relates to: [Switch the wasmtime engine to the pooling allocator](01KQZ0FRSMRSVMMNJ6QCM9CBJP-switch-the-wasmtime-engine-to-the-pooling-allocator.md), the largest single host-side win.
- Relates to: [Evict idle gadget instances when the launcher is hidden](01KQZ0FRSMRSVMMNJ6QCM9CBJQ-evict-idle-gadget-instances-when-the-launcher-is-hidden.md), orthogonal, covering the many-idle-gadgets case instead of per-instance footprint.
- Relates to: [Post-process gadget WASM with `wasm-opt` and `wasm-tools strip`](01KQZJDCAK7WZ8CPZSDSZN6C63-post-process-gadget-wasm-with-wasm-opt-and-wasm-tools-strip.md), a binary-size todo to do first.
- Relates to: [Tune the gadget Cargo release profile for size](01KQZJDCAK7WZ8CPZSDSZN6C64-tune-the-gadget-cargo-release-profile-for-size.md), a binary-size todo to do first.
- Relates to: [Audit and slim heavy gadget dependencies](01KQZJDCAK7WZ8CPZSDSZN6C65-audit-and-slim-heavy-gadget-dependencies.md), a binary-size todo to do first.
- Relates to: [Replace `dlmalloc` with a smaller wasm allocator](01KQZJDCAK7WZ8CPZSDSZN6C66-replace-dlmalloc-with-a-smaller-wasm-allocator.md), a binary-size todo to do first.
- Relates to: [Nightly `build-std` + `panic_immediate_abort` for gadget builds](01KQZJDCAK7WZ8CPZSDSZN6C67-nightly-build-std-panicimmediateabort-for-gadget-builds.md), a binary-size todo to do first.
