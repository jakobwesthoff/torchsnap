---
title: "Switching a gadget off during its startup enable runs disable at the same time"
kind: bug
component: gadget-host
impact: low
origin: review
tags: [concurrency]
---
# Switching a gadget off during its startup enable runs disable at the same time

At startup, `initialize_and_start` in `src-tauri/src/gadget_host.rs` (Phase 2) calls each gadget's `enable()` directly, outside the slot's settings dispatcher. A switch-off arriving while a slow startup `enable()` runs goes through the dispatcher and runs `disable()` concurrently with it. Exit disable bypasses the dispatcher the same way. Found on 2026-10-21 in the review of the settings dispatcher fix, read in the code, not reproduced.

## Impact

Rare: a gadget can end up half-enabled or with resources from `enable()` that `disable()` already released.

## Suggested fix

Route the startup enable through the slot as well, with a start kind that distinguishes startup from a user switch-on, so enable and disable for one gadget are always serialized.
