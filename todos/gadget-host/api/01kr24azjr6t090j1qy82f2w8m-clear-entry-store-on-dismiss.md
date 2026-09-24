---
kind: improvement
status: open
---

# Clear entry store on panel dismiss

## Context

The search entry store (introduced with the execute data
parameter feature, `ScoredEntry::data`) holds
the most recent search session's `ScoredEntry` values in a
host-side map so `execute()` can pass the full entry back to
the gadget.

The store is cleared atomically at the start of each
`search()` call. When the launcher panel is dismissed (click
outside, global shortcut toggle) without a new search, the
entries remain in memory until the next `search()` invocation.

This is bounded — at most one search session's worth of
entries (typically a few hundred small structs). The stale
entries cannot be executed because the UI is hidden.

## Proposed change

Add a `search_reset` Tauri command that the frontend calls on
panel hide. The command clears the entry store on the host
side. This eliminates the residual memory between panel
dismiss and next search.

Only worth implementing if profiling shows the retained
entries consume meaningful memory — e.g. gadgets attaching
large `data` payloads to entries.

## Related

- `ScoredEntry::data` in `src-tauri/src/commands/types.rs`: the
  feature that introduces the entry store.
