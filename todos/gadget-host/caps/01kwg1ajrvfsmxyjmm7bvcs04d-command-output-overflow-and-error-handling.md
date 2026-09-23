# CommandCap output handling: overflow doesn't stop the child, read errors report success

**Kind:** possible-bug
**Severity:** medium
**Area:** src-tauri/src/caps/command.rs

## Problem
Three related defects in the output path of
`run_child_with_caps` / `read_capped`:

1. **Overflow doesn't terminate the child — `Timeout` masks
   `OutputTooLarge`.** `read_capped` reads at most `cap + 1`
   bytes and then simply returns
   (`src-tauri/src/caps/command.rs:387-406`). Nothing kills the
   child when the cap is hit. A child producing far more output
   than the cap fills the now-undrained pipe, blocks, never
   exits, and `timeout(child.wait())` elapses
   (`command.rs:327-328`) — so the caller receives
   `CommandCapError::Timeout` after waiting out the full
   timeout, when the true condition was "output too large". The
   `OutputTooLarge` variant is only reachable when the child's
   total output fits between `cap + 1` and roughly the pipe
   buffer size above the cap.

2. **Reader errors and panics collapse into "success with empty
   output".** In the normal-exit branch
   (`command.rs:331-342`), both the `JoinError` and the
   `io::Result` layers are flattened with `unwrap_or_else`
   producing `(Vec::new(), false, <error string>)` — and the
   error string is immediately discarded by the destructuring
   (`let (stdout_bytes, stdout_over, _) = ...`). A failed stdout
   read therefore yields `Ok(CommandResult { exit_code: Some(0),
   stdout: [], .. })`: the gadget observes a successful command
   with empty output and no hint of trouble.

3. **Dead partial-output collection on timeout.** The timeout
   branch awaits both reader tasks, binds their bytes, then
   explicitly discards them: `let _ = (stdout_bytes,
   stderr_bytes); Err(CommandCapError::Timeout)`
   (`command.rs:370-382`). The WIT `command-error::timeout`
   variant carries no payload, so the collection is pure dead
   code today — either the error type should carry partial
   output (useful for debugging what a killed command printed)
   or the collection should go.

Relatedly, `read_capped`'s third tuple element is always
`String::new()` (`command.rs:405`) and exists only to be
discarded — vestigial plumbing from an abandoned error-reporting
design.

## Impact
Gadgets get wrong signals: over-producing commands surface as
timeouts (after the full timeout latency, e.g. 10 s default),
and I/O read failures surface as silently-empty successful runs.
Both misdirect gadget-author debugging.

## Suggested fix
Kill the process group as soon as either reader hits the cap
(readers can signal via a shared flag / channel selected
alongside `child.wait()`), and return `OutputTooLarge`
distinctly. Propagate reader errors as an error variant instead
of empty-success. Decide whether `Timeout` should carry partial
output and remove the dead plumbing either way.
