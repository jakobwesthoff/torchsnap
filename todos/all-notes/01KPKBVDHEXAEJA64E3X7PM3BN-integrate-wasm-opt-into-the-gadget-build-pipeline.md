---
title: "Integrate wasm-opt into the gadget build pipeline"
kind: improvement
component: project
status: needs-discussion
tags: [tooling, memory, performance]
---
# Integrate wasm-opt into the gadget build pipeline

Cargo's own optimizer is the last stage that touches a gadget's `.wasm` before it is copied to the gadget root and zipped into the `.torchsnap`. Binaryen's `wasm-opt` typically recovers another 10 to 30 % of size on top of an LTO'd Cargo release, and in modern Binaryen releases it can see through the component wrapper, which Cargo cannot. This todo adds `wasm-opt` and `wasm-tools strip` as an opt-in post-process step in the gadget build pipeline. Binary size drives the RAM wasmtime needs per instance: it compiles the whole module on load and the native image is roughly 2 to 4 times the wasm size. Archive compression does not help there.

## Context

Gadget crates are built with the shared `[profile.release]` in `gadgets/Cargo.toml` (`opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = "debuginfo"`). The profile drops DWARF but keeps the `name` section and the `producers` and `target_features` custom sections. Nothing else post-processes the `.wasm` after Cargo.

`wasm-opt` runs a different set of passes than LLVM (for example `--merge-blocks`, `--dae` across the whole module, `--vacuum` and `--precompute`).

`wasm-opt` is **not** installed on the dev machine (`which wasm-opt` returns not found, checked 2026-10-21), and no install recipe adds it. `just/doctor.just` checks for `wasm-tools` only. Everything below is deferred until a maintainer explicitly opts in.

Raw sizes of the release builds in `gadgets/target/wasm32-wasip2/release/` on 2026-10-21 (most built 2026-09-30): calculator 1.73 MB, emoji-picker 1.40 MB, open-url 1.38 MB, zerotier 548 KB, hello-world 309 KB, bangs 254 KB, template 233 KB, awake 197 KB.

## Scope

Add `wasm-opt` as an **opt-in** post-process step in the gadget build pipeline, followed by `wasm-tools strip`:

1. Install recipe: add `wasm-opt` to `just/install.just`. Binaryen is available via `brew install binaryen` on macOS and as a standalone download on Linux. The recipe must be idempotent and skip the install when the binary is already on `$PATH`. Add a `just doctor` check for it next to `wasm-tools`.
2. Build recipe: in `just/gadgets.just` (recipe `build-gadget`), after the `cp "$src" "$dir/$wasm_file"` step, conditionally run `wasm-opt` over the copied artifact **in place**, then `wasm-tools strip`. When the binary is not on `$PATH`, the recipe must warn once and continue, so debug builds and fresh clones without Binaryen still produce a working gadget. When `wasm-opt` runs and returns non-zero, the recipe fails. It never ships an unoptimised binary as if optimisation had succeeded.
3. Apply the same pass to `build-test-fixtures`, so the committed test fixtures ship with the same shape as production gadgets.

## Flag selection

Binaryen 120 or newer understands the component model. The version check must fail the recipe if the installed `wasm-opt` is older. Starting command:

```bash
wasm-opt -O3 --enable-bulk-memory --enable-sign-ext \
         --enable-nontrapping-float-to-int \
         --enable-reference-types --enable-multimemory \
         "$dir/$wasm_file" -o "$dir/$wasm_file"
wasm-tools strip "$dir/$wasm_file" -o "$dir/$wasm_file"
```

Spell the baseline features out (bulk-memory, multivalue, reference-types are the baseline of `wasm32-wasip2` core modules), so a future Binaryen upgrade does not silently drop them.

If `wasm-opt` fails on a component, it operates on core modules. The fallbacks are `wasm-tools component embed` and `wasm-tools component new` in the loop, or `--strip-target-features` with the component treated as core wasm during optimisation. Verify before committing, since component-model support in `wasm-opt` is recent and versions vary.

## Open questions

- `-O3` or `-Oz`. Gadgets are not on a hot loop during search, so `-Oz` is the size-first candidate, and the todo "Tune the gadget Cargo release profile for size" proposes `opt-level = "z"` for the same reason. The profile still says `opt-level = 3`, so keep `-O3` unless measurement shows `-Oz` is a strictly better size and latency point for the launcher hot path (nucleo scoring, regex, evalexpr).
- Whether to also run `--strip-producers` and `--strip-target-features`. These custom sections are preserved by Cargo and survive the binding layer, together about 240 and 148 bytes per gadget today. Drop them only if there is no need to reproduce the exact toolchain from the artifact.
- Does `wasm-opt` preserve the `name` and `component-name` custom sections? `wasm-tools strip` removes the `name` section. Keep it if function names should show up in host error logs such as trap backtraces. Today they do not, so stripping is acceptable. If `wasm-opt` drops the section anyway, `strip = "debuginfo"` in the Cargo profile makes no difference for release bundles.

## Acceptance

- `just install-tools` installs `wasm-opt`, or documents the manual step if `brew` or apt is not available. `just doctor` flags a missing `wasm-opt`.
- `just build-gadget <id>` produces a `.wasm` smaller than today's baseline. Record before and after sizes for the four bundled gadgets: `hello-world`, `template`, `calculator` and `emoji-picker`, in the commit.
- Every existing test (`check-gadgets`, the host loader integration tests, runtime smoke tests) still passes against the optimised binaries.
- `just build-gadgets` and `just stage-bundled-gadgets` keep working on a machine without Binaryen, falling back to the Cargo-only output with a single informational warning.
- CI, if any, either installs Binaryen in its setup or tolerates the fallback path.

## Notes

- This todo is the measurement baseline for the other size todos: the Cargo release profile and the dependency audit. Do this first, then re-measure before tackling those.
- `--strip-debug` is safe. Local debug builds use the `dev` profile and are unaffected.

## Related

- `gadgets/Cargo.toml`: the shared release profile this pass layers on top of.
- `just/gadgets.just`: `build-gadget`, `package-gadget` and `build-test-fixtures` are the recipes to extend.
- `just/install.just`: where the install step lands.
