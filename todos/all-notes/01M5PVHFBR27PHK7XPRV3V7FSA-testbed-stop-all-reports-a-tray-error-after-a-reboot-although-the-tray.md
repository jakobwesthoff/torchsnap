---
title: "testbed stop-all reports a tray error after a reboot although the tray is off"
kind: bug
component: project
impact: low
origin: implementation
tags: [linux, testing, tooling]
---
# testbed stop-all reports a tray error after a reboot although the tray is off

After the Pi reboots, `just testbed-stop-all` prints "the AppIndicator
extension did not become INACTIVE" even though the tray is off. Seen on
2026-10-24 after self-test item R (reboot).

## Problem

`desk_tray_switch off` in `tools/linux-testbed/pi/testbed-pi` waits for
the AppIndicator extension's state to become `INACTIVE`. After a reboot
an extension that was never enabled in that boot reports `INITIALIZED`,
which is also off, so the wait times out and the recipe reports an
error.

## Suggested fix

Accept `INITIALIZED` as off in `desk_tray_switch`, with a test that
feeds it that state.
