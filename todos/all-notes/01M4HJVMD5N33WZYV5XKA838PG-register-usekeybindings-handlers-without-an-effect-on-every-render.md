---
title: "Register useKeyBindings handlers without an effect on every render"
kind: refactor
component: frontend
impact: low
origin: review
tags: [performance]
---
# Register useKeyBindings handlers without an effect on every render

Found in the second frontend memory leak audit (2026-03-28) of all frontend
source files. Severity in the audit: low. The first effect in `useKeyBindings`
has no dependency array and does comparison work after every render. It is not a
memory leak, only unnecessary per-render work.

## Problem

`src/keybindings/useKeyBindings.ts`, around lines 50-88

The first `useEffect` has no dependency array, so it executes after every
render. It clears and repopulates `handlersRef` and runs the `hasChanged`
structural comparison each time. When the comparison returns true (e.g.
`enabled` toggled, action list changed), it calls `cleanupRef.current()` and
`register()` to re-register.

This is intentional: the handler-ref indirection keeps registration stable while
handlers change. But the missing dep array means the comparison and ref update
run on every render of every component that uses `useKeyBindings`, regardless of
whether anything changed. Not a memory leak per se, but unnecessary per-render
work that scales with the number of active keybinding consumers.

## Suggested fix

Move the handler-ref update out of the effect (it's a render-time assignment,
not a side effect). Then add a dependency array that triggers re-registration
only when the definitions reference changes.
