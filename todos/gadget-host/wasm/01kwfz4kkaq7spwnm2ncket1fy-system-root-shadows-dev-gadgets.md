# System gadget root shadows Dev root during development

**Kind:** question
**Severity:** medium
**Area:** src-tauri/src/wasm/discovery.rs

## Problem
`enumerate_search_roots` returns roots in the order System, Dev,
User (`src-tauri/src/wasm/discovery.rs:51-90`), and per the
module comment (`:29-32`) the caller resolves cross-root gadget-id
collisions by letting the earlier root win. The test at `:315-330`
pins "System must precede User" and its comment states the first
occurrence of a gadget id wins.

In a debug build both System and Dev roots can be present at the
same time: `tauri dev` exposes a resource dir, and Tauri copies
the `resources` entries from `tauri.conf.json` (which include
`target/bundled-gadgets/`, per the repo CLAUDE.md build-pipeline
notes) next to the debug binary. Once `just build` (or any run of
`stage-bundled-gadgets`) has populated `target/bundled-gadgets/`,
a subsequent `tauri dev` session resolves e.g. `calculator` from
the stale staged archive (System) instead of the live source
under `gadgets/calculator/` (Dev), because System precedes Dev.

The loader logs a collision warning (per `:31-32`), but the
outcome is inverted from what a developer iterating on a bundled
gadget wants: edits to the dev copy appear to have no effect.

## Impact
Confusing dev loop for exactly the gadgets listed in
`gadgets/bundled.toml`: stale bundled code wins over the working
tree until the staged archive is deleted. No release impact
(release has no Dev root).

## Suggested fix
Decide the intended precedence for debug builds. Options:
1. Order Dev before System in debug builds (one-line reorder
   under `cfg(debug_assertions)`).
2. Keep the order but skip the System root entirely in debug
   builds, since Dev covers the same gadgets from source.
Verify first (in `lib.rs` / the loader) that the earlier-root-wins
behavior actually applies as documented, and check whether
`tauri dev` really materializes the staged resources — the
failure mode only exists if it does.

Related open question: within a single root, when two entries
declare the same gadget id in their manifests, the winner depends
on `read_dir` iteration order (`:119`), which is
filesystem-dependent and nondeterministic. Check how the caller
handles same-root id collisions.
