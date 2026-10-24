---
title: "testbed-stop-all fails switching tray support back to its initial state"
kind: bug
component: project
impact: low
origin: investigation
tags: [linux, testing, tooling]
---
# testbed-stop-all fails switching tray support back to its initial state

At the end of the Linux memory review on 2026-10-24, `just testbed-stop-all` stopped every unit and then failed with "the AppIndicator extension did not become INACTIVE" while switching tray support back to the state recorded in `~/.cache/torchsnap-testbed/selftest-initial`. Afterwards `just testbed-tray status` reported `off`. The testbed lives on branch `linux-testbed` until it lands.

## Problem

Not reproduced or analysed. The session had rebooted the machine once earlier that night, and the extension may have been inactive already, so the wait for INACTIVE may not see the transition it expects; that is a guess.

## Suggested fix

Reproduce with the extension already off when `stop-all` runs, and treat "already in the recorded state" as success.
