---
kind: bug
severity: medium
status: open
area: [src-tauri/src/gadget_host.rs, src-tauri/src/commands/mod.rs]
tags: [unconfirmed, concurrency]
plan: todos/plans/01m3cjnk9q29z0s23xyysqcm19-entry-action-commands-implementation.md
---

# Overlapping search() calls interleave entry_store clear/insert without a generation guard

## Problem
The `search` Tauri command (`src-tauri/src/commands/mod.rs:27-32`)
awaits `GadgetHost::search` directly — there is no cancellation
of an in-flight search and no generation counter. Each keystroke
starts a new concurrent `search()` on the same host.

`GadgetHost::search` begins with `self.entry_store.clear()`
(`src-tauri/src/gadget_host.rs:661`) and then inserts results
incrementally as gadgets complete: prefix path at
`gadget_host.rs:715`, catalog batch at `:754`, and per-gadget
inserts inside the JoinSet drain loop at `:814`. With two
searches S1 (old query) and S2 (new query) overlapping, the
sequence

```
S1: clear, insert(catalog), ...
S2: clear                      ← wipes S1's inserts (fine)
S1: insert(gadget-a, old-query results)   ← stale re-insert
S2: insert(gadget-a, new-query results)   ← may land before S1's
```

is possible: `EntryStore::insert` calls are keyed by source, so
a *late* S1 insert can overwrite S2's fresher entries for the
same gadget, or add stale entries S2 never emitted to the
frontend. `execute()` then resolves `(source, entry_id)` against
this mixed store (`gadget_host.rs:979-985`):

- an entry the user sees can be missing → the "entry not found
  in entry store (bug ...)" error branch fires;
- an entry id that exists in both generations resolves to the
  *old* payload → the gadget executes against stale data.

The frontend discards results from superseded channels, so the
UI looks right — only the host-side store diverges, which is
what makes the resulting execute misbehavior hard to reproduce.

Related but distinct existing todos:
`todos/gadget-host/api/01krxs5qbzpr4vfq2dfywqc56r-concurrent-entries-calls.md`
(sequential catalog scan) and
`todos/gadget-host/api/01kr24azjr6t090j1qy82f2w8m-clear-entry-store-on-dismiss.md`
(clearing on dismiss).

`todos/gadget-host/api/01kr2357mcz36g4gte0c0t1qz5-entry-action-commands-and-slots.md`
(ADR 55) depends on this fix. After that redesign, a stale store
entry runs a stale command.

## Impact
Fast typing plus a slow gadget (exactly the third-party WASM
case) yields intermittent "entry not found" failures and
executes against stale entry data. No memory unsafety, but
user-visible actions can silently target the wrong entry
version.

## Decided fix
A search generation, decided by the maintainer on 2026-09-25.
`search()` starts a new generation at the top, and every insert
into the entry store carries the generation it was produced
for. The store drops inserts from an older generation.

Put the generation in `EntryStore` so the interleaving can be
tested there: `GadgetHost::search` has no test harness, because
it needs a Tauri store and an IPC channel.

Aborting the previous search was rejected. Gadget calls run in
`spawn_blocking`, and tokio cannot abort a blocking task once it
has started, so the gadget work would continue anyway.
