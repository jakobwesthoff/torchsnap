---
kind: bug
severity: medium
status: open
area: [src/devtools/console/useLogStream.ts]
---

# Devtools log stream permanently misses items between history fetch and subscribe

## Problem
`useLogStream` connects in two sequential steps (`src/devtools/console/useLogStream.ts:161-192`):

```ts
async function connect() {
  // 1. Fetch existing items.
  try {
    const history = await command("devtools_log_history", {
      afterSeq: 0,
      limit: 10_000,
    });
    if (cancelled) return;
    store.setHistory(history);
  } catch (e) { ... }

  // 2. Subscribe to live items.
  const channel = new Channel<DevToolsMessage>();
  ...
  await command("devtools_log_subscribe", { channel });
```

On the Rust side, `devtools_log_subscribe` creates its broadcast
receiver at command-invocation time with no replay
(`src-tauri/src/wasm/logging/commands.rs:94`: `let mut rx = state.subscribe();`).
There is no seq-based handoff: any item pushed between the moment the
history snapshot was taken and the moment the broadcast receiver is
registered is in neither the history response nor the live stream. The
frontend never calls `devtools_log_history` again with the last-seen
`seq` to close the gap.

Two related inconsistencies in the same file:

1. On backpressure the backend sends `Dropped { count }` and its
   comment says "The frontend can fetch missed items via
   devtools_log_history if needed" (`commands.rs:103-106`). The
   frontend only increments a counter
   (`useLogStream.ts:181-183`) and never re-fetches, even though the
   dropped items are usually still in the 10k ring buffer
   (`DEFAULT_RING_BUFFER_CAPACITY`, `src-tauri/src/wasm/logging/mod.rs:55`)
   and each `LogItem` carries a `seq` for exactly this catch-up.
2. The module header comment (`useLogStream.ts:11`) claims it
   "Merges them into a single array keyed by seq number", but no
   seq-keyed merge exists — `appendEntries`
   (`useLogStream.ts:90-94`) blindly concatenates. The comment
   describes deduplication/merging logic that was never written.

## Impact
Items logged during the window between the history snapshot and the
broadcast subscription silently never appear in the console. The
window spans one IPC round-trip plus React commit time; during gadget
startup bursts (a common time to open devtools) this can swallow a
batch of messages with no indication. Separately, on broadcast lag
the user sees "N messages dropped" although the items are still
recoverable from the ring buffer.

## Suggested fix
Track the highest `seq` seen. Subscribe *first*, buffer incoming live
batches, then fetch history and merge by `seq` (dropping duplicates),
which makes the header comment true. On `dropped` messages, issue a
`devtools_log_history({ afterSeq: lastSeq })` catch-up fetch and merge
by seq instead of only counting.
