---
kind: refactor
severity: low
status: open
area: [src-tauri/src/wasm/bridge.rs]
---

# Cron normalization duplicated between bridge and manifest validator

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
must stay in sync; any fix to the normalization (for example the
POSIX/Quartz weekday remap raised in
`01kwfz4kkaq7spwnm2ncket1g5-cron-weekday-semantics.md`) has to
land in both, and missing one produces schedules that validate
one way at parse time and fire another way at runtime.

## Impact
Pure drift risk today; becomes a live bug the first time the
normalization logic changes.

## Suggested fix
Make `parse_cron_schedule` `pub(crate)` (it already is) and call
it from `WasmGadgetBridge::new` instead of re-implementing the
wrap. Alternatively, parse once at manifest load and carry the
`cron::Schedule` values to the bridge, eliminating the second
parse entirely.
