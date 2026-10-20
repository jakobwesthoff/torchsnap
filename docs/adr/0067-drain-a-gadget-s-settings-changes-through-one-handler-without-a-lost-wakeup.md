# 67. Drain a gadget's settings changes through one handler without a lost wakeup

Date: 2026-10-21

## Status

Accepted

Amends [26. Use CoalescingDispatcher for serialized settings dispatch](0026-use-coalescing-dispatcher-for-serialized-settings-dispatch.md) (how a drain ends, and which callback drains a slot)

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
with no drain to take it until the next change to that gadget.

Each gadget slot had one dispatcher, and the host passed it one
callback for `enabled.<id>` changes and another for `gadgets.<id>.*`
changes. Whichever caller drained ran its own callback over the other
kind's changes too: a setting change made while a Retry started the
gadget was skipped, and a Retry that came while a setting change was
being delivered reached the gadget as a setting named `enabled.<id>`
and did not start it.

## Decision

The dispatcher keeps its queue and a `draining` flag under one mutex.
`dispatch()` returns at once when `draining` is set, as before.
Otherwise it sets the flag and drains; when it finds the queue empty,
it clears the flag in that same critical section, and nothing after
that point touches the flag.

The queue key of a gadget slot names the kind of change: the switch,
or one of the gadget's settings by its relative key. Every dispatch of
a slot runs the slot's one handler, which applies a switch change and
then signals the shortcut reactor, or delivers a setting change to the
gadget. The slot holds the host's enable failures and the reactor's
sender itself, so the handler is the same whichever caller drains.

A panicking callback is not caught: the flag stays set and the
dispatcher delivers nothing more to that gadget.

## Consequences

A caller can return before its change is applied, when another caller
is draining; that drain applies it. This includes Retry.

Changes submitted through a slot, by the listener and by Retry, never
overlap each other and keep the order in which they were enqueued. The
startup `enable()` and `disable_all()` at exit do not go through the
slot.
