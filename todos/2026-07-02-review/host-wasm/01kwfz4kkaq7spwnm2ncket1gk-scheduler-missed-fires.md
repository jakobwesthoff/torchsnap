# Scheduled tasks missed during sleep are silently skipped

**Kind:** question
**Severity:** low
**Area:** src-tauri/src/wasm/bridge.rs

## Problem
The scheduler loop snapshots future fire times and sleeps until
the earliest (`src-tauri/src/wasm/bridge.rs:408-492`). After the
machine wakes from sleep (or the process was suspended), the
loop re-queries `sched.after(&now)`, which by construction
returns only *future* fires. Every fire whose time passed while
asleep is skipped without a log line. The module comment
(`:403-405`) frames the re-query as handling "clock jumps,
laptop sleep/wake, and DST naturally", which is true for
correctness of the *next* fire but leaves the missed-fire policy
implicit.

For the current workloads (retention cleanup) skip-and-wait is
probably right. For a task like "daily 04:00 data refresh" on a
laptop that is always asleep at 04:00, the task never runs at
all.

## Impact
Depends on future gadget task semantics; today only a
documentation gap. A gadget author cannot express "run when
possible if a fire was missed".

## Suggested fix
Decide and document the policy in the manifest docs ("fires
missed while the host was asleep are skipped"). If catch-up
semantics are ever wanted, an opt-in `catch-up = true` per task
that fires once on wake when the last-completed timestamp
predates the last scheduled fire would cover it (requires
persisting last-run times).
