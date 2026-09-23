# execute() records frecency before validating the entry or action

**Kind:** improvement
**Severity:** low
**Area:** src-tauri/src/gadget_host.rs

## Problem
`GadgetHost::execute` records frecency as its very first step
(`src-tauri/src/gadget_host.rs:959-961`):

```rust
// Record frecency before execution — captures user intent
// regardless of whether the action succeeds.
self.frecency.record(source, entry_id);
```

The "even if the action fails" rationale is sound for real
executions, but the placement also records in two cases the
function itself treats as non-executions:

1. **Unknown entries.** When the entry-store lookup fails, the
   code logs "(bug — the UI should only execute entries from
   the current search)" and returns `Nothing`
   (`gadget_host.rs:979-985`) — but the frecency hit for the
   phantom `(source, entry_id)` pair has already been stored.
   Given the overlapping-search race in
   `todos/backend/search/01kwg1ph0qcdqtara5jcw7abym-concurrent-searches-corrupt-entry-store.md`,
   this branch is reachable in practice, so garbage ids
   accumulate in the frecency database.

2. **`OpenSettings`.** The host short-circuits this action
   before any gadget dispatch (`gadget_host.rs:969-977`) —
   opening a gadget's settings panel still counts as a "use" of
   that entry and permanently boosts its ranking, which is not
   what frecency is meant to capture.

## Impact
Frecency data slowly accumulates entries for ids that were never
executed and inflates scores for entries whose settings the user
merely inspected. Effect on ranking is small but permanent and
invisible.

## Suggested fix
Move `frecency.record` after the entry-store lookup succeeds and
after the `OpenSettings` short-circuit (keeping it *before* the
gadget's `execute()` call to preserve the record-on-failure
intent).
