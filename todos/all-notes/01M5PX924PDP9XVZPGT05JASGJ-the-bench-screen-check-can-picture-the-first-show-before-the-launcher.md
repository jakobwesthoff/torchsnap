---
title: "The bench screen check can picture the first show before the launcher has painted"
kind: bug
component: project
impact: low
origin: implementation
tags: [linux, testing, tooling]
---
# The bench screen check can picture the first show before the launcher has painted

`tools/bench-memory-cycles --screen-check` (on the `linux-testbed`
branch) reported "not painted (0 px)" for the first show of a freshly
started app on the Linux testbed, although the launcher paints: with a
hold of 1.5 s after the show, the same check reports painted. Seen by
the C5 measurement series on 2026-10-24.

## Problem

`screen_check` in `tools/bench-memory-cycles` takes the `shown` picture
as soon as the page reports `contentVisible`. On the first show after
the app starts, the screen can lag that report, so the `shown` picture
still shows the desktop, equals the `hidden` one and the check fails.
The bench's own check at the end of a run (the eleventh show) has not
shown this. The failure points the safe way: a run fails, no invalid
figure passes.

## Suggested fix

In the `shown` stage, take pictures until the area differs from one
taken before the show, with a timeout, instead of a single picture at
`contentVisible`. Test it with a fake screen whose first picture is the
desktop.
