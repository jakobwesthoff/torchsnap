---
title: "Tune the gadget Cargo release profile for size"
kind: improvement
component: project
tags: [memory, performance]
---
# Tune the gadget Cargo release profile for size

Gadgets do not run hot loops during search, yet the shared release profile in `gadgets/Cargo.toml` still sets `opt-level = 3`, which optimises for speed. That grows the compiled native image that wasmtime keeps in RAM. Moving `opt-level` to a size-tuned value, keeping LTO, abort-on-panic and the debuginfo strip already in the profile, should cut the `.wasm` sizes. The earlier estimate was 20 to 40 percent for the whole profile change. The share that `opt-level` alone brings has not been measured.

## Problem

The shared `[profile.release]` in `gadgets/Cargo.toml` is:

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = "debuginfo"
```

The comment above it justifies `opt-level = 3` with "optimise for speed on the search hot path". In practice gadgets handle a few hundred input events per session at most, often gated on user typing. The cost of `opt-level = 3` is paid in code size and therefore in compiled native image size inside wasmtime/Cranelift, which directly drives RSS.

`strip = "debuginfo"` is already set (it keeps the `name` section, so wasmtime trap messages still mention function names). The panic formatting machinery behind `unwrap`, `expect`, indexing and overflow checks is a separate todo, nightly `build-std` with `panic_immediate_abort`. This todo covers the stable, no-extra-flags change that is left: `opt-level`.

## Goal

Switch the shared release profile to a size-tuned `opt-level`, orthogonal to and combinable with `wasm-opt`.

## Proposed change

```toml
[profile.release]
opt-level = "z"      # or "s" if measurements show "z" regresses too far
```

All other keys stay as they are.

### `opt-level = "z"` vs `"s"`

- `"z"`: most aggressive size pass; disables loop vectorisation, prefers smaller code shapes. Typical Rust/wasm saving vs `3`: 20 to 40 %.
- `"s"`: middle ground; about 10 to 20 % saving, keeps more vectorisation.
- Decision: start with `"z"`. If a gadget shows user-visible latency (calculator regex evaluation, emoji-picker fuzzy match), drop that gadget back to `"s"` or `3` via a per-package profile override rather than regress the whole workspace.

## Implementation steps

1. Change `opt-level` in the `[profile.release]` block of `gadgets/Cargo.toml`.
2. Update the leading comment block of the profile to give the size-first rationale, with a pointer to the wasm-opt todo.
3. Re-build every gadget and record before and after sizes in the commit message body.
4. Re-run the host integration tests against the new artifacts.
5. Sanity-check a representative gadget (calculator) by typing an expression and confirming the result is rendered without perceptible delay. Wasmtime + Cranelift compile time may also shift; note it in the commit if it is significant.

## Acceptance criteria

- Every gadget builds and passes `check-gadgets`.
- `.wasm` sizes shrink across the board (record exact numbers).
- No gadget shows user-visible interaction latency regressions.
- The profile comment block is accurate without "previously" / "now" wording (per CLAUDE.md literate-programming guidance).

## Trade-offs

- Some gadgets might regress on peak throughput. The fat ones in the workspace today (calculator, emoji-picker, open-url) are bounded by I/O and string work, not arithmetic, so a regression is extremely unlikely to be measurable.
- If a future gadget genuinely needs speed (for example a fuzzy-search ranking gadget), it can override the workspace profile in its own `Cargo.toml` via a `[profile.release.package.<name>]` section rather than regressing the workspace default.
- Combine with the wasm-opt pass (`wasm-opt -Oz`) for a compounding effect. Do not skip post-processing on the assumption that the Cargo profile alone is sufficient: Binaryen catches passes Rust's LLVM does not.

## Relations

- Relates to: [Integrate wasm-opt into the gadget build pipeline](01KPKBVDHEXAEJA64E3X7PM3BN-integrate-wasm-opt-into-the-gadget-build-pipeline.md), the post-process pass that runs on top of this profile.
