---
title: "Launcher activate-gadget-custom-ui listener doubles under StrictMode"
kind: bug
component: frontend
impact: low
origin: review
tags: [memory, concurrency]
---
# Launcher activate-gadget-custom-ui listener doubles under StrictMode

Found in the second frontend memory leak audit (2026-03-28) of all frontend
source files. Severity in the audit: negligible. In development under React
StrictMode, the `activate-gadget-custom-ui` listener in the launcher can briefly
exist twice. Production is not affected.

## Problem

`src/launcher/Launcher.tsx`, around lines 117-130

Same async unlisten pattern as `useWindowLifecycle`. The effect has `[]` deps so
it runs once in production, so there is no leak. Under React StrictMode (dev
only), effects mount, unmount and remount synchronously. The async gap between
`listen()` returning a promise and the cleanup `.then()` resolving means both
the old and new listeners can be briefly active at the same time during the
StrictMode double-mount cycle. This can cause duplicate `activate-gadget-custom-ui`
handling in development.

Not a production concern. Mentioned for completeness; fix alongside the
`useWindowLifecycle` pattern if that gets addressed.

## Suggested fix

Apply the same `await`-first pattern as in the `useWindowLifecycle` todo: await
the `listen` promise, and unlisten immediately if the effect went stale before
it resolved.

## Relations

- Relates to: [Make useWindowLifecycle unlisten safe across effect re-runs](01M4HJVBNY3ACFGAY2MPYQNEBA-make-usewindowlifecycle-unlisten-safe-across-effect-re-runs.md), same async unlisten pattern, fix together
