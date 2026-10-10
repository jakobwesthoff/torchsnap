---
title: "Integrate wasm-opt into the gadget build pipeline"
kind: improvement
component: project
status: needs-discussion
tags: [tooling]
---
# Integrate wasm-opt into the gadget build pipeline

Cargo's own optimizer is the last stage that touches a gadget's `.wasm` before it is copied to the gadget root and zipped into the `.torchsnap`. Binaryen's `wasm-opt` typically recovers another 10 to 30 % of size on top of an LTO'd Cargo release, and in modern Binaryen releases it can see through the component wrapper, which Cargo cannot. This todo adds `wasm-opt` as an opt-in post-process step in the gadget build pipeline.

## Context

Gadget crates are built with a tuned `[profile.release]` (`lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `opt-level = 3`; see each `gadgets/*/Cargo.toml`). Nothing else post-processes the `.wasm` after Cargo.

`wasm-opt` runs a different set of passes than LLVM (for example `--merge-blocks`, `--dae` across the whole module, `--vacuum` and `--precompute`).

`wasm-opt` is **not** currently installed on the dev machine (`which wasm-opt` returns not found), and no install recipe adds it. Everything below is deferred until a maintainer explicitly opts in.

## Scope

Add `wasm-opt` as an **opt-in** post-process step in the gadget build pipeline:

1. Install recipe: add `wasm-opt` to `just/install.just`. Binaryen is available via `brew install binaryen` on macOS and as a standalone download on Linux. The recipe must be idempotent and skip the install when the binary is already on `$PATH`.
2. Build recipe: in `just/gadgets.just` (recipe `build-gadget`), after the `cp "$src" "$dir/$wasm_file"` line, conditionally run `wasm-opt` over the copied artifact **in place**. When the binary is not on `$PATH`, the recipe must warn once and continue, so debug builds and fresh clones without Binaryen still produce a working gadget.
3. Apply the same pass to `build-test-fixtures`, so the committed test fixtures ship with the same shape as production gadgets.

## Flag selection

Binaryen 120 or newer understands the component model. The version check must fail the recipe if the installed `wasm-opt` is older. Starting command:

```bash
wasm-opt -O3 --enable-bulk-memory --enable-sign-ext \
         --enable-nontrapping-float-to-int \
         --enable-reference-types --enable-multimemory \
         "$dir/$wasm_file" -o "$dir/$wasm_file"
```

## Open questions

- `-O3` or `-Oz`: the gadget-side profile already chose speed (`opt-level = 3`), so keep `-O3` unless measurement shows `-Oz` is a strictly better size and latency point for the launcher hot path (nucleo scoring, regex, evalexpr).
- Whether to also run `--strip-producers` and `--strip-target-features`. These custom sections are preserved by Cargo and survive the binding layer, together about 240 and 148 bytes per gadget today. Drop them only if there is no need to reproduce the exact toolchain from the artifact.
- Does `wasm-opt` preserve the `name` and `component-name` custom sections? If not, the choice of `strip` in the Cargo profile becomes moot for release bundles.

## Acceptance

- `just install-tools` installs `wasm-opt`, or documents the manual step if `brew` or apt is not available.
- `just build-gadget <id>` produces a `.wasm` smaller than today's baseline. Record before and after sizes for the four bundled gadgets: `hello-world`, `template`, `calculator` and `emoji-picker`.
- `just build-gadgets` and `just stage-bundled-gadgets` keep working on a machine without Binaryen, falling back to the Cargo-only output with a single informational warning.
- CI, if any, either installs Binaryen in its setup or tolerates the fallback path.

## Related

- `gadgets/*/Cargo.toml`: the release profile settings this pass layers on top of.
- `just/gadgets.just`: `build-gadget`, `package-gadget` and `build-test-fixtures` are the recipes to extend.
- `just/install.just`: where the install step lands.
