---
title: "Find out why a 50 ms wait before the Linux hide leaves less memory than 500 ms"
kind: investigation
component: platform
impact: low
horizon: someday
origin: investigation
tags: [linux, memory]
---
# Find out why a 50 ms wait before the Linux hide leaves less memory than 500 ms

In the C5 measurement series (2026-10-24, Raspberry Pi 5, GNOME 48 Wayland, 4K at scale 2, three runs per arm), a 50 ms wait between the mapped shrink and the hide ended 60 s after a dismiss with a decision figure (system Shmem plus the processes' anonymous PSS) of 568.7 MiB, below the range of the 500 ms wait (576.1 to 587.6, mean 582.1). The arm that waited for GDK frame timings (in effect a 300 ms wait) ended at 574.8. The UI process's GPU memory at 60 s differed: 32.9 MiB with 50 ms, 36.1 with the frame-timing arm, 42.8 with 500 ms, ranges overlapping. Three runs per arm do not explain it.

ADR 0074 waits 150 ms (587.4 MiB at 60 s). If a shorter wait reliably leaves less memory, the choice of the wait would change; 50 ms missed the release in 1 of 20 dismisses, though.

## Open questions

- Is the difference real with more runs, or noise of three runs?
- Which buffers does the UI process keep after a longer wait, and why?

Data: logbook of the Linux testbed work, `plans/c5-shrink-signal-files/block-b-summary.md` and the `B-*` run directories.

## Revisit when

The wait in ADR 0074 is revisited, or the UI process's GPU memory becomes a target.
