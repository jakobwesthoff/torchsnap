---
kind: bug
severity: medium
status: open
area: [src-tauri/src/gadget_host.rs, src-tauri/src/commands/mod.rs]
tags: [unconfirmed]
---

# A panicking gadget aborts the whole search/execute pipeline via expect()

## Problem
Every place the host runs gadget code on the blocking pool
unwraps the `JoinHandle` result with a panic-on-panic `expect`:

- prefix search: `.expect("prefix search task not panicked")`
  (`src-tauri/src/gadget_host.rs:751`)
- catalog search: `.expect("catalog search task not panicked")`
  (`gadget_host.rs:815`)
- query fan-out: `result.expect("query search task not
  panicked")` (`gadget_host.rs:850`)
- execute: `.expect("search_execute task must not panic")`
  (`src-tauri/src/commands/mod.rs:62`)

A panic inside any single gadget's `search()` / `entries()` /
`execute()` — native gadget bug, or a WASM-bridge panic such as
the poisoned-mutex cascade already recorded in
`todos/gadget-host/wasm/01kwfz4kkaq7spwnm2ncket1ge-mutex-poisoning-cascade.md`
— therefore re-panics the *host* task handling the whole
request:

- In `search()`, the panic unwinds the async command task before
  `SearchMessage::Done` is sent (`gadget_host.rs:827`). The
  frontend's result channel never terminates for that
  generation.
- In the query fan-out specifically, one panicking gadget
  discards the not-yet-drained results of every other gadget in
  the `JoinSet`.

The comment at `gadget_host.rs:1246-1252` (test MockGadget)
shows the trait contract assumes well-behaved gadgets; for
third-party WASM gadgets that assumption doesn't hold — and the
bridge itself has known panic paths.

## Impact
One faulty gadget degrades the entire launcher search for every
keystroke that reaches it, instead of being isolated. Search
appears to hang (missing `Done`) or silently drops other
gadgets' results.

## Suggested fix
Treat `JoinError::is_panic()` as "this gadget produced no
results" — log which gadget panicked, optionally flip its
`enabled` flag (self-quarantine, matching the existing
enable-failure pattern at `gadget_host.rs:483-487`), and
continue draining the JoinSet. `Done` must be sent on all paths.
