---
kind: bug
severity: medium
status: open
area: [src-tauri/src/wasm/bridge.rs, src-tauri/src/wasm/runtime/instance.rs, src-tauri/src/gadget_host.rs]
tags: [error-handling]
---

# A trapped WASM gadget stays enabled but can never be called again

A gadget whose guest call fails (a panic in the gadget, which
`panic = "abort"` in `gadgets/Cargo.toml` turns into a trap, or any
other error out of the call) is left enabled with a dead component. The
host keeps dispatching to it, every call fails, and nothing recovers it
short of disabling and re-enabling the gadget or restarting Torchsnap.
Found while researching native keep-awake
(`docs/research/native-keep-awake.md`, section 7.1), where it would leave
a wake lock held with no way to end it from the launcher.

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
  (`src-tauri/src/wasm/bridge.rs:760-763`). The gadget silently drops out
  of the results and logs an error on every keystroke.
- `entries()` logs and returns an empty list (`bridge.rs:722-725`).
- `execute()` and `handle_message()` return the error to the caller
  (`bridge.rs:729-735`, `bridge.rs:668-701`).
- `setting_changed()` logs (`bridge.rs:646`).
- Scheduled tasks log `scheduled task ... failed` on every fire and the
  scheduler keeps running (`bridge.rs:495-497`).

Only a failed `enable()` tears the instance down (`bridge.rs:581-601`,
ADR 0033), and `GadgetHost` then clears the slot's `enabled` flag
(`src-tauri/src/gadget_host.rs:567-568`, `1124-1125`). Nothing looks for
the trapped state anywhere else (`rg -i 'trapped|Trap::' src-tauri/src`
finds nothing).

`disable()` still works on a trapped instance: the guest `disable()`
call fails and is logged, but `clear_caps()` runs and the store is
dropped (`bridge.rs:604-621`), and a later `enable()` builds a fresh
instance.

## Impact

- The gadget disappears from search without any visible error; the log
  fills with one error per keystroke and per scheduled fire.
- Resources the host holds for the gadget stay held until disable or
  quit: SQL handles in the store's `ResourceTable`, and any future
  host-owned resource such as the wake lock proposed in
  `docs/research/native-keep-awake.md`, where an infinite keep-awake
  session would keep the Mac awake with no launcher entry to end it.
- Third-party gadgets are user-installable (ADR 0035), so a buggy gadget
  is an expected case, not an edge case.

## Suggested fix

Detect the poisoned state in the bridge and tear the instance down the
way a failed `enable()` does: stop the scheduler, run `clear_caps()`,
drop the store, and clear the slot's `enabled` flag in `GadgetHost`.
Detection can use the error (downcast to `wasmtime::Trap`) or ask the
store whether it is still enterable before and after each call. Log the
trap once with the gadget id and the call that failed.

Open choices:

- Quarantine until the user re-enables the gadget, or restart it
  automatically with a backoff and a retry limit.
- Whether the in-memory `enabled` flag alone is enough, or the settings
  UI should show the gadget as crashed (today a failed `enable()` also
  only flips the in-memory flag).
- Whether an error returned by a host import, which also poisons the
  store, should be treated like a guest panic.

Tests: a fixture gadget under `src-tauri/tests/fixtures/` that panics on
demand in `search`, `execute`, `handle-message`, `run-task` and
`on-setting-changed`, asserting that after the first failure the
instance is gone, no further calls reach it, its resources are released
and re-enabling gives a working instance.

Related: `todos/backend/search/01kwg1ph0qcdqtara5jcw7abyk-gadget-panic-propagates-into-search.md`
(host-side panics while running gadget code) and
`todos/gadget-host/wasm/01kwfz4kkaq7spwnm2ncket1ge-mutex-poisoning-cascade.md`
(host panics poisoning the store mutex). Both are about host panics; this
todo is about failures inside the component.
