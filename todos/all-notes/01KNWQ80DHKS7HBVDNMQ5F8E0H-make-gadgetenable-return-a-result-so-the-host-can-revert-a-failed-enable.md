---
title: "Make Gadget::enable return a Result so the host can revert a failed enable"
kind: bug
component: gadget-host
status: needs-discussion
tags: [error-handling]
---
# Make Gadget::enable return a Result so the host can revert a failed enable

When a gadget's enable path fails, the host keeps the gadget flagged as enabled, so the settings UI shows it as enabled while it silently does nothing. For WASM gadgets the failure can come from the guest's own `lifecycle::enable` export returning `Err`, and the bridge can only log it, because `Gadget::enable` returns `()`.

## Context

There is no channel for a gadget to report an enable failure to `GadgetHost`, so the host's `AtomicBool enabled` flag is never reverted. The WASM instance destroy/recreate refactor (ADR 0033) made this worse: on a guest `enable()` failure the bridge now drops the `WasmGadgetInstance` back to `None` (the right semantic, see that ADR), but the host's `AtomicBool` stays `true`. The settings UI shows the gadget as enabled, subsequent dispatch hits the bridge's defensive `None` branch and logs a warning. The user sees a "working" gadget that silently does nothing.

## Current state

- `Gadget` trait in `src-tauri/src/gadgets/mod.rs`: `fn enable(&self, app: &AppHandle, ctx: &GadgetContext)`, with no return value.
- `GadgetHost::handle_setting_changed` for `enabled.<id>` calls `slot.gadget.enable(&app, &ctx)` and assumes success.
- `WasmGadgetBridge::enable` (post ADR 0033) logs and drops the instance on a guest-side failure. It cannot report the failure upward.

## Target

- `Gadget::enable` returns `Result<(), GadgetEnableError>` (or `anyhow::Error`, to be decided during design).
- `GadgetHost` checks the return value. On `Err` it flips the slot's `enabled` `AtomicBool` back to `false`, persists the settings store's `enabled.<id>` key back to `false`, logs the failure and notifies UI subscribers so the settings toggle reflects reality.
- Every existing `Gadget` impl is updated. Native gadgets mostly return `Ok(())`; the WASM bridge propagates the guest error.
- New tests: a gadget whose `enable` fails deterministically. After the failed transition, the host's `enabled` state and the settings store must both be `false`.

### Bridge disabled-dispatch handling upgrade

Once the host flips its `AtomicBool` back on enable failure, the window where the instance slot is `None` while the host thinks the gadget is enabled is gone. The `log_dispatched_while_disabled` helper in `WasmGadgetBridge` then becomes genuinely unreachable. The call sites in the trait methods (`setting_changed`, `entries`, `execute`, `search`, `handle_message`) should move from "`Error` log and no-op" to `debug_assert!(false, ...)`, or to `unreachable!()` in release, so future dispatch bugs crash loudly instead of silently doing nothing. The helper itself can be deleted at that point.

## Open questions

- Error type: a dedicated `GadgetEnableError` enum, or `anyhow::Error` for simplicity?
- UI feedback: should a failed enable surface a toast or notification to the user, or is the silent revert plus a log line enough?
- Retry semantics: does flipping the flag back mean the next `enabled.<id>: true` setting write is a fresh attempt, or should there be a cooldown or backoff to avoid toggle loops?

## References

- ADR 0025: enable/disable lifecycle foundation.
- ADR 0033: WASM instance lifecycle, which introduces the failure path this todo addresses.
- `src-tauri/src/gadget_host.rs`, `handle_setting_changed` for `enabled.<id>`: where the new error handling wires in.
- `src-tauri/src/wasm/bridge.rs`: the WASM bridge's enable path, the first real producer of the error.
