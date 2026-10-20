---
title: "Back off background app refresh after a discovery failure"
kind: improvement
component: gadgets
impact: low
---

# Back off background app refresh after a discovery failure

After a failed background discovery, the app launcher retries on every
keystroke, because the failure path never advances the refresh
timestamp. With Spotlight disabled, that spawns a failing `mdfind`
process per keystroke for the rest of the session.

## Problem

`maybe_trigger_background_refresh` (`app_launcher.rs`)
triggers a refresh whenever `now - last_refresh >= 300`. The
`last_refresh` timestamp is only advanced on success
(in the `Ok` arm of the refresh thread). The `Err` arm keeps
the old timestamp:

```rust
Err(e) => {
    // Keep serving the old cache rather than clearing it.
    eprintln!("background app discovery failed: {e:#}");
}
```

`entries()` is called on every search keystroke and calls
`maybe_trigger_background_refresh()` first
(in `AppLauncherGadget::entries`). Once the interval has elapsed and
discovery is failing (Spotlight disabled, `mdfind` missing or
erroring), every keystroke spawns a new thread and `mdfind`
subprocess as soon as the previous failed attempt clears the
`refreshing` flag. If `mdfind` fails fast, that is one subprocess
spawn per keystroke, indefinitely, plus an `eprintln!` per attempt.

The same pattern applies after a failed `enable()` discovery
(the `Err` arm in `enable()`): `last_refresh` stays 0, so the first
`entries()` call immediately triggers a retry. That retry is correct and
desirable. Only the lack of failure backoff is the issue.

## Impact

Degenerate but sustained. On systems where Spotlight is disabled (a real
configuration), typing in the launcher continuously spawns failing
`mdfind` processes. There is no user-visible breakage, but the work is
wasted and the log fills with messages for the lifetime of the session.

## Suggested fix

Also store a timestamp on failure. Either advance `last_refresh`
unconditionally after an attempt, or keep a separate `last_attempt` used
for the interval check. A shorter retry interval on failure (e.g. 30 s)
preserves the recovery property without the per-keystroke spawn.
