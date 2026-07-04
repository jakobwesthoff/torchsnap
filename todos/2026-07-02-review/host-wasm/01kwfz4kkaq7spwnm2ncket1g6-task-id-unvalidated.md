# Scheduled task ids are not validated

**Kind:** improvement
**Severity:** low
**Area:** src-tauri/src/wasm/manifest/tasks.rs

## Problem
`validate_task_definitions`
(`src-tauri/src/wasm/manifest/tasks.rs:48-58`) checks uniqueness
and cron validity but accepts any string as the task `id`,
including empty, whitespace-only, or arbitrarily long values.
The id is passed back to the guest via `tasks::run-task(task-id)`
and presumably appears in host logs/span names. Compare
`validate_gadget_id` (`manifest/mod.rs:185-204`), which enforces
a strict charset for the analogous gadget identifier.

## Impact
`id = ""` loads successfully today; any downstream code that
keys logs, metrics, or storage on the task id inherits the
degenerate value. Cosmetic until something keys on it.

## Suggested fix
Reuse the gadget-id charset rule (lowercase alphanumeric plus
hyphen, non-empty) for task ids, enforced in
`validate_task_definitions` with a test for the empty-string
case.
