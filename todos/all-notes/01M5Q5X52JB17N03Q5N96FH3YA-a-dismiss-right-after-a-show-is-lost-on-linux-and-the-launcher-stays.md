---
title: "A dismiss right after a show is lost on Linux and the launcher stays open"
kind: bug
component: frontend
impact: low
origin: implementation
tags: [linux, concurrency]
---
# A dismiss right after a show is lost on Linux and the launcher stays open

On Linux a control API `dismiss` sent 50 ms after a `show` is lost: the launcher stays shown and never shrinks or hides. Found on 2026-10-24 on the Raspberry Pi testbed (GNOME 48 Wayland, release build with ADR 0074's mapped shrink), 5 of 5 times. The window maps only about 60 ms after the show request, so the dismiss arrives before it. A hotkey pressed twice quickly would take the same path.

## Problem

Not analysed yet. The guess is the page's `isHiding`/`isShowing` flags in `src/launcher/visibility.ts`: `launcher-shown` arrives after the dismiss and `restoreVisibility` clears `isHiding`, so `dismissLauncher` stops at one of its `if (!isHiding) return` steps and never calls `launcher_hide`. Not measured on a build without ADR 0074, so it may predate it.

## Suggested fix

First reproduce it on the build before ADR 0074. Then decide what a dismiss before the show has finished should do (most likely: the launcher ends hidden) and fix the ordering, with a frontend test that drives the two events in that order.
