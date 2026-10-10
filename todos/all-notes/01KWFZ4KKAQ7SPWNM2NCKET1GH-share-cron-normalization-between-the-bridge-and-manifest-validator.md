---
title: "Share cron normalization between the bridge and manifest validator"
kind: refactor
component: gadget-host
impact: low
---
# Share cron normalization between the bridge and manifest validator

`WasmGadgetBridge::new` repeats the cron wrapping that the manifest validator already does. The two copies have to stay in sync, and a fix that lands in only one of them makes a schedule validate one way and fire another way.

## Problem
`WasmGadgetBridge::new` re-parses each task schedule with its own
inline normalization (`src-tauri/src/wasm/bridge.rs:237-253`):

```rust
let normalized = format!("0 {} *", task.schedule);
let schedule = Schedule::from_str(&normalized)...
```

This duplicates `manifest::tasks::parse_cron_schedule`
(`manifest/tasks.rs:71-99`), which performs the identical
wrapping plus the friendly field-count pre-check. The two copies
must stay in sync. Any fix to the normalization, such as the
POSIX/Quartz weekday remap (see the Relations section), has to
land in both, and missing one produces schedules that validate
one way at parse time and fire another way at runtime.

## Impact
Pure drift risk today; becomes a live bug the first time the
normalization logic changes.

## Proposal
Make `parse_cron_schedule` `pub(crate)` (it already is) and call
it from `WasmGadgetBridge::new` instead of re-implementing the
wrap. Alternatively, parse once at manifest load and carry the
`cron::Schedule` values to the bridge, eliminating the second
parse entirely.

## Relations

- Relates to: [Verify POSIX-vs-Quartz weekday numbering in task schedules](01KWFZ4KKAQ7SPWNM2NCKET1G5-verify-posix-vs-quartz-weekday-numbering-in-task-schedules.md), the weekday remap would have to land in both copies
