---
title: "A settings change can wait in Tauri until the next event, and a listener panic stops all Rust listeners"
kind: bug
component: backend
origin: upstream
tags: [concurrency, dependencies]
---
# A settings change can wait in Tauri until the next event, and a listener panic stops all Rust listeners

Settings changes reach the Rust side as a Tauri event. In tauri 2.11.6, `emit_filter` has its own lost wakeup: a pending emit is flushed only by a holder whose event had a Rust handler, and the Retry thread and the shortcut reactor emit events that have none. A settings-changed emit can then wait until the next change. All Rust listeners also run under one global `handlers` mutex, so a panic in any listener poisons it and stops every Rust event listener of the app for good. Found on 2026-10-21 in the review of the settings dispatcher fix, read in the tauri 2.11.6 source, not reproduced.

## Impact

Rare: a setting change or switch applies late, only once something else changes. A panicking listener silently disables all settings reactions until restart.

## Suggested fix

Send settings changes from the frontend to Rust through a command instead of an event, so delivery does not depend on Tauri's emit queue and a handler panic stays local to its call. Check the current tauri version first; the upstream behaviour may have changed.
