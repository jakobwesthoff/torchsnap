---
title: "Hide the Linux launcher on a WebKit draw signal instead of a fixed wait"
kind: improvement
component: platform
impact: low
horizon: someday
origin: investigation
tags: [linux, memory, performance]
---
# Hide the Linux launcher on a WebKit draw signal instead of a fixed wait

On Linux the launcher shrinks to 1x1 while mapped and hides a fixed 150 ms later (ADR 0074, `MAPPED_SHRINK_WAIT` in `src-tauri/src/launcher_hide.rs`), so the window buffers are released before the hide. A signal from GTK or WebKit could hide it sooner. The C5 measurement series (2026-10-24, Raspberry Pi 5, GNOME 48 Wayland, 4K at scale 2) found one candidate.

## Findings

- The webview's second `draw` at the small allocation ("S3b") comes a median 14 ms after the web process releases its buffers. As the hide signal on its own it passed the release test in 19 of 20 dismisses; the failure was a web release 95 ms after the shrink that S3b preceded by 2.6 ms.
- Over 200 dismisses (500 ms wait, S3b logged), no web or UI release was measured later than 29.8 ms after S3b. Those release times are lower bounds: the sampler stamped its rows before slow reads, so a release could be dated too early, never too late.
- `configure-event`, the webview's `size-allocate` and its first draw at the small size come before the release (0 of 20). GDK frame timings for the S3b frame never completed (300 ms timeout every time). A Wayland frame callback requested at the allocation comes before the buffer swaps (0 of 20). Driving it from the page (resize, two animation frames) passed 3 and 16 of 20.
- Dismiss to hidden, median from content visible: 42 ms today, 204 ms with the 150 ms wait, 83 ms with S3b alone. S3b plus 30 ms would land near 113 ms.

## Proposal

Hide 30 ms after S3b, with the fixed wait as the upper bound. Needs a hook on the webview's draw signal at the small size, a timeout fallback, and the 200-dismiss release test on the real code. The data and tools: logbook of the Linux testbed work, `plans/c5-shrink-signal-report.md` and `plans/c5-shrink-signal-files/`.

## Revisit when

The 150 ms longer dismiss on Linux is noticed, or the hide path changes for another reason.
