---
title: "Measure malloc_trim after the Linux launcher hides, on top of the mapped shrink"
kind: investigation
component: platform
impact: low
origin: investigation
tags: [linux, memory]
---
# Measure malloc_trim after the Linux launcher hides, on top of the mapped shrink

Calling glibc's `malloc_trim(0)` in the UI process after the launcher hides may return freed heap pages to the system. The Linux memory review measured it once and found the UI process's anonymous memory 11.8 MiB lower at the final idle phase, the same sign in all three rounds, but below the 12 MiB detection floor (`docs/research/linux-launcher-memory.md`, table "No effect, below detection, or worse", row C7). The C5 measurement series (2026-10-24) planned to measure it again with more rounds on top of the mapped shrink (ADR 0074) and dropped that block when the data were enough for the shrink.

## How to measure

- On top of ADR 0074's mapped shrink, release build, Raspberry Pi testbed, launcher seen painted on screen.
- The whole scenario: `testbed-pi bench-memory --cycles 20 --timing-cycles 0` (15 phases plus 20 cycles), whose built-in screen check must end with "Screen check: launcher painted".
- Five rounds of: shrink, shrink plus trim, shrink. Verdict per round against the mean of the two shrink runs, on the decision figure (system Shmem plus the processes' anonymous PSS) and on the UI process's anonymous PSS: same sign in all five, outside every shrink run, mean at least 12 MiB. Null check: first against last shrink run per round.
- Log how long `malloc_trim` takes on the main thread.
- The C5 instrumentation (logbook of the Linux testbed work, `c5-shrink-signal-files/instrumentation.diff`, switch `C5_TRIM=1`) and the earlier L2 instrumentation (`L2_TRIM_ON_HIDE`) already had the call behind an environment switch.

## Done when

The result is in the research doc's levers section, and the trim is built (with a todo or ADR) or recorded as not worth it.
