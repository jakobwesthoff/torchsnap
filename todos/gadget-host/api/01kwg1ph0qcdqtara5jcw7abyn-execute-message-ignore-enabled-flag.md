---
kind: bug
severity: low
status: open
area: [src-tauri/src/gadget_host.rs]
tags: [unconfirmed]
---

# execute() and handle_message() dispatch to disabled gadgets

## Problem
The search paths consistently gate on the host-owned enabled
flag (`slot.is_active()`: `src-tauri/src/gadget_host.rs:803`,
`:834`, `:1269`), but the two dispatch entry points do not:

- `execute()` resolves the entry's slot command via
  `resolve_command` (post-ADR-55) and then finds the gadget
  slot purely by id (`gadget_host.rs:1051`), calling
  `slot.gadget.execute(&command)` even when `enabled` is false.
- `handle_message()` does the same (`gadget_host.rs:1073`).

A gadget can be disabled between the search that produced an
entry and the user pressing Enter (settings toggle, or a
self-disable on `enable()` failure). For WASM gadgets, disable
tears down runtime state, so a post-disable `execute()`/
`handle_message()` reaches a gadget whose instance-side state
is gone; whatever error surfaces is the bridge's incidental
behavior rather than a deliberate "gadget is disabled"
response. Custom-UI views that are still mounted in the
frontend keep a live message pipe to a disabled gadget the
same way.

## Impact
Undefined behavior at the disable boundary is currently decided
by each gadget/bridge code path instead of the host. Symptoms
would be confusing one-off errors right after disabling a
gadget.

## Suggested fix
Check `slot.is_active()` in both `execute()` (`gadget_host.rs:1022`)
and `handle_message()` (`gadget_host.rs:1066`) and return a
deliberate error (or `PostAction::Nothing` plus a log line) for
disabled gadgets, so the boundary is host-enforced and uniform.
