---
title: "The Linux testbed should detect an active GNOME screen shield"
kind: bug
component: project
origin: implementation
tags: [linux, testing, tooling]
---
# The Linux testbed should detect an active GNOME screen shield

On 2026-10-23 GNOME's screen shield came up on the testbed by itself (org.gnome.ScreenSaver `GetActive` true from about 20:45) although provisioning turns lock and idle blanking off (`lock-enabled` false, `disable-lock-screen` true, `idle-delay` 0). While it was up, the testbed's GNOME Shell extension was inactive, so every recipe that uses Shell's Eval (`testbed-eval`, `testbed-windows`, `testbed-launcher-state`) failed. A pointer move (`just testbed-pointer`) cleared it. The testbed lives on branch `linux-testbed` until it lands.

## Problem

Nothing in the testbed checks for the shield: `session-ready` and the self-test assume an unlocked, unblanked session. The cause of the shield is not known; a guess is the self-test's undo item (U), which turns the idle and blanking settings back to their defaults for a moment.

## Suggested fix

Have `session-ready` (and the recipes that need Shell) check `org.gnome.ScreenSaver.GetActive` and wake the session with a pointer move, or fail with a clear message. Check whether item U leaves a window in which the shield can start, and close it.
