---
title: "Decide whether scheduled gadget tasks catch up after sleep"
kind: investigation
component: gadget-host
status: needs-discussion
impact: low
---
# Decide whether scheduled gadget tasks catch up after sleep

Scheduled gadget tasks that fall due while the host sleeps never run, and the scheduler logs nothing for them. Skip-and-wait fits the current retention cleanup, but a daily refresh on a laptop that is asleep at its fire time never runs at all. The policy is not written down, so the user has to decide it.

## Context
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

## Options
Decide and document the policy in the manifest docs ("fires
missed while the host was asleep are skipped"). If catch-up
semantics are ever wanted, an opt-in `catch-up = true` per task
that fires once on wake when the last-completed timestamp
predates the last scheduled fire would cover it (requires
persisting last-run times).
