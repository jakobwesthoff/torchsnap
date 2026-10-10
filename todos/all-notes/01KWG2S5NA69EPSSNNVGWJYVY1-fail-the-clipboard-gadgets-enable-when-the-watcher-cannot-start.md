---
title: "Fail the clipboard gadget's enable when the watcher cannot start"
kind: bug
component: gadgets
---
# Fail the clipboard gadget's enable when the watcher cannot start

`enable()` treats a clipboard watcher startup failure as non-fatal, so the
gadget reports success while no clipboard changes are recorded and no
retention cleanup runs. The user sees an apparently enabled gadget with a
frozen history.

## Problem

Read in the code, not reproduced.

`enable()` treats a watcher startup failure as non-fatal
(`src-tauri/src/gadgets/clipboard/mod.rs`, around lines 336-345):

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
`ClipboardContext::new()` fails (`mod.rs`, around lines 161-171), that is,
when the platform clipboard cannot be acquired at all. In that case the
gadget still reports `enable()` success: it appears enabled, its catalog
entry and shortcut work, the history UI opens, but no new clipboard
changes are ever recorded, and no retention cleanup runs (the retention
thread is started by the same function, after the failing calls).

The `Gadget::enable` contract has a dedicated failure path:
"Returning `Err` disables the gadget, no further calls
dispatched" (`src-tauri/src/gadgets/mod.rs`, trait doc).

## Impact

A user whose clipboard context cannot be acquired silently gets a
frozen clipboard history (old entries only, never cleaned up) with
no indication anything is wrong. The error goes to stderr, which is
invisible in a packaged app.

## Suggested fix

Propagate the error: `start_watcher(...)` should end `enable()`
with `?`. If partially-working behavior (browse-only history) is
actually desired, that is a product decision that should be
recorded; the current silent half-enabled state is the worst of
both options.

## Relations

- Relates to: [Host errors go only to stderr and are invisible in a packaged app](01KWG2FTAE87ZZD75QGCPA6TTZ-host-errors-go-only-to-stderr-and-are-invisible-in-a-packaged-app.md), the stderr output this failure is written to
