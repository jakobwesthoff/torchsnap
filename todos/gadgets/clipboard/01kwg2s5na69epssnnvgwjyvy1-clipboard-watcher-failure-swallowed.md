---
kind: bug
severity: medium
status: open
area: [src-tauri/src/gadgets/clipboard/mod.rs]
tags: [unconfirmed]
---

# Clipboard gadget reports success when the watcher fails to start

## Problem

`enable()` treats a watcher startup failure as non-fatal
(`src-tauri/src/gadgets/clipboard/mod.rs:328-337`):

```rust
if let Err(e) = start_watcher(
    &self.platform,
    &shared,
    &self.lifecycle,
    &self.retention_condvar,
    &self.retention_days,
) {
    eprintln!("clipboard: failed to start watcher: {e:#}");
}
Ok(())
```

`start_watcher` fails when `ClipboardWatcherContext::new()` or
`ClipboardContext::new()` fails (`mod.rs:161-171`) — i.e. when the
platform clipboard cannot be acquired at all. In that case the
gadget still reports `enable()` success: it appears enabled, its
catalog entry and shortcut work, the history UI opens — but no new
clipboard changes are ever recorded, and no retention cleanup runs
(the retention thread is started by the same function, after the
failing calls).

The `Gadget::enable` contract has a dedicated failure path:
"Returning `Err` disables the gadget — no further calls
dispatched" (`src-tauri/src/gadgets/mod.rs` trait doc).

## Impact

A user whose clipboard context cannot be acquired silently gets a
frozen clipboard history (old entries only, never cleaned up) with
no indication anything is wrong — the error goes to stderr, which
is invisible in a packaged app (see
`todos/backend/01kwg2ftae87zzd75qgcpa6ttz-host-errors-eprintln-invisible-in-bundle.md`).

## Suggested fix

Propagate the error: `start_watcher(...)` should end `enable()`
with `?`. If partially-working behavior (browse-only history) is
actually desired, that is a product decision that should be
recorded; the current silent half-enabled state is the worst of
both options.
