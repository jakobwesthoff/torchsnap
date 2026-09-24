---
kind: bug
severity: medium
status: open
area: [src-tauri/src/gadgets/clipboard/storage.rs, src-tauri/src/gadgets/clipboard/mod.rs]
tags: [unconfirmed]
---

# `retentionDays = 0` silently wipes the entire clipboard history every 30 minutes

## Problem

`delete_expired_entries` builds the cutoff as a SQLite date
modifier (`storage.rs:347-352`):

```rust
pub fn delete_expired_entries(&self, retention_days: u32) -> Result<()> {
    let cutoff = format!("-{retention_days} days");
    ... WHERE captured_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?1)
```

With `retention_days = 0` the cutoff is `now`, i.e. *every*
existing entry matches and the whole history is deleted. The
retention thread runs this every 30 minutes
(`mod.rs:59,225-252`), and also immediately when the setting
changes (`setting_changed` notifies the condvar, `mod.rs:349-357`).

There is no validation anywhere on the path:

- `initialize_settings` defaults to 30 (`mod.rs:265`) but doesn't
  constrain the range.
- `setting_changed("retentionDays")` accepts any `u64` and stores
  `days as u32` (`mod.rs:352-353`) — a value ≥ 2^32 silently
  truncates (e.g. 4294967296 → 0, triggering exactly the wipe
  above).
- `enable()` reads `settings.get("retentionDays").unwrap_or(30)`
  (`mod.rs:315`) with the same lack of bounds.

Whether `0` should mean "keep forever" or "keep nothing" is
undefined in code and docs; users commonly expect 0/blank to mean
"disabled" (keep forever). The settings UI's slider constrains the
value to 1–365 (`src/gadgets/clipboard/ClipboardSettings.tsx:137-145`,
`min={1} max={365}`), so the normal UI path cannot produce 0 — the
remaining exposure is direct settings-store writes (any other
webview/tool writing `gadgets.clipboard-manager.retentionDays`),
future UI changes, and the `as u32` truncation aliasing to 0. The
backend enforcing its own invariant is still warranted; note the
calculator gadget already guards the same pattern with `.max(1)`
(`gadgets/calculator/src/lib.rs:345`).

## Impact

A user setting retention to 0 (or a bug/overflow producing 0)
irreversibly deletes their entire clipboard history within the
same half-hour, with no confirmation and no way back.

## Suggested fix

Decide the semantics and enforce them at both ends:

- If 0 = unlimited: `delete_expired_entries` returns early for 0,
  and the settings UI documents "0 = keep forever".
- If 0 is invalid: clamp to a minimum of 1 in `setting_changed`
  and `enable()`, and constrain the settings UI input.

Replace the `as u32` cast with `u32::try_from(days)` + clamp so
oversized values cannot alias to 0.
