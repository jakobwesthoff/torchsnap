---
kind: improvement
status: open
---

# Gadget::enable should return Result so the host can react to failure

## Context

Today `Gadget::enable(&self, app, ctx)` returns `()`. If a gadget's
enable path fails — for WASM gadgets, that includes the guest's own
`lifecycle::enable` export returning `Err` — the bridge can only log
the error. There is no channel to tell `GadgetHost` "I failed, please
flip my `AtomicBool enabled` flag back to false."

The WASM instance destroy/recreate refactor (ADR 0033) made this worse:
on guest `enable()` failure the bridge now drops the `WasmGadgetInstance`
back to `None` (the right semantic — see that ADR), but the host's
`AtomicBool` stays `true`. The settings UI shows the gadget as enabled;
subsequent dispatch hits the bridge's defensive `None` branch and logs a
warning. The user sees a "working" gadget that silently does nothing.

## Current State

- `Gadget` trait in `src-tauri/src/gadgets/mod.rs`: `fn enable(&self,
  app: &AppHandle, ctx: &GadgetContext)` — no return value
- `GadgetHost::handle_setting_changed` for `enabled.<id>` calls
  `slot.gadget.enable(&app, &ctx)` and assumes success
- `WasmGadgetBridge::enable` (post-ADR-0033) logs + drops instance on
  guest-side failure; cannot report failure upward

## Target

- `Gadget::enable` returns `Result<(), GadgetEnableError>` (or
  `anyhow::Error` — TBD during design)
- `GadgetHost` checks the return and on `Err` flips the slot's `enabled`
  `AtomicBool` back to `false`, persists the settings store's
  `enabled.<id>` key back to `false`, logs the failure, and notifies
  any UI subscribers so the settings toggle reflects reality
- Every existing `Gadget` impl updated (native gadgets: most simply
  `Ok(())` their way through; WASM bridge propagates the guest error)
- New tests: a gadget whose `enable` fails deterministically, assert
  that after the failed transition the host's `enabled` state is `false`
  and the settings store reflects that

### Bridge disabled-dispatch handling upgrade

Once the host flips its `AtomicBool` back on enable failure, the
"instance slot is `None` while the host thinks the gadget is enabled"
window is eliminated. At that point the `log_dispatched_while_disabled`
helper in `WasmGadgetBridge` becomes genuinely unreachable, and the
call sites in the trait methods (`setting_changed`, `entries`,
`execute`, `search`, `handle_message`) should be upgraded from
"`Error` log + no-op" to `debug_assert!(false, ...)` — or an outright
`unreachable!()` in release — so future dispatch bugs crash loudly
instead of silently no-opping. The helper itself can be deleted at
that point.

## Open Questions

- Error type: introduce a dedicated `GadgetEnableError` enum, or just
  use `anyhow::Error` for simplicity?
- UI feedback: should a failed enable surface a toast/notification to
  the user, or is the silent revert + log line enough?
- Retry semantics: does flipping the flag back mean the next
  `enable.<id>: true` setting write is treated as a fresh attempt, or
  should there be a cooldown / backoff to avoid toggle loops?

## References

- ADR 0025 — enable/disable lifecycle foundation
- ADR 0033 — WASM instance lifecycle (introduces the failure path this
  todo addresses)
- `src-tauri/src/gadget_host.rs` — `handle_setting_changed` for
  `enabled.<id>` is where the new error handling wires in
- `src-tauri/src/wasm/bridge.rs` — WASM bridge's enable path, first
  real producer of the error
