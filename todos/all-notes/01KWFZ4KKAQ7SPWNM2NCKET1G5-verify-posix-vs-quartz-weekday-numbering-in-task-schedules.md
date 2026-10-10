---
title: "Verify POSIX-vs-Quartz weekday numbering in task schedules"
kind: investigation
component: gadget-host
status: needs-discussion
---
# Verify POSIX-vs-Quartz weekday numbering in task schedules

Scheduled task cron expressions are documented as POSIX cron, but the parser does not translate weekday numbers. If the `cron` crate uses Quartz numbering, a gadget author's `0 9 * * 1` ("every Monday 09:00") fires on Sunday. No test uses a numeric weekday, so the behavior is unpinned.

## Context
`parse_cron_schedule` (`src-tauri/src/wasm/manifest/tasks.rs`, around lines 71 to 99) documents the gadget-facing syntax as "5-field POSIX cron". It converts that to the `cron` crate's Quartz-style syntax by wrapping the expression with seconds and a year:

```rust
let normalized = format!("0 {schedule} *");
cron::Schedule::from_str(&normalized)
```

The wrapping fixes the field count but does not translate field semantics. POSIX cron numbers weekdays 0 to 6 with 0 = Sunday (7 is also accepted as Sunday by many crons). Quartz numbers them 1 to 7 with 1 = Sunday. If the `cron` crate follows Quartz numbering, then `0 9 * * 1` gets Sunday, and `* * * * 0` either errors or silently means something else. Every test in the file uses `*` in the weekday field, so the behavior is unpinned.

## Impact
Scheduled tasks firing on the wrong weekday, or valid POSIX expressions rejected, depending on the crate's numbering. Silent, and hard for gadget authors to notice.

## Options
- Write a test that parses `0 0 * * 1` and asserts the weekday of the next fire against a known date, plus one for `0` and `7` as Sunday aliases.
- If the crate is Quartz-numbered, either remap the weekday field during normalization (POSIX to Quartz: n+1, with 0 and 7 mapped to 1), or change the documented contract. Update the manifest docs either way.
- Month names and ranges, and `SUN`-style names, deserve the same check.
