---
title: "Command output overflow reports a timeout, and output read errors report success"
kind: bug
component: gadget-host
tags: [capabilities]
---
# Command output overflow reports a timeout, and output read errors report success

The output path of `run_child_with_caps` and `read_capped` sends the wrong signal in three ways. Overflow does not stop the child, so it surfaces as a timeout. Read errors surface as a successful run with empty output. Partial output is collected on timeout and then thrown away.

## Problem
Read in the code, not reproduced.

Three related defects in the output path of
`run_child_with_caps` / `read_capped`:

1. **Overflow doesn't terminate the child, and `Timeout` masks
   `OutputTooLarge`.** `read_capped` reads at most `cap + 1`
   bytes and then simply returns
   (`read_capped` in `src-tauri/src/caps/command.rs`). Nothing kills the
   child when the cap is hit. A child producing far more output
   than the cap fills the now-undrained pipe, blocks, never
   exits, and `timeout(child.wait())` elapses
   (`run_child_with_caps` in `command.rs`). The caller therefore receives
   `CommandCapError::Timeout` after waiting out the full
   timeout, when the true condition was "output too large". The
   `OutputTooLarge` variant is only reachable when the child's
   total output fits between `cap + 1` and roughly the pipe
   buffer size above the cap.
2. **Reader errors and panics collapse into "success with empty
   output".** In the normal-exit branch
   (`run_child_with_caps` in `command.rs`), both the `JoinError` and the
   `io::Result` layers are flattened with `unwrap_or_else`,
   producing `(Vec::new(), false, <error string>)`. The error string is
   immediately discarded by the destructuring
   (`let (stdout_bytes, stdout_over, _) = ...`). A failed stdout
   read therefore yields `Ok(CommandResult { exit_code: Some(0),
   stdout: [], .. })`: the gadget observes a successful command
   with empty output and no hint of trouble.
3. **Dead partial-output collection on timeout.** The timeout
   branch awaits both reader tasks, binds their bytes, then
   explicitly discards them: `let _ = (stdout_bytes,
   stderr_bytes); Err(CommandCapError::Timeout)`
   (`run_child_with_caps` in `command.rs`). The WIT `command-error::timeout`
   variant carries no payload, so the collection is pure dead
   code today. Either the error type should carry partial
   output (useful for debugging what a killed command printed),
   or the collection should go.

Relatedly, `read_capped`'s third tuple element is always
`String::new()` (`read_capped` in `command.rs`) and exists only to be
discarded. It is vestigial plumbing from an abandoned error-reporting
design.

## Impact
Gadgets get wrong signals. Over-producing commands surface as
timeouts (after the full timeout latency, e.g. 10 s default),
and I/O read failures surface as silently-empty successful runs.
Both misdirect gadget-author debugging.

## Suggested fix
Kill the process group as soon as either reader hits the cap.
Readers can signal via a shared flag or channel selected
alongside `child.wait()`, and the call returns `OutputTooLarge`
distinctly. Propagate reader errors as an error variant instead
of an empty success. Decide whether `Timeout` should carry partial
output, and remove the dead plumbing either way.
