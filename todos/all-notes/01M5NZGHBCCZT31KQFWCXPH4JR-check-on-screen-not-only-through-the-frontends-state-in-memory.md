---
title: "Check on screen, not only through the frontend's state, in memory benchmarks"
kind: improvement
component: project
origin: investigation
tags: [linux, testing, tooling, memory]
---
# Check on screen, not only through the frontend's state, in memory benchmarks

`tools/bench-memory`'s show-to-content time and the control `snapshot` report what the frontend believes, not what reaches the screen. In the Linux memory review of 2026-10-24 they looked normal for WebKit switches under which the launcher painted nothing, and those switches showed large memory savings only because nothing was drawn. A separate visual check caught it.

## Suggested fix

Have every benchmark run check pixels after a show: a screenshot of the launcher's area compared against the same area while hidden, or a GNOME Shell after-paint hook as the flicker check of 2026-10-23 used. Fail or flag the run when the launcher area did not change. On Linux the testbed's `testbed-screenshot --area` and `testbed-launcher-state` give what is needed; macOS needs its own way.
