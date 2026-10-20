---
title: "Settings changes are delivered to disabled gadgets"
kind: bug
component: gadget-host
impact: low
origin: review
tags: [logging]
---
# Settings changes are delivered to disabled gadgets

`GadgetHost::handle_setting_changed` in `src-tauri/src/gadget_host.rs` dispatches `gadgets.<id>.<key>` changes without checking whether the gadget is active. A disabled WASM gadget then logs `bug: setting_changed dispatched on disabled gadget` (`src-tauri/src/wasm/bridge.rs`, the guard that builds that message) for every change. Found on 2026-10-21 in the review of the settings dispatcher fix, read in the code.

## Impact

Log noise that reads like a host bug; no wrong behaviour, since the bridge refuses the call.

## Suggested fix

Skip setting delivery for an inactive slot and test it; decide whether a gadget should get the changed values when it is enabled again (it reads settings in `enable()` today).
