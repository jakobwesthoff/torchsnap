# Tune the gadget Cargo release profile for size

## Problem

The shared release profile in `gadgets/Cargo.toml` is currently
optimised for speed:

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
```

The comment in that file ("optimise for speed on the search hot
path") reflects an early assumption. In practice gadgets do not sit
on a tight CPU loop during search — they handle a few hundred input
events per session at most, often gated on user typing. The cost of
`opt-level = 3` is paid in code size and therefore in compiled native
image size inside wasmtime/Cranelift, which directly drives RSS.

The profile also does not strip debug info or the `name` section.
Even with `panic = "abort"`, Rust still emits the formatting machinery
behind `unwrap`, `expect`, indexing, and integer overflow checks — that
is addressed by a separate todo (`build-std` +
`panic_immediate_abort`); this todo focuses on stable, no-extra-flags
changes.

## Goal

Migrate the shared release profile to size-tuned settings while
keeping LTO and the abort-on-panic invariants. Expected savings:
20–40 % on top of the current `.wasm` sizes (orthogonal to and
combinable with `wasm-opt`).

## Proposed profile

```toml
[profile.release]
opt-level = "z"      # or "s" if measurements show "z" regresses too far
lto = "fat"
codegen-units = 1
panic = "abort"
strip = "debuginfo"  # or "symbols" for an extra few KB
```

### `opt-level = "z"` vs `"s"`

- `"z"` — most aggressive size pass; disables loop vectorisation,
  prefers smaller code shapes. Typical Rust/wasm saving vs `3`:
  20–40 %.
- `"s"` — middle ground; ~10–20 % saving, keeps more vectorisation.
- Decision: start with `"z"`. If a gadget shows user-visible latency
  (calculator regex evaluation, emoji-picker fuzzy match) we can drop
  *that* gadget back to `"s"` via a per-package profile override
  rather than regress the whole workspace.

### `strip = "debuginfo"` vs `"symbols"`

- `"debuginfo"` — drops DWARF; keeps the `name` section so wasmtime
  trap messages still mention function names.
- `"symbols"` — drops both. Combine with `wasm-tools strip` (separate
  todo) for the same effect.
- Decision: use `"debuginfo"`. The `wasm-opt`/`wasm-tools strip` todo
  handles the rest at post-process time and is reversible without a
  Cargo profile change.

## Implementation steps

1. Edit `gadgets/Cargo.toml` `[profile.release]` block to the proposed
   values.
2. Update the leading comment block to reflect the new rationale
   (size-first, with a pointer to the post-process todo).
3. Re-build every gadget; record before/after sizes in the commit
   message body.
4. Re-run the host integration tests against the new artifacts.
5. Sanity-check a representative gadget (calculator) by typing an
   expression and confirming the result is rendered without
   perceptible delay. Wasmtime + Cranelift compile time may also
   shift — note it in the commit if it's significant.

## Acceptance criteria

- Every gadget builds and passes `check-gadgets`.
- `.wasm` sizes shrink across the board (record exact numbers).
- No gadget shows user-visible interaction latency regressions.
- The profile comment block is updated to be accurate forever — no
  "previously" / "now" wording (per CLAUDE.md literate-programming
  guidance).

## Trade-offs

- Some gadgets *might* regress on peak throughput. The fat ones in
  the workspace today (calculator, emoji-picker, open-url) are
  bounded by I/O and string work, not arithmetic — extremely
  unlikely to be measurable.
- If a future gadget genuinely needs speed (e.g. a fuzzy-search
  ranking gadget), it can override the workspace profile in its own
  `Cargo.toml` via a `[profile.release.package.<name>]` section
  rather than regressing the workspace default.
- Combine with the post-process todo (`wasm-opt -Oz`) for compounding
  effect; do *not* skip post-processing on the assumption that the
  Cargo profile alone is sufficient — Binaryen catches passes Rust's
  LLVM does not.
