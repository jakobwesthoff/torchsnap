---
title: "Shortcut problem note can miss a change between its first pull and its listener"
kind: bug
component: frontend
impact: low
origin: review
tags: [concurrency]
---
# Shortcut problem note can miss a change between its first pull and its listener

`useShortcutProblem` in `src/settings/ShortcutProblemNote.tsx` pulls `shortcut_problems` first and registers its `SHORTCUT_PROBLEMS_CHANGED` listener afterwards (lines 55-65). A change emitted after the pull was answered and before `listen` resolved is never pulled, so the note shows a stale problem or none until the next change. Noted on 2026-10-20 while planning the failed-gadget note, read in the code, not reproduced.

## Impact

Rare: the window is one IPC round trip at mount. The settings then show a wrong or missing shortcut problem until the next change or remount.

## Suggested fix

Register the listener first and pull once it resolved, as the failed-gadget status hook does. A test with a mocked `listen` that emits before resolving pins the order.
