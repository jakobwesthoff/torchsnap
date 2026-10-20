---
title: "Enable toggles and gadget setting changes share one dispatcher queue and drop each other"
kind: bug
component: gadget-host
origin: review
tags: [concurrency]
---
# Enable toggles and gadget setting changes share one dispatcher queue and drop each other

Each gadget slot has one `CoalescingDispatcher`, and `GadgetHost::handle_setting_changed` in `src-tauri/src/gadget_host.rs` enqueues both `enabled.<id>` changes and `gadgets.<id>.<key>` changes into it, each with its own dispatch callback. Whichever callback drains the queue gets the other path's keys too: the enable callback skips every key without the `enabled.` prefix, so a gadget setting change pending at that moment is lost, and the settings callback hands a pending `enabled.<id>` key to the gadget as a setting change and never applies the toggle. Found on 2026-10-21 while planning the Retry button for a failed enable, read in the code, not reproduced.

## Problem

- Path 1 (`enabled.<id>`): `slot.dispatcher.enqueue(key, value)`, then `dispatch` with a callback that returns early unless the key starts with `enabled.`.
- Path 2 (`gadgets.<id>.<key>`): `slot.dispatcher.enqueue(setting_key, value)` into the same dispatcher, then `dispatch` with a callback that calls `gadget.setting_changed(k, v)` for every key.
- A batch taken by one callback includes the other path's pending items, which that callback mishandles.

## Impact

Rare: it needs a gadget setting change and an enable toggle (or a Retry) for the same gadget pending at once. The setting change is then silently lost, or the toggle is not applied and the gadget receives a bogus setting key.

## Suggested fix

Give each path its own dispatcher per slot, or tag queued items with their path and dispatch both kinds from one callback. Test with a gate that holds one dispatch while the other path enqueues.

## Relations

- Relates to: [CoalescingDispatcher can strand the last enqueued change](01KWG0B6TYFPTJCAFW5TMAG25W-coalescingdispatcher-can-strand-the-last-enqueued-change.md), the lost-wakeup bug in the same dispatcher
