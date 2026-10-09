---
kind: improvement
status: open
tags: [performance]
---

# Post-process gadget WASM with `wasm-opt` and `wasm-tools strip`

## Problem

The gadget build pipeline in `just/gadgets.just` runs
`cargo build --release` and copies the resulting `.wasm` directly into
the gadget directory and the `.torchsnap` archive. No post-processing
step shrinks the binary. The Cargo release profile already enables
`opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, and
`panic = "abort"` (`gadgets/Cargo.toml`), but Rust's own optimizer
leaves a lot on the table compared to Binaryen's `wasm-opt`, and the
binaries still ship with the `name`, `producers`, and (depending on
profile) DWARF custom sections.

Current raw `.wasm` sizes from
`gadgets/target/wasm32-wasip2/release/`:

| gadget        | size     |
|---------------|----------|
| calculator    | 1.63 MB  |
| emoji-picker  | 1.37 MB  |
| open-url      | 1.20 MB  |
| zerotier      |  461 KB  |
| hello-world   |  232 KB  |
| bangs         |  198 KB  |
| template      |  150 KB  |

`.wasm` size is the lever for runtime RAM: wasmtime/Cranelift compile
the full module on load and the compiled native image is roughly
2–4× the wasm size. Archive (`.torchsnap`) compression does not help
RAM.

## Goal

Add a deterministic post-processing step that runs
`wasm-opt -Oz` followed by `wasm-tools strip` on every gadget's `.wasm`
before it is copied to the gadget root and packaged into the archive.
Expected savings combined: 15–35 % on top of the current Cargo
release profile, plus a few KB from custom-section stripping.

## Design

### Pipeline integration

In `just/gadgets.just`, the `build-gadget` recipe currently does:

1. (optional) build frontend
2. `cargo build --release` inside `gadgets/`
3. copy `gadgets/target/wasm32-wasip2/release/<crate>.wasm` to
   `gadgets/<name>/<wasm-file-from-manifest>`
4. `just package-gadget <name>`

Insert the post-processing between steps 2 and 3 so the optimised
binary is what lands at the gadget root and inside the archive. Run
on the cargo output path, then copy.

```bash
# After cargo build, before cp:
wasm-opt -Oz \
    --enable-bulk-memory \
    --enable-multivalue \
    --enable-reference-types \
    --strip-debug \
    --strip-producers \
    "$src" -o "$src.opt"
wasm-tools strip "$src.opt" -o "$src"
rm "$src.opt"
```

### Feature flags for `wasm-opt`

`wasm-opt` rejects modules using wasm features it doesn't know about.
The component model itself is handled, but the underlying core
modules use bulk-memory, multivalue, and reference-types as a baseline
for `wasm32-wasip2`. List those explicitly so a future Binaryen
upgrade doesn't silently drop the flags.

If wasm-opt fails on a component (it operates on core modules), we
need `wasm-tools component embed` / `wasm-tools component new` in the
loop, OR use `wasm-opt --strip-target-features` and treat components
as core wasm during optimisation. Verify before committing — wasm-opt
recently gained component-model support but versions vary.

### Tool installation

Add a `doctor` check for `wasm-opt` and ensure it is documented in
the install path next to `wasm-tools`. On macOS:
`brew install binaryen`. Pin a version that handles components.

## Acceptance criteria

- `just build-gadgets` produces `.wasm` files that are demonstrably
  smaller than before (capture before/after table in the commit).
- Every existing test (`check-gadgets`, the host loader integration
  tests, runtime smoke tests) still passes against the optimised
  binaries.
- `just doctor` flags missing `wasm-opt`.
- Recipe fails loudly if `wasm-opt` returns non-zero — never silently
  ship an un-optimised binary as if optimisation succeeded.

## Trade-offs and notes

- `wasm-opt -Oz` is purely a size pass; `-O3` is a speed pass and
  produces slightly larger output. Gadgets are not on a hot loop
  during search — pick `-Oz`.
- `--strip-debug` is safe; we don't ship symbol-level debugging into
  end-user gadgets. Local debug builds use the `dev` profile and are
  unaffected.
- `wasm-tools strip` removes the `name` section. If we ever surface
  function names in host error logs (e.g. trap backtraces), we'd
  want to keep it — currently we don't, so strip.
- This todo is the prerequisite measurement baseline for the other
  size-related todos in this directory: do this first, then re-measure
  before tackling Cargo profile or dependency changes.
