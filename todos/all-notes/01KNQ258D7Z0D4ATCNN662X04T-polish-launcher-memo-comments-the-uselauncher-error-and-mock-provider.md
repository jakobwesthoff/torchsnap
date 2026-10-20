---
title: "Polish launcher memo comments, the useLauncher error and mock provider docs"
kind: improvement
component: frontend
impact: low
origin: review
tags: [error-handling]
---
# Polish launcher memo comments, the useLauncher error and mock provider docs

Three polish items surfaced by the post-migration validation review of 2026-04-08 (seven parallel review agents covering D1-D6 and the calculator port). The critical and important findings of that review were fixed in 11 follow-up commits (clipboard host import, SQL handle drop, scheduler abort race, lazy SDK shim resolution, type sync assertion, etc.). The items below are lower-priority polish that was deliberately deferred. None of them blocks current functionality. They land together as one small frontend PR. Each item keeps the original review section number so back-references stay intact.

## #22: Explanatory comments on intentionally omitted memo deps

**Where:** `src/launcher/Launcher.tsx`, around the `gadgetLauncher` and `inlineLauncher` `useMemo` blocks (search for `useMemo<LauncherActions>`: `gadgetLauncher` and `inlineLauncher`).

**The thing:** the deps arrays for these memos intentionally omit `setGadgetFooter` / `setInlineFooter` (stable React `setState` setters) and `mouseActiveRef` (a stable ref). The omissions are correct but undocumented. A future maintainer running an exhaustive-deps lint check will be tempted to "fix" them and trigger needless re-renders.

**Fix:** add a one-line comment above each memo deps array explaining why those values are stable and absent on purpose.

## #23: Generalize the `useLauncher()` error message

**Where:** `src/contexts/useLauncher.ts`. The SDK shim `useLauncher` in `packages/gadget-sdk/src/shims/hooks.ts` only delegates to the host hook and carries no message of its own.

**Current error string:**
> `useLauncher called outside the launcher tree (settings panels do not have launcher actions)`

**The thing:** the parenthetical is too specific. Today the only non-launcher mount is the settings panels, but the GadgetContext design supports future mount kinds (modal slot, command palette, menu bar and so on). When those land, the error will be misleading.

**Fix:** drop the settings-panel name. Suggested: `useLauncher called from a context that was not mounted inside the launcher tree: this hook is only available to launcher view and inline-view components`.

## #26: `MockGadgetContextProvider` JSDoc accuracy

**Where:** `packages/gadget-sdk/src/testing/MockGadgetContextProvider.tsx`.

**The thing:** the `launcher` prop's JSDoc says "calling `useLauncher()` from a child throws exactly as it would in a settings panel". That is accurate for the current code, but it conflates two facts: (a) the `launcher` slice is absent, and (b) `useLauncher()` throws when the slice is absent. If `useLauncher()` ever returns `undefined` instead of throwing, the doc claim silently becomes wrong.

**Fix:** describe the slice presence directly, not the consequent throw. For example: "When `launcher` is omitted, the rendered context value carries no `launcher` slice. Calling `useLauncher()` from a child fails according to what the host's hook does in that case (currently it throws)."

A trivial doc-only change.

## Other review items tracked elsewhere

These came up in the same review but already have dedicated todos or were explicitly deferred by the migration plan:

- **`spawn_blocking` for the scheduler's `run_task` call**: covered by a separate todo (see Relations). The scheduler's inline comment now documents the threshold (`scheduler_loop` in `src-tauri/src/wasm/bridge.rs`).
- **`messaging-stream` WIT sub-interface** for WASM gadget streaming RPC. Out of scope per ADR 0030. It would also need the gadget channel lifecycle todo (see Relations) to land first.

## Relations

- Relates to: [Use spawn_blocking instead of std::thread::spawn for Tokio-dependent threads](01KN89G33581ZA71V2JBBHX4AH-use-spawnblocking-instead-of-stdthreadspawn-for-tokio-dependent-threads.md), which covers the scheduler's `run_task` call.
- Relates to: [Add explicit close signaling for gadget channels](01KN30TH27X0ARV9A5CEKKWGPW-add-explicit-close-signaling-for-gadget-channels.md), which the messaging-stream item needs first.
