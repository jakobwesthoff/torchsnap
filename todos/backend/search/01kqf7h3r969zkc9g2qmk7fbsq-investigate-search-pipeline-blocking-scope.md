# Investigate: does a slow plugin block the entire result list, or just its own row?

## Context

The open-url WASM port (commits `4ba8e65`, `a82c9cf`) introduced
the first WASM plugin that uses `LookupMode::Blocking` on a
host import that can take 2+ seconds (network round-trip on
cold cache). The user reports that during the wait, **the
entire result list stays empty**, not just open-url's
contribution — and that subsequent searches also stay empty
even after clearing the query and typing something else.

A tooling-side exploration agent claimed the orchestrator
streams results incrementally (each plugin's results delivered
as it finishes) but also claimed "new catalog results for
fresh query: blocked behind the same plugin instance's mutex
chain." Those two claims are inconsistent — different plugins
have separate per-instance mutexes, so a slow open-url
*should not* block calculator/app-launcher/etc.

This investigation is needed before committing to an
architectural fix. The choice between approaches depends on
which symptom is actually happening:

- **If only open-url's row is empty** during the slow window
  while other plugins render normally for the current query →
  the bug is contained to that one plugin and its mutex.
  Fixing the per-instance queue (option A') is sufficient.
- **If the entire list is empty** during the slow window
  (no plugin's results render until open-url finishes) → the
  orchestrator has a "wait for all plugins" gate or shared
  rendering lock somewhere, and the architectural problem is
  bigger than per-plugin mutex queues.

## Concrete questions to answer

1. **Result streaming reality.** In `src-tauri/src/plugin_host.rs`
   around line 449 (`PluginHost::search`), trace the path
   from per-plugin `spawn_blocking` task to the IPC channel
   send. Is each plugin's `SearchResponse` forwarded to the
   frontend independently as it arrives, or is there an
   aggregation step that waits for completion of all (or
   some subset of) plugins?

2. **Catalog vs. query path.** Catalog plugins likely populate
   the result list via a different code path than query
   plugins. Is the catalog batch sent before, after, or
   interleaved with query-plugin results? Could a slow query
   plugin delay catalog rendering?

3. **Frontend reception.** In `src/launcher/hooks/useSearch.ts`
   (around line 95), how does the frontend incorporate
   incremental results? Are plugin results merged into the
   visible list one-at-a-time, or is there a buffer that
   waits for some completion signal?

4. **Empirical reproduction.** Reproduce the empty-list
   symptom in the running app with logs/instrumentation.
   Type `google.de` cold, observe what other plugins return.
   Tail backend logs for plugin completion timestamps. Does
   calculator (for `=2+2`) or app-launcher (for any partial
   app name) produce visible results during the open-url
   slow window? Or do they too stay invisible?

5. **Search-result-message ordering on the IPC channel.**
   Look for any `SearchMessage::*` variants that are sent
   only on completion of all plugins. The channel emit
   pattern matters.

## Why this matters for the architectural fix

The per-instance mutex queue fix (option A') works perfectly
when the only victim is the slow plugin's own row. If the
fix's premise — "other plugins can render concurrently
because they don't share the slow plugin's mutex" — is
wrong, then A' addresses the wasted-compute symptom but
not the user-visible "list stays empty" symptom. A different
or additional fix would be needed.

This investigation should complete BEFORE implementing A'
so we know whether A' is sufficient or merely a partial
patch.

## References

- `src-tauri/src/plugin_host.rs:449` — search orchestrator
- `src-tauri/src/plugin_host.rs:560` — per-plugin spawn_blocking
- `src-tauri/src/wasm/runtime/instance.rs:232` — WASM mutex
- `src-tauri/src/wasm/runtime/host/website_metadata.rs:72` — block_in_place
- `src/launcher/hooks/useSearch.ts:95-102` — frontend generation
- Conversation: open-url WASM port commits `4ba8e65`, `a82c9cf`
