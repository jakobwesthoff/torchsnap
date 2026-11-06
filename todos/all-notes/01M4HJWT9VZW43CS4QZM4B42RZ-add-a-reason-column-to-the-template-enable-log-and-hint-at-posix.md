---
title: "Add a reason column to the template enable_log and hint at POSIX equivalents for cron macros"
kind: improvement
component: gadget-host
impact: low
origin: review
tags: [error-handling]
---
# Add a reason column to the template enable_log and hint at POSIX equivalents for cron macros

Two gadget-author developer-experience items surfaced by the post-migration validation review of 2026-04-08 (seven parallel review agents covering D1-D6 and the calculator port). The critical and important findings of that review were fixed in 11 follow-up commits. These items are lower-priority polish that was deliberately deferred. Neither blocks current functionality. They land as one PR because the friendlier cron error for `@daily` macros has gadget-author user impact, so it is worth combining with the template `reason` column change. Each item keeps the original review section number so back-references stay intact.

## #30: Template `enable_log` table mixes two event types

**Where:** `gadgets/template/migrations/001_init.sql` and `gadgets/template/src/lib.rs` (`enable()` and the `heartbeat` task).

**The thing:** the `enable_log` table is written from two places: the lifecycle `enable()` and the scheduled `heartbeat` task. No column tells the two event types apart. Gadget authors reading the demo see a table called `enable_log` holding both kinds of events and get confused.

**Fix:** add a `reason TEXT NOT NULL DEFAULT 'enable'` column to the migration. The `enable()` insert keeps the default, and the `heartbeat` task inserts with `reason = 'heartbeat'` explicitly. The frontend can then count each type with `SELECT COUNT(*) FROM enable_log WHERE reason = ?`.

This is a small but pedagogically valuable change. It shows gadget authors the right pattern for multi-event log tables instead of teaching them to mix unrelated events into one undifferentiated table.

**Migration concern:** the template is a greenfield example with no production databases, so this can be a destructive edit to `001_init.sql` rather than a `002_*.sql` add-column migration. Real gadget authors should add a new migration file for production schemas, and the template should call this out in a comment.

## #31: Friendlier rejection of Quartz `@daily` and `@hourly` macros

**Where:** `src-tauri/src/wasm/manifest.rs`, `parse_cron_schedule` (after the existing 5-field token-count check that landed in 3ecc17c). `manifest.rs` is now a module: `parse_cron_schedule` lives in `src-tauri/src/wasm/manifest/tasks.rs` (checked 2026-10-10).

**The thing:** gadget authors who write `@daily` or `@hourly` (Quartz aliases for common schedules) get a generic "expected 5-field POSIX cron, got 1 field(s)" error. The 1-field count is technically correct but not actionable: the author thinks `@daily` is a field, not an alias they should expand.

**Fix:** detect single-token inputs that start with `@` and emit a specific error that suggests the POSIX equivalent:

```rust
if let Some(macro_name) = schedule.strip_prefix('@') {
    let suggestion = match macro_name {
        "yearly" | "annually" => Some("0 0 1 1 *"),
        "monthly" => Some("0 0 1 * *"),
        "weekly" => Some("0 0 * * 0"),
        "daily" | "midnight" => Some("0 0 * * *"),
        "hourly" => Some("0 * * * *"),
        _ => None,
    };
    if let Some(equiv) = suggestion {
        anyhow::bail!(
            "Quartz `@{macro_name}` aliases are not supported — use the equivalent 5-field POSIX expression `{equiv}`"
        );
    }
}
```

Add tests `@daily_rejected_with_hint` and `@hourly_rejected_with_hint` covering the alias path. The existing `quartz_macro_at_daily_rejected` test already verifies that `@daily` fails. This change only makes the message more useful.
