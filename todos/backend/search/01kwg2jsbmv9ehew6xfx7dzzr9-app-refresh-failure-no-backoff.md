# App-launcher background refresh retries on every keystroke after a discovery failure

**Kind:** improvement
**Severity:** low
**Area:** src-tauri/src/gadgets/app_launcher.rs

## Problem

`maybe_trigger_background_refresh` (`app_launcher.rs:82-124`)
triggers a refresh whenever `now - last_refresh >= 300`. The
`last_refresh` timestamp is only advanced on *success*
(`app_launcher.rs:113` inside the `Ok` arm); the `Err` arm keeps
the old timestamp:

```rust
Err(e) => {
    // Keep serving the old cache rather than clearing it.
    eprintln!("background app discovery failed: {e:#}");
}
```

`entries()` is called on every search keystroke and calls
`maybe_trigger_background_refresh()` first
(`app_launcher.rs:190-191`). Once the interval has elapsed and
discovery is failing (Spotlight disabled, `mdfind` missing or
erroring), every keystroke spawns a new thread + `mdfind`
subprocess as soon as the previous failed attempt clears the
`refreshing` flag. If `mdfind` fails fast, that is one subprocess
spawn per keystroke, indefinitely, plus an `eprintln!` per attempt.

The same pattern applies after a failed `enable()` discovery
(`app_launcher.rs:181-183`): `last_refresh` stays 0, so the first
`entries()` call immediately triggers a (correct, desirable) retry
— that part is fine; only the lack of failure backoff is the
issue.

## Impact

Degenerate but sustained: on systems where Spotlight is disabled
(a real configuration), typing in the launcher continuously spawns
failing `mdfind` processes. No user-visible breakage, but wasted
work and log spam for the lifetime of the session.

## Suggested fix

Also store a timestamp on failure — either advance `last_refresh`
unconditionally after an attempt, or keep a separate
`last_attempt` used for the interval check. A shorter retry
interval on failure (e.g. 30 s) preserves the recovery property
without the per-keystroke spawn.
