---
title: "Scheduled task ids are not validated"
kind: improvement
component: gadget-host
impact: low
---
# Scheduled task ids are not validated

`validate_task_definitions` checks task ids for uniqueness and cron validity, but it accepts any string as the id, including empty, whitespace-only, or very long values. The id goes back to the guest through `tasks::run-task(task-id)` and presumably appears in host logs and span names, so any code that keys on it inherits a degenerate value.

## Problem
`validate_task_definitions` (`src-tauri/src/wasm/manifest/tasks.rs`, around lines 48 to 58) checks uniqueness and cron validity but accepts any string as the task `id`. Compare `validate_gadget_id` (`src-tauri/src/wasm/manifest/mod.rs`), which enforces a strict charset for the analogous gadget identifier.

## Impact
`id = ""` loads successfully today. Any downstream code that keys logs, metrics, or storage on the task id inherits the degenerate value. Cosmetic until something keys on it.

## Proposal
Reuse the gadget-id charset rule (lowercase alphanumeric plus hyphen, non-empty) for task ids. Enforce it in `validate_task_definitions`, with a test for the empty-string case.
