---
title: "A hanging mdfind stops app discovery until Torchsnap restarts"
kind: bug
component: platform
origin: review
tags: [macos, error-handling]
---
# A hanging mdfind stops app discovery until Torchsnap restarts

Found in the 2026-11-06 plan review of the empty-discovery fix.
`Mdfind::query` has no timeout, so an `mdfind` that never exits blocks
discovery for good. It matters on a machine where the Spotlight daemons
crash repeatedly, as they did on 2026-11-04 to 2026-11-06.

## Problem

Read in the code, not reproduced (2026-11-06).

`Mdfind::query` in `src-tauri/src/platform/macos/app_discovery.rs` calls
`Command::output()`, which waits without limit. In the background refresh,
`refreshing` stays set while the thread waits, so
`maybe_trigger_background_refresh` in
`src-tauri/src/gadgets/app_launcher.rs` starts no further refresh. A hang
in `enable()` means the first list never arrives.

## Impact

The app list stops following installs and removals until restart, or
stays empty when the hang happens at startup. Nothing reports it.

## Suggested fix

Give the `mdfind` run a time limit. On expiry, kill and reap the child and
return `Err`, so the existing failure path keeps the list and retries
later. The child's stdout must be drained while waiting, since a full pipe
would otherwise block the child and look like a hang.

## Done when

- A run past the limit ends with `Err`, leaving no child process behind.
- Output larger than the pipe buffer still arrives whole.
- `CHANGELOG.md` has an entry under `[Unreleased]`.

## Relations

- Relates to: [App launcher replaces its whole app list when Spotlight returns nothing](01M6RCCXE19QR476ZZ5QY41ZET-app-launcher-replaces-its-whole-app-list-when-spotlight-returns-nothing.md), same discovery failure path.
