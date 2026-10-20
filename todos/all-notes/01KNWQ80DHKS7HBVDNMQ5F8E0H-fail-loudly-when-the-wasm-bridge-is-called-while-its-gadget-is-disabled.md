---
title: "Fail loudly when the WASM bridge is called while its gadget is disabled"
kind: bug
component: gadget-host
status: blocked
---
# Fail loudly when the WASM bridge is called while its gadget is disabled

`Gadget::enable` returns a `Result` and the host handles a failure, so the original problem is solved. What remains is the bridge's handling of calls that reach a disabled gadget.

## Done

- `Gadget::enable` returns `anyhow::Result<()>` (`WasmGadgetBridge::enable` in `src-tauri/src/wasm/bridge.rs`).
- On `Err` the host clears the slot's runtime flag and keeps the error per gadget id (`EnableFailures` in `src-tauri/src/gadget_status.rs`). Settings show it, with a Retry button (ADR 0063, ADR 0064).
- The stored `enabled.<id>` setting stays on after a failure. That is decided in ADR 0063, so the earlier idea of writing `false` back to the settings store is dropped. The toast question and the retry semantics are answered by the same two ADRs.

## Remaining: the bridge's disabled-dispatch handling

`WasmGadgetBridge::log_dispatched_while_disabled` still logs an `Error` with a `bug:` prefix and does nothing else. It is called from `setting_changed`, `handle_message`, `entries`, `execute` and `search`. Once no path can reach a disabled bridge, these call sites should become `debug_assert!(false, ...)`, or `unreachable!()` in release, so a future dispatch bug fails loudly. The helper can then be deleted.

The paths into the bridge while the gadget is disabled are not closed yet. `GadgetHost::handle_setting_changed` still delivers `gadgets.<id>.*` changes to a disabled slot, which logs exactly this message for every change. Fix that first.

## Relations

- Depends on: [Settings changes are delivered to disabled gadgets](01M5DQ9P07CV15003RN7F9M5NG-settings-changes-are-delivered-to-disabled-gadgets.md)
