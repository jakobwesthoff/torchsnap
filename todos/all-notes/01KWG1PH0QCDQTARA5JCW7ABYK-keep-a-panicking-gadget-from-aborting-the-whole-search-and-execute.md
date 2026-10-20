---
title: "Keep a panicking gadget from aborting the whole search and execute pipeline"
kind: bug
component: gadget-host
---

# Keep a panicking gadget from aborting the whole search and execute pipeline

Every place where the host runs gadget code on the blocking pool unwraps
the `JoinHandle` result with an `expect` that panics. A panic in any
single gadget therefore aborts the host task that handles the request,
which stops the search result stream and drops other gadgets' results.
Read in the code, not reproduced.

## Problem

Every place the host runs gadget code on the blocking pool
unwraps the `JoinHandle` result with a panic-on-panic `expect`:

- prefix search: `.expect("prefix search task not panicked")`
  (`GadgetHost::search` in `src-tauri/src/gadget_host.rs`)
- catalog search: `.expect("catalog search task not panicked")`
  (`GadgetHost::search` in `gadget_host.rs`)
- query fan-out: `result.expect("query search task not
  panicked")` (`GadgetHost::search` in `gadget_host.rs`)
- execute: `.expect("search_execute task must not panic")`
  (`search_execute` in `src-tauri/src/commands/mod.rs`)

A panic inside any single gadget's `search()` / `entries()` /
`execute()` (a native gadget bug, or a WASM-bridge panic such as the
poisoned-mutex cascade described in the related todo) therefore
re-panics the *host* task handling the whole request:

- In `search()`, the panic unwinds the async command task before
  `SearchMessage::Done` is sent (`GadgetHost::search`). The
  frontend's result channel never terminates for that
  generation.
- In the query fan-out specifically, one panicking gadget
  discards the not-yet-drained results of every other gadget in
  the `JoinSet`.

The comment on the test `MockGadget` in `gadget_host.rs`
shows the trait contract assumes well-behaved gadgets. For
third-party WASM gadgets that assumption doesn't hold, and the
bridge itself has known panic paths.

## Impact

One faulty gadget degrades the entire launcher search for every
keystroke that reaches it, instead of being isolated. Search
appears to hang (missing `Done`) or silently drops other
gadgets' results.

## Suggested fix

Treat `JoinError::is_panic()` as "this gadget produced no
results": log which gadget panicked, optionally flip its
`enabled` flag (self-quarantine, matching the existing
enable-failure pattern in `enable_gadget` in `gadget_host.rs`), and
continue draining the JoinSet. `Done` must be sent on all paths.

## The same class in settings delivery

Found on 2026-10-21 in the review of the settings dispatcher fix. A
gadget panicking inside `on_setting_changed` or `enable()` while its
slot's settings dispatcher runs leaves that dispatcher dead: later
switches and setting changes for that gadget are never delivered. On
the Tauri listener path the panic also poisons Tauri's global
`handlers` mutex, which stops every Rust event listener in the app.
The reviewers weighed a batch guard that keeps the dispatcher alive
(it does not help the listener path) and `catch_unwind` per callback
(protects both); the fix should cover this path together with the
search and execute pipeline.

## Relations

- Relates to: [Mutex-poisoning `expect`s turn one panic into a permanent cascade](01KWFZ4KKAQ7SPWNM2NCKET1GE-mutex-poisoning-expects-turn-one-panic-into-a-permanent-cascade.md), one of the WASM-bridge panic paths that reach this pipeline.
