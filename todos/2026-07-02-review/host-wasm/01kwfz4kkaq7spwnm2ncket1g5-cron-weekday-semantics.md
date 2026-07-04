# Verify POSIX-vs-Quartz weekday numbering in task schedules

**Kind:** question
**Severity:** medium
**Area:** src-tauri/src/wasm/manifest/tasks.rs

## Problem
`parse_cron_schedule` documents the gadget-facing syntax as
"5-field POSIX cron" and converts it to the `cron` crate's
Quartz-style syntax by wrapping with seconds and year
(`src-tauri/src/wasm/manifest/tasks.rs:71-99`):

```rust
let normalized = format!("0 {schedule} *");
cron::Schedule::from_str(&normalized)
```

The wrapping fixes the field count but does not translate field
semantics. POSIX cron numbers weekdays 0-6 with 0 = Sunday
(7 also accepted as Sunday by many crons). Quartz numbers them
1-7 with 1 = Sunday. If the `cron` crate follows Quartz
numbering, then a gadget author writing the POSIX expression
`0 9 * * 1` ("every Monday 09:00") gets Sunday, and `* * * * 0`
either errors or silently means something else. No test in the
file uses a numeric weekday; every test uses `*` in that field,
so the behavior is unpinned.

## Impact
Scheduled tasks firing on the wrong weekday, or valid POSIX
expressions rejected, depending on the crate's numbering. Silent
and hard for gadget authors to notice.

## Suggested fix
Write a test that parses `0 0 * * 1` and asserts the weekday of
the next fire against a known date, plus one for `0` and `7` as
Sunday aliases. If the crate is Quartz-numbered, either remap
the weekday field during normalization (POSIX→Quartz: n+1, with
0/7→1) or change the documented contract, and update the
manifest docs either way. Month names/ranges and `SUN`-style
names deserve the same check.
