---
title: "The useGadgetStream channel cannot be silenced after cleanup"
kind: bug
component: frontend
origin: review
tags: [memory]
---
# The useGadgetStream channel cannot be silenced after cleanup

Found in the second frontend memory leak audit (2026-03-28) of all frontend
source files. The audit excluded the four issues from the original memory leak
cleanup pass: three of those have been fixed, and the fourth (`useGadgetStream`)
is revisited here with additional context. Severity in the audit: medium
(design gap).

## Problem

`src/hooks/useGadgetStream.ts`, `src/lib/gadgetMessage.ts`

The frontend memory leak cleanup todo (closed in `eeaa41e4`) recommended nulling `channel.onmessage` in the cleanup return.
This is not feasible with the current API: `sendGadgetMessage` creates the
`Channel` internally (`src/lib/gadgetMessage.ts`, around line 26) and returns
only the invoke promise. The caller has no reference to the channel object and
therefore cannot silence it.

The `cancelled` flag prevents state updates but the closure (holding
`snapshotRef`, `listenersRef`) stays alive until the backend stops sending on
the channel.

## Suggested fix

Options:

1. Return the channel alongside the promise from `sendGadgetMessage` (e.g.
   `{ promise, channel }`), so the caller can null `onmessage` in cleanup.
2. Accept an `AbortSignal` parameter; when aborted, `sendGadgetMessage` nulls the
   channel's `onmessage` internally.
3. Expose a dedicated `subscribeGadgetStream` helper that returns a handle with
   an explicit `close()` / `silence()` method.

Option 2 is the most ergonomic for React effects (`AbortController` in setup,
`controller.abort()` in cleanup).
