---
kind: improvement
severity: low
status: open
area: [src-tauri/src/gadget_host.rs]
---

# execute() records frecency before validating the entry or action

## Problem
`GadgetHost::execute` records frecency as its very first step
(`src-tauri/src/gadget_host.rs:1029-1031`):

```rust
// Record frecency before execution — captures user intent
// regardless of whether the action succeeds.
self.frecency.record(source, entry_id);
```

The "even if the action fails" rationale is sound for real
executions, but the placement also records in cases the function
itself treats as non-executions:

1. **Unknown entries or empty slots.** When `resolve_command`
   returns `Unresolved::UnknownEntry` or `Unresolved::EmptySlot`,
   `execute` logs a bug comment and returns `Nothing`
   (`gadget_host.rs:1033-1049`), but the frecency hit for the
   phantom `(source, entry_id)` pair has already been stored.
   Overlapping searches used to make `UnknownEntry` reachable in
   practice; `EntryStore` now drops results of an older search.
   Whenever either branch is still reached, a garbage id lands in
   the frecency database.

2. **`OpenSettings`.** `OpenSettings` is now a fixed `Slot`
   resolved and dispatched to the gadget like any other slot (ADR
   55/56); the gadget returns `PostAction::OpenSettings`, which
   `execute` turns into `HostEffect::OpenGadgetSettings`
   (`gadget_host.rs:1051-1061`). There is no host-side
   short-circuit anymore, but `frecency.record` still runs before
   this dispatch, so opening a gadget's settings panel still counts
   as a "use" of that entry and permanently boosts its ranking,
   which is not what frecency is meant to capture.

## Impact
Frecency data slowly accumulates entries for ids that were never
executed and inflates scores for entries whose settings the user
merely inspected. Effect on ranking is small but permanent and
invisible.

## Suggested fix
Move `frecency.record` after `resolve_command` succeeds and after
checking whether the resolved slot is `OpenSettings` (keeping it
*before* the gadget's `execute()` call to preserve the
record-on-failure intent).
