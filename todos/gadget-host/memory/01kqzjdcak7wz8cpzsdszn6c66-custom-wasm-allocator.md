---
kind: improvement
status: open
tags: [performance]
---

# Replace `dlmalloc` with a smaller wasm allocator

## Problem

Rust's default allocator on `wasm32-wasip2` is `dlmalloc`, embedded
into every gadget binary via the `std` runtime. It contributes:

- ~10 KB of code per gadget (multiplied across 7+ gadgets).
- A non-trivial initial linear-memory footprint for its bookkeeping
  structures (free lists, bin tables) that wasmtime has to map for
  every instance.

For our gadgets — small, short-lived allocation patterns (parsing a
query, returning a result list) — the heavy general-purpose
allocator is overkill. Smaller allocators give up some throughput on
adversarial allocation patterns we don't have, in exchange for less
code and a smaller initial heap.

## Goal

Replace `dlmalloc` with a lighter allocator in the
`torchsnap-gadget-sdk` so every gadget benefits without per-gadget
plumbing. Expected effect:

- A few KB shaved per `.wasm` (small per file, real in aggregate).
- Lower per-instance linear-memory high-water mark in wasmtime —
  this is the main motivation, since RAM scales with concurrent
  instances.

## Candidate allocators

### `lol_alloc`

- Tiny, simple bump-and-free-list allocator designed for wasm.
- Multiple flavours: `FreeListAllocator`, `LeakingPageAllocator`
  (truly minimal — never frees, suitable for never-restarting
  short-lived flows), `AssumeSingleThreadedAllocator` wrapper.
- Maintained, small surface area.
- Good default choice.

### `talc`

- Modern segregated-fit allocator; faster than `lol_alloc` under
  realistic loads while still small.
- Larger code size than `lol_alloc` but still well under
  `dlmalloc`.
- Pick this if measurements show `lol_alloc` regresses on a
  hot-path gadget (unlikely).

### `wee_alloc`

- Historically the standard "tiny wasm allocator". **Unmaintained**
  and has known fragmentation issues. Do not use.

## Design

The SDK is the right place to install the allocator because every
gadget already depends on it via
`use torchsnap_gadget_sdk::prelude::*;` and registers via
`define_gadget!(...)`. Putting the `#[global_allocator]` static in
the SDK means:

- Single source of truth — switching allocator later is a one-line
  change.
- Gadget authors don't have to remember to set it.
- We can gate it behind a feature flag (`small-alloc`, default on)
  so a gadget that genuinely needs `dlmalloc` performance can opt
  out.

```rust
// in torchsnap-gadget-sdk/src/lib.rs (or a dedicated module)
#[cfg(target_family = "wasm")]
#[global_allocator]
static ALLOC: lol_alloc::AssumeSingleThreadedAllocator<
    lol_alloc::FreeListAllocator,
> = unsafe {
    lol_alloc::AssumeSingleThreadedAllocator::new(
        lol_alloc::FreeListAllocator::new(),
    )
};
```

(Confirm exact API surface against current `lol_alloc` release.)

## Verification

1. Pick a representative gadget (calculator — heaviest allocation
   workload).
2. Build before/after, record `.wasm` size and use
   `wasm-tools` to inspect the initial-memory declaration.
3. Run a stress smoke test: 1000 calculator queries in a tight
   loop, watch `torchsnap` RSS via `ps`.
4. Run the existing host integration tests against the new SDK.

## Acceptance criteria

- The SDK installs a global allocator under `cfg(target_family = "wasm")`
  only — host crates that depend on the SDK for shared types are
  unaffected.
- All gadgets continue to pass tests.
- `.wasm` size delta and RSS delta captured in the commit body.
- The choice (`lol_alloc` vs `talc`) and the rationale is documented
  in a literate comment next to the `#[global_allocator]` static.

## Trade-offs

- A simpler allocator can fragment under long-lived allocation
  patterns. Gadgets are short-lived per invocation, so this is
  unlikely to bite.
- If a gadget ever needs threading inside the wasm module (currently
  none do; `wasm32-wasip2` is single-threaded by default), the
  `AssumeSingleThreadedAllocator` wrapper would need to go. Note
  this in the comment so a future contributor doesn't trip on it.
- Pairs naturally with the host-side wasmtime memory tuning todo:
  reducing per-instance memory pressure on the guest side
  complements reducing the wasmtime allocator reservation on the
  host side.
