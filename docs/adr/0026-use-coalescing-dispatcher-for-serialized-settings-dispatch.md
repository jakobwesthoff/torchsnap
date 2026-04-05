# 26. Use CoalescingDispatcher for serialized settings dispatch

Date: 2026-04-05

## Status

Accepted

## Context

When the host routes settings changes to plugins via `setting_changed()`,
rapid changes — a slider being dragged, for example — can produce many events
for the same key in quick succession. Without deduplication, each intermediate
value triggers a full callback. Additionally, if a `setting_changed()` callback
is slow, concurrent changes could pile up and execute out of order or
overwhelm the plugin.

## Decision

Introduce a `CoalescingDispatcher` per plugin slot (see ADR 0025). It uses
two mutexes:

- `work: Mutex<()>` — serializes dispatch execution so only one dispatch loop
  runs at a time.
- `pending: Mutex<Vec<(String, Value)>>` — accumulates pending changes.

`enqueue(key, value)` performs a retain-and-push: it removes any existing
entry with the same key and appends the new one at the end, preserving
chronological order while deduplicating same-key changes.

`dispatch(callback)` uses `try_lock` on the work mutex. If the lock is already
held, it returns immediately — the active dispatch loop will pick up any newly
enqueued items. The dispatch loop drains the pending queue in a tight loop
until it is empty, ensuring that items enqueued during a callback invocation
are always picked up in the next iteration.

## Consequences

- Rapid same-key changes coalesce to the latest value — plugins never see
  intermediate slider positions during a drag gesture.
- Different-key changes preserve order — a plugin sees `(retentionDays, 30)`
  then `(historyEnabled, false)` in the order they were enqueued.
- No thread spawning per change — dispatch runs inline on the
  settings-changed listener thread.
- If dispatch is already running when new changes arrive, those changes are
  picked up by the existing loop rather than starting a parallel execution,
  preventing event storms under heavy settings churn.
