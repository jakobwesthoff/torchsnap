# CommandCap: stdin write runs before output readers and outside the timeout

**Kind:** possible-bug
**Severity:** medium
**Area:** src-tauri/src/caps/command.rs

## Problem
`run_child_with_caps` sequences the child I/O as: spawn → write
all stdin → spawn stdout/stderr reader tasks → `timeout(child.wait())`
(`src-tauri/src/caps/command.rs:302-328`):

```rust
if let Some(bytes) = stdin_bytes {
    if let Some(mut child_stdin) = child.stdin.take() {
        if let Err(e) = child_stdin.write_all(&bytes).await {
```

Two consequences:

1. **The stdin phase has no timeout.** `timeout_ms` only wraps
   `child.wait()` (`command.rs:327-328`). A child that neither
   reads stdin nor exits leaves `write_all` blocked forever once
   the OS pipe buffer (~64 KiB) is full. The whole guest call
   hangs indefinitely — and because WASM guest calls hold the
   instance's store lock, the gadget is wedged until app restart.
2. **Pipe deadlock for chatty children.** Output readers are
   spawned only *after* stdin is fully written
   (`command.rs:324-325`). A child that emits more than a pipe
   buffer of output before consuming all of a large stdin blocks
   writing stdout; the host blocks writing stdin; classic mutual
   pipe deadlock. Same unbounded hang as (1) since no timeout is
   active yet.

Also missing: `child_stdin` is dropped after `write_all`
(closing the pipe — good), but a child that exits early makes
`write_all` fail with EPIPE, which is handled
(`command.rs:308-311`), so only the no-read-no-exit and
big-IO-interleaving cases hang.

## Impact
A misbehaving (or merely slow-to-read) permitted binary can hang
a gadget permanently with no timeout rescue. The declared
timeout contract ("command timed out") silently does not cover
the stdin phase.

## Suggested fix
Start the stdout/stderr reader tasks immediately after spawn,
perform the stdin write as a third concurrent task, and wrap the
*entire* join (stdin done + child exit) in the single
`timeout_ms` envelope, killing the process group on elapse (the
kill path already exists: `kill_child_with_grace`,
`command.rs:408-442`).
