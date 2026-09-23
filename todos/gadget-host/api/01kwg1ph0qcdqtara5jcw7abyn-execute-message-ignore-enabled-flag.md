# execute() and handle_message() dispatch to disabled gadgets

**Kind:** possible-bug
**Severity:** low
**Area:** src-tauri/src/gadget_host.rs

## Problem
The search paths consistently gate on the host-owned enabled
flag (`slot.is_active()`: `src-tauri/src/gadget_host.rs:737`,
`:768`, `:1187`), but the two dispatch entry points do not:

- `execute()` finds the slot purely by id
  (`gadget_host.rs:987`) and calls `slot.gadget.execute(...)`
  even when `enabled` is false.
- `handle_message()` does the same (`gadget_host.rs:1017-1019`).

A gadget can be disabled between the search that produced an
entry and the user pressing Enter (settings toggle, or the
self-disable on `enable()` failure at `gadget_host.rs:483-487`
and `:1071-1074`). For WASM gadgets, disable tears down runtime
state, so a post-disable `execute()`/`handle_message()` reaches
a gadget whose instance-side state is gone; whatever error
surfaces is the bridge's incidental behavior rather than a
deliberate "gadget is disabled" response. Custom-UI views that
are still mounted in the frontend keep a live message pipe to a
disabled gadget the same way.

## Impact
Undefined behavior at the disable boundary is currently decided
by each gadget/bridge code path instead of the host. Symptoms
would be confusing one-off errors right after disabling a
gadget.

## Suggested fix
Check `slot.is_active()` in both `execute()` and
`handle_message()` and return a deliberate error (or
`PostAction::Nothing` plus a log line) for disabled gadgets, so
the boundary is host-enforced and uniform.
