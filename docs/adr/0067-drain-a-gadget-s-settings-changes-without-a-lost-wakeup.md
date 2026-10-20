# 67. Drain a gadget's settings changes without a lost wakeup

Date: 2026-10-21

## Status

Accepted

Amends [26. Use CoalescingDispatcher for serialized settings dispatch](0026-use-coalescing-dispatcher-for-serialized-settings-dispatch.md) (how a drain ends)

## Context

`handle_setting_changed` has two callers: the `settings-changed`
listener, which runs on a tokio worker, and the Retry of a failed
gadget enable (ADR 64), which runs on a `spawn_blocking` thread. Tauri
runs Rust listener callbacks one at a time, so only Retry runs at the
same time as another caller.

ADR 26's dispatcher held a `work` mutex for the whole drain and kept
the queue under a second mutex. A drain took an empty batch, released
the queue and only then released `work`. A caller that enqueued in
between found `work` held, returned, and left its change in the queue
with no drain to take it until the next change to that gadget. A Retry
could end that way without starting the gadget, and a setting changed
as a Retry finished could wait for the next change.

## Decision

The dispatcher keeps its queue and a `draining` flag under one mutex.
`dispatch()` returns at once when `draining` is set, as before.
Otherwise it sets the flag and drains; when it finds the queue empty,
it clears the flag in that same critical section, and nothing after
that point touches the flag.

A panicking callback is not caught: the flag stays set and the
dispatcher delivers nothing more to that gadget.

## Consequences

A caller can return before its change is applied, when another caller
is draining; that drain applies it. This includes Retry.
