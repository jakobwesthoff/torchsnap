# Investigate: does a slow gadget block the entire result list, or just its own row?

## Context

The open-url WASM port (commits `4ba8e65`, `a82c9cf`) introduced
the first WASM gadget that uses `LookupMode::Blocking` on a
host import that can take 2+ seconds (network round-trip on
cold cache). The user reports that during the wait, **the
entire result list stays empty**, not just open-url's
contribution — and that subsequent searches also stay empty
even after clearing the query and typing something else.

A tooling-side exploration agent claimed the orchestrator
streams results incrementally (each gadget's results delivered
as it finishes) but also claimed "new catalog results for
fresh query: blocked behind the same gadget instance's mutex
chain." Those two claims are inconsistent — different gadgets
have separate per-instance mutexes, so a slow open-url
*should not* block calculator/app-launcher/etc.

This investigation is needed before committing to an
architectural fix. The choice between approaches depends on
which symptom is actually happening:

- **If only open-url's row is empty** during the slow window
  while other gadgets render normally for the current query →
  the bug is contained to that one gadget and its mutex.
  Fixing the per-instance queue (option A') is sufficient.
- **If the entire list is empty** during the slow window
  (no gadget's results render until open-url finishes) → the
  orchestrator has a "wait for all gadgets" gate or shared
  rendering lock somewhere, and the architectural problem is
  bigger than per-gadget mutex queues.

## Concrete questions to answer

1. **Result streaming reality.** In `src-tauri/src/gadget_host.rs`
   around line 449 (`GadgetHost::search`), trace the path
   from per-gadget `spawn_blocking` task to the IPC channel
   send. Is each gadget's `SearchResponse` forwarded to the
   frontend independently as it arrives, or is there an
   aggregation step that waits for completion of all (or
   some subset of) gadgets?

2. **Catalog vs. query path.** Catalog gadgets likely populate
   the result list via a different code path than query
   gadgets. Is the catalog batch sent before, after, or
   interleaved with query-gadget results? Could a slow query
   gadget delay catalog rendering?

3. **Frontend reception.** In `src/launcher/hooks/useSearch.ts`
   (around line 95), how does the frontend incorporate
   incremental results? Are gadget results merged into the
   visible list one-at-a-time, or is there a buffer that
   waits for some completion signal?

4. **Empirical reproduction.** Reproduce the empty-list
   symptom in the running app with logs/instrumentation.
   Type `google.de` cold, observe what other gadgets return.
   Tail backend logs for gadget completion timestamps. Does
   calculator (for `=2+2`) or app-launcher (for any partial
   app name) produce visible results during the open-url
   slow window? Or do they too stay invisible?

5. **Search-result-message ordering on the IPC channel.**
   Look for any `SearchMessage::*` variants that are sent
   only on completion of all gadgets. The channel emit
   pattern matters.

## Why this matters for the architectural fix

The per-instance mutex queue fix (option A') works perfectly
when the only victim is the slow gadget's own row. If the
fix's premise — "other gadgets can render concurrently
because they don't share the slow gadget's mutex" — is
wrong, then A' addresses the wasted-compute symptom but
not the user-visible "list stays empty" symptom. A different
or additional fix would be needed.

This investigation should complete BEFORE implementing A'
so we know whether A' is sufficient or merely a partial
patch.

## References

- `src-tauri/src/gadget_host.rs:449` — search orchestrator
- `src-tauri/src/gadget_host.rs:560` — per-gadget spawn_blocking
- `src-tauri/src/wasm/runtime/instance.rs:232` — WASM mutex
- `src-tauri/src/wasm/runtime/host/website_metadata.rs:72` — block_in_place
- `src/launcher/hooks/useSearch.ts:95-102` — frontend generation
- Conversation: open-url WASM port commits `4ba8e65`, `a82c9cf`
