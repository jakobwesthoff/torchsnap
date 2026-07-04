# Devtools log subscription can churn on Lagged under gadget log floods

**Kind:** possible-bug
**Severity:** low
**Area:** src-tauri/src/wasm/logging/commands.rs

## Problem
`devtools_log_subscribe` reads one broadcast item, then sleeps a
fixed 16 ms to batch
(`src-tauri/src/wasm/logging/commands.rs:97-134`). The broadcast
channel capacity is `BROADCAST_CAPACITY = 256`
(`logging/mod.rs:70`). A gadget can emit log items far faster
than 256 per 16 ms (the mpsc side is 1024 and drops silently
past that, but the broadcast side lags at 256). Under such a
flood every loop iteration returns `RecvError::Lagged`, the
handler sends a `Dropped` message and `continue`s, and the
devtools console shows a stream of "dropped N" notices while the
actual entries have to be back-filled via `devtools_log_history`.

Separately, the batch drain caps at 500 items
(`:119-125`) but there is no cap on how large a single
`LogItem.message` or its metadata can be. A gadget logging
multi-megabyte strings pushes those verbatim through the ring
buffer, the broadcast channel, and the IPC boundary.

## Impact
A chatty or hostile gadget degrades the devtools experience
(constant lag notices) and can inflate host memory and IPC
payloads via oversized individual log entries. No crash;
diagnostics usefulness drops exactly when you need it.

## Suggested fix
Consider draining the broadcast receiver in a tight loop before
the 16 ms sleep (so a burst is captured in one batch rather than
triggering repeated Lagged), and cap per-item message/metadata
length at the `logging::log` host import boundary
(`runtime/host/logging.rs`) with a truncation marker. Relates to
the span-registry-bounds todo
`01kwfz4kkaq7spwnm2ncket1gn-span-registry-unbounded-unowned.md`.
