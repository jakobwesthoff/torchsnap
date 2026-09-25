---
kind: improvement
status: open
---

# Clear entry store on panel dismiss

## Context

The search entry store (`src-tauri/src/entry_store.rs`) holds
the most recent search session's `ScoredEntry` values in a
host-side map so `execute()` can pass the full entry back to
the gadget. After ADR 55 it serves the same purpose for the
per-slot commands: the host takes the chosen slot's command
from the stored entry.

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
large command payloads to their actions.

## Related

- `todos/gadget-host/api/01kr2357mcz36g4gte0c0t1qz5-entry-action-commands-and-slots.md`:
  replaces `ScoredEntry::data` with per-slot commands.
