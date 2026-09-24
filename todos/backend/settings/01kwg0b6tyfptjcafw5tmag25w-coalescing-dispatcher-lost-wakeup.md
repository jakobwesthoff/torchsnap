---
kind: bug
severity: medium
status: open
area: [src-tauri/src/settings/coalescing_dispatcher.rs]
tags: [unconfirmed]
---

# CoalescingDispatcher can strand the last enqueued change

## Problem
`dispatch()` uses `try_lock` on the `work` mutex and returns
immediately when another dispatch is running
(`src-tauri/src/settings/coalescing_dispatcher.rs:77-98`),
relying on the active loop to pick up newly enqueued items. The
loop exits when it observes an empty queue:

```rust
let batch = { ... std::mem::take(&mut *pending) };
if batch.is_empty() { break; }
```

There is a lost-wakeup window between that empty check and the
release of the `work` guard:

1. Thread A (active dispatcher) drains `pending`, sees empty,
   `break`s — but still holds `work` until the function returns.
2. Thread B enqueues `("k", v)` (only touches `pending`).
3. Thread B calls `dispatch()`; `try_lock` fails because A still
   holds `work`; B returns, assuming A will process the item.
4. A returns and drops `work` without ever re-checking
   `pending`.

The change for `k` now sits in the queue and is only delivered
when some *future* settings event triggers another
`dispatch()`. If `k` was the last change of a burst, the gadget
never observes it until unrelated activity occurs — for a
settings-driven gadget this looks like "sometimes the last
toggle doesn't apply".

The three-step interleaving needs two threads racing within
microseconds, so it will be rare and unreproducible in practice,
which is the worst kind of bug to field-debug.

## Impact
Occasional silently-delayed (potentially indefinitely-delayed)
`on_setting_changed` delivery to gadgets.

## Suggested fix
After `break`, re-check `pending` once more while still holding
`work`, looping back if non-empty (check-then-release must be
atomic with respect to enqueue+try_lock). Simplest correct
shape: swap the `try_lock` fast-exit for a small state machine
(e.g. an `AtomicBool` "rerun" flag set by enqueue when dispatch
is active, checked by the loop before exiting), or accept
blocking `lock()` since callbacks are already serialized on a
`spawn_blocking` thread. A loom-style interleaving test is
overkill; a targeted unit test with two threads and a barrier
can pin the fix.
