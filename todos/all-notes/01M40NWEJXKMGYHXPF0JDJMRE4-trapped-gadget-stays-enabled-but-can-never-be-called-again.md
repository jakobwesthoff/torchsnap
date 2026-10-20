---
title: "Trapped gadget stays enabled but can never be called again"
kind: bug
component: gadget-host
origin: implementation
tags: [error-handling]
---
# Trapped gadget stays enabled but can never be called again

A gadget whose guest call fails stays enabled with a dead component. That covers a panic in the gadget, which `panic = "abort"` in `gadgets/Cargo.toml` turns into a trap, and any other error out of the call. Every later call then fails, and only disabling and re-enabling the gadget or restarting Torchsnap recovers it. This was found while researching native keep-awake (`docs/research/native-keep-awake.md`, section 7.1), where it would leave a wake lock held with no way to end it from the launcher.

## Problem

wasmtime 49.0.0 (`src-tauri/Cargo.lock`) poisons the store on any failed
component call:

- `src/runtime/component/func.rs:389-391`: `if result.is_err() {
  store.0.set_trapped(); }`
- `func.rs:468-469`: every later call bails with
  `Trap::CannotEnterComponent` once `!store.0.may_enter()`
- `src/runtime/component/store.rs:292-298`: `may_enter` is false "only
  [if] self has been poisoned due to a trap"

The bridge treats every guest-call error as a one-off and keeps the
instance:

- `search()` logs `search() failed` and returns `None`
  (`Gadget::search` in `src-tauri/src/wasm/bridge.rs`). The gadget silently drops out
  of the results and logs an error on every keystroke.
- `entries()` logs and returns an empty list (`Gadget::entries` in `bridge.rs`).
- `execute()` and `handle_message()` return the error to the caller
  (`Gadget::execute` and `Gadget::handle_message` in `bridge.rs`).
- `setting_changed()` logs (`Gadget::setting_changed` in `bridge.rs`).
- Scheduled tasks log `scheduled task ... failed` on every fire, and the
  scheduler keeps running (the task scheduler in `bridge.rs`).

Only a failed `enable()` tears the instance down (`Gadget::enable` in
`bridge.rs`, ADR 0033), and `GadgetHost` then clears the slot's runtime
flag and keeps the error for Settings (`enable_gadget` in
`src-tauri/src/gadget_host.rs`, `EnableFailures` in
`src-tauri/src/gadget_status.rs`, ADR 0063). Only the `enable()`
call recognises a trap (`guest_trapped` in
`src-tauri/src/wasm/runtime/instance.rs`, which words the error). Nothing
looks for the trapped state after any other call (`may_enter` is not
used anywhere in `src-tauri/src`).

`disable()` still works on a trapped instance. The guest `disable()`
call fails and is logged, but `clear_caps()` runs and the store is
dropped (`Gadget::disable` in `bridge.rs`). A later `enable()` builds a fresh
instance.

## Impact

- The gadget disappears from search without any visible error. The log
  fills with one error per keystroke and per scheduled fire.
- Resources the host holds for the gadget stay held until disable or
  quit. These include SQL handles in the store's `ResourceTable`, and any
  future host-owned resource such as the wake lock proposed in
  `docs/research/native-keep-awake.md`. For an infinite keep-awake
  session, that would keep the Mac awake with no launcher entry to end it.
- Third-party gadgets are user-installable (ADR 0035), so a buggy gadget
  is an expected case, not an edge case.

## Suggested fix

Detect the poisoned state in the bridge and tear the instance down the
way a failed `enable()` does. Stop the scheduler, run `clear_caps()`,
drop the store, and clear the slot's `enabled` flag in `GadgetHost`.
Detection can use the error (downcast to `wasmtime::Trap`) or ask the
store whether it is still enterable before and after each call. Log the
trap once, with the gadget id and the call that failed.

## Open questions

- Quarantine until the user re-enables the gadget, or restart it
  automatically with a backoff and a retry limit?
- Is the in-memory `enabled` flag alone enough, or should the settings
  UI show the gadget as crashed? A failed `enable()` is now shown in
  Settings with its error and a Retry button (ADR 0063, ADR 0064), so a
  trapped gadget could reuse that mechanism.
- Should an error returned by a host import, which also poisons the
  store, be treated like a guest panic?

## Tests

A fixture gadget under `src-tauri/tests/fixtures/` panics on
demand in `search`, `execute`, `handle-message`, `run-task` and
`on-setting-changed`. The test asserts that after the first failure the
instance is gone, no further calls reach it, its resources are released,
and re-enabling gives a working instance.

## Relations

- Relates to: [Keep a panicking gadget from aborting the whole search and execute pipeline](01KWG1PH0QCDQTARA5JCW7ABYK-keep-a-panicking-gadget-from-aborting-the-whole-search-and-execute.md), host-side panics while running gadget code. This todo covers failures inside the component.
- Relates to: [Mutex-poisoning `expect`s turn one panic into a permanent cascade](01KWFZ4KKAQ7SPWNM2NCKET1GE-mutex-poisoning-expects-turn-one-panic-into-a-permanent-cascade.md), host panics that poison the store mutex
