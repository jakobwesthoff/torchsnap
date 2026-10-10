---
title: "System gadget root shadows Dev root during development"
kind: bug
component: gadget-host
status: needs-discussion
---
# System gadget root shadows Dev root during development

In a debug build, a bundled gadget can resolve from the stale staged archive under `target/bundled-gadgets/` (System root) instead of the live source under `gadgets/<id>/` (Dev root). Edits to the dev copy then appear to have no effect. The loader logs a collision warning, but the outcome is the opposite of what a developer iterating on a bundled gadget wants. The precedence for debug builds is undecided.

## Problem
Read in the code; not yet verified in a running `tauri dev` session. `enumerate_search_roots` in `src-tauri/src/wasm/discovery.rs` (around lines 51 to 90) returns roots in the order System, Dev, User. The caller resolves cross-root gadget-id collisions by letting the earlier root win. The module comment (around lines 29 to 32) and a test (around lines 315 to 330) state this.

In a debug build both System and Dev roots can be present at the same time. `tauri dev` exposes a resource dir, and Tauri copies the `resources` entries from `tauri.conf.json` next to the debug binary. Those entries include `target/bundled-gadgets/`, per the repository's CLAUDE.md build-pipeline notes. Once `just build` (or any run of `stage-bundled-gadgets`) has populated `target/bundled-gadgets/`, a later `tauri dev` session resolves e.g. `calculator` from the stale staged archive (System) instead of the live source under `gadgets/calculator/` (Dev), because System precedes Dev.

## Impact
Confusing dev loop for exactly the gadgets listed in `gadgets/bundled.toml`: stale bundled code wins over the working tree until the staged archive is deleted. No release impact, since a release has no Dev root.

## Suggested fix
Decide the intended precedence for debug builds. Options:
1. Order Dev before System in debug builds (one-line reorder under `cfg(debug_assertions)`).
2. Keep the order but skip the System root entirely in debug builds, since Dev covers the same gadgets from source.

Verify first (in `lib.rs` and the loader) that the earlier-root-wins behavior applies as documented. Check whether `tauri dev` really materializes the staged resources. The failure mode only exists if it does.

## Open questions
Within a single root, when two entries declare the same gadget id in their manifests, the winner depends on `read_dir` iteration order (`src-tauri/src/wasm/discovery.rs`, around line 119). That order is filesystem-dependent and nondeterministic. Check how the caller handles same-root id collisions.
