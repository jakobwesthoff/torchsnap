---
title: "testbed-windows lists no window while a bench-memory app runs"
kind: bug
component: project
impact: low
origin: investigation
tags: [linux, testing, tooling]
---
# testbed-windows lists no window while a bench-memory app runs

During the Linux memory investigation of 2026-10-24 on the testbed, `just testbed-windows` printed `[]` while `tools/bench-memory` had the app running (the investigation's per-run `windows.txt` files). The testbed lives on branch `linux-testbed` until it lands.

## Problem

`testbed-windows` asks GNOME Shell for the window actors whose WM class is `torchsnap` (`app_windows_js` in `tools/linux-testbed/pi/desktop-gnome.sh`); it does not depend on the testbed's app unit. Why it found none is not established. Two readings fit what is known and neither is checked: the launcher was hidden when the list was taken (a hidden window has no window actor), or the binary started through `bench-memory --app` gets a WM class other than `torchsnap`.

## Suggested fix

Reproduce first: start a bench run, then call `testbed-windows` once with the launcher shown and once hidden, and print every window actor's WM class. If the WM class differs, match the bench-started app too; if hidden windows are the cause, say in the recipe's help that it lists mapped windows only.
