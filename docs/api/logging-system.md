# Logging System

Torchsnap's logging system is a single, unified pipeline for structured log
messages and timing spans produced by WASM plugins, host-side Rust code, and
the React frontend. There is one destination: an in-memory ring buffer that
broadcasts items live to the Developer Tools console window.

The system is intentionally narrow:

- No `tracing` / `tracing-subscriber` integration on the host.
- No log file on disk, no log rotation, no `stderr` writer.
- No browser-console fan-out from the frontend logger.

Everything that wants to be visible flows through the ring buffer and the
DevTools window. If the DevTools window is closed, items still accumulate
and can be inspected when it is opened.

## Contents

- [Architecture](#architecture)
- [Log Item Model](#log-item-model)
- [Plugin Authors (WASM Guest)](#plugin-authors-wasm-guest)
- [Host Developers (Rust)](#host-developers-rust)
- [Frontend Developers (React)](#frontend-developers-react)
- [Developer Tools Console](#developer-tools-console)
- [Tauri Commands](#tauri-commands)
- [Constants](#constants)

---

## Architecture

```
WASM Plugin                Host Rust                 Frontend (React)
  guest call                Logger / SpanGuard         Logger.info / .spanStart
       │                          │                          │
       ▼                          │                          ▼
PluginState::log / span_start ────┤                  log worker (singleton)
PluginState::span_end             │                  - allocates local span IDs
       │                          │                  - awaits backend ID
       │                          │                  - serializes IPC
       ▼                          ▼                          ▼
                 LogSender::send(LogItem)  ◄── logger_emit / logger_span_start /
                          │                    logger_span_end Tauri commands
                          ▼
              bounded mpsc channel (CHANNEL_CAPACITY = 1024)
                          │
                          ▼
               logging task (single async task)
               - assigns monotonic seq numbers
               - pushes into RingBufferStorage (10k items)
               - broadcasts to subscribers
                          │
                          ▼
              tokio::sync::broadcast (BROADCAST_CAPACITY = 256)
                          │
                          ▼
               devtools_log_subscribe (per DevTools window)
               - 16ms batch window, max 500 items per batch
               - emits DevToolsMessage::Entries / ::Dropped
                          │
                          ▼
                  Developer Tools window
```

All producers — WASM guest calls, host `Logger`, frontend `logger_*` Tauri
commands — converge on a single `LogSender` that performs `try_send` on a
bounded mpsc. If the channel is full, the item is dropped silently and a
counter is incremented. Logging never blocks the caller.

The host does not own a `tracing` subscriber. Native `tracing::info!` calls
do not appear in the DevTools console — instrumented host code uses the
`Logger` API directly.

Source files:

- `src-tauri/src/wasm/logging/mod.rs` — types and constants
- `src-tauri/src/wasm/logging/channel.rs` — `LogSender`, `LoggingSystem`,
  background task
- `src-tauri/src/wasm/logging/spans.rs` — `SpanRegistry`, `Logger`,
  `SpanBuilder`, `SpanGuard`
- `src-tauri/src/wasm/logging/storage.rs` — `LogStorage` trait and
  `RingBufferStorage`
- `src-tauri/src/wasm/logging/commands.rs` — Tauri commands
- `src-tauri/src/wasm/runtime/host/logging.rs` — WIT host-import bridge
- `plugins/plugin-sdk/src/logging.rs` — plugin SDK facade and macros
- `plugins/plugin-sdk/wit/torchsnap-plugin.wit` — WIT `logging` interface
- `src/lib/logger.ts`, `src/lib/logWorker.ts` — frontend `Logger` and worker

---

## Log Item Model

Every entry in the stream is a `LogItem` with a shared envelope and a
discriminated payload (`LogItemKind`).

Envelope:

| Field | Type | Notes |
|-------|------|-------|
| `seq` | `u64` | Assigned by the logging task, strictly increasing across the lifetime of the system. Producers send `0`. |
| `timestamp` | `SystemTime` | Wall-clock time, serialized as milliseconds since Unix epoch. |
| `source` | `LogSource` | `Plugin(<id>)` or `Host`. |
| `kind` | `LogItemKind` | One of `Message`, `SpanStart`, `SpanEnd`. |

Payloads:

- `Message` — `level`, `message`, `metadata: Vec<(String, String)>`,
  `span_id: Option<u64>` (associates the message with an active span).
- `SpanStart` — `span_id`, `name`, `parent_id`, `depth`, `metadata` (start
  metadata only, captured when the span opens).
- `SpanEnd` — `span_id`, `name`, `parent_id`, `depth`, `duration_us`,
  `metadata` (start- and end-metadata merged; end wins on key collision).

Levels are `Trace`, `Debug`, `Info`, `Warn`, `Error`.

### Span lifecycle

A span produces two items in the stream: one `SpanStart` when it opens and
one `SpanEnd` when it closes. The `SpanStart` carries the depth (computed
from walking the parent chain) so the frontend doesn't need to resolve it.
The `SpanEnd` carries the wall-clock duration and the merged metadata.

The `SpanRegistry` (a `Mutex<HashMap<u64, OpenSpan>>` plus an `AtomicU64`
ID generator) is shared via `Arc` between WASM hosts, the frontend command
handlers, and host-side `Logger` instances. WASM-, host-, and frontend-
originated spans coexist in the same registry and share the same ID space.

Maximum nesting depth is `MAX_SPAN_NESTING = 32`. A `span_start` that would
exceed the limit returns `None` on the host side and `0` to the WASM/frontend
caller. A subsequent `span_end` for that ID is a no-op (ID not found in the
registry).

---

## Plugin Authors (WASM Guest)

Plugins talk to the `logging` WIT interface (defined in
`plugins/plugin-sdk/wit/torchsnap-plugin.wit`):

```wit
interface logging {
  enum log-level { trace, debug, info, warn, error }
  type metadata-entry = tuple<string, string>;

  log: func(level: log-level, message: string,
            metadata: list<metadata-entry>, span: option<u64>);
  span-start: func(name: string, parent: option<u64>,
                   metadata: list<metadata-entry>) -> u64;
  span-end: func(span-id: u64, metadata: list<metadata-entry>);
}
```

The plugin SDK re-exports the generated bindings as `logging` (functions and
the `LogLevel` enum) and provides convenience macros for the common case.

### Macros (preferred for plain messages)

```rust
use torchsnap_plugin_sdk::prelude::*;

log_info!("Plugin initialized");
log_warn!("Network slow", "host" => "example.com", "rtt_ms" => 1234);
log_error!("Failed to load index", "error" => err);
log_debug!("Cache miss", "key" => key);
log_trace!("Tick", "n" => counter);
```

The `key => value` pairs are converted via `ToString`, so scalars
(`i32`, `bool`, `&str`, `String`) all work without explicit conversion.
The macros deliberately do not forward span handles — span-scoped logging
is opt-in via the direct API.

### Direct API (full WIT surface)

Use the direct call when you need a `span` handle, or when you already
have the metadata as a slice:

```rust
use torchsnap_plugin_sdk::prelude::*;

logging::log(
    logging::LogLevel::Info,
    "Search completed",
    &[
        ("query".into(), query.clone()),
        ("result_count".into(), results.len().to_string()),
    ],
    None,                  // no parent span
);
```

### Spans

```rust
let span = logging::span_start(
    "fuzzy-search",
    None,                                // parent
    &[("query".into(), query.clone())],  // start metadata
);

let results = do_fuzzy_search(&query);

logging::log(
    logging::LogLevel::Debug,
    "Scoring complete",
    &[],
    Some(span),  // associate this message with the span
);

logging::span_end(
    span,
    &[("result_count".into(), results.len().to_string())],
);
```

End-metadata is merged with start-metadata; on key collision, end wins.

Nesting is explicit — pass the parent handle:

```rust
let outer = logging::span_start("search", None, &[]);
let inner = logging::span_start("score", Some(outer), &[]);
logging::span_end(inner, &[]);
logging::span_end(outer, &[]);
```

There is no implicit "current span" stack. WASM guest execution is
single-threaded per instance, so the caller-managed handle is unambiguous
and avoids the cost of thread-local lookups.

If `span_start` would exceed the depth limit it returns `0`. Passing `0` to
`span_end` is a safe no-op.

---

## Host Developers (Rust)

Host code uses the `Logger` struct from `wasm::logging::spans`. It bundles a
`LogSender`, a shared `Arc<SpanRegistry>`, and a `LogSource` so callers don't
have to thread these through manually.

```rust
use crate::wasm::logging::{LogLevel, LogSource};
use crate::wasm::logging::spans::Logger;

let logger = Logger::new(
    logging_system.sender(),
    Arc::clone(&span_registry),
    LogSource::Host,                  // or LogSource::Plugin("id".into())
);
```

`Logger` is `Clone` — clone freely into subsystems. The runtime constructs
a per-plugin `Logger` with `LogSource::Plugin(<id>)` for use by host-side
code that operates on behalf of that plugin (compilation, instantiation,
bridge errors).

### Messages

```rust
logger.log(LogLevel::Info, "Plugin loaded");

logger.log_with_meta(
    LogLevel::Debug,
    "Cache miss",
    vec![("key".into(), "favicon-example.com".into())],
);

logger.log_in_span(LogLevel::Debug, "Scoring candidates", span_id);
```

### Spans (RAII)

`SpanGuard` ensures the matching `SpanEnd` is always emitted, even on early
return or panic. `start()` returns `Option<SpanGuard>` — `None` only when
the nesting limit is exceeded.

```rust
let span = logger.span("compile")
    .meta("plugin_id", plugin_id)
    .start()
    .expect("compile span within nesting limit");

let bytes = compile_component(source);

// Consume the guard to attach end-metadata. Without this call the guard
// ends with empty end-metadata on drop.
span.end_with_meta(vec![
    ("artifact_size".into(), bytes.len().to_string()),
]);
```

`SpanGuard::child()` produces a `SpanBuilder` that auto-parents:

```rust
let load = logger.span("load").start();

if let Some(load) = load.as_ref() {
    let _compile = load.child("compile").start();
    compile_component(source);
    // _compile drops here, emitting SpanEnd
}

// load drops at end of scope, encompassing the child's duration
```

`SpanGuard::log(level, message)` emits a message that is automatically
associated with that span's ID:

```rust
let span = logger.span("search").start().expect("nesting");
span.log(LogLevel::Debug, "Phase 1: candidate generation");
// ...
span.log(LogLevel::Debug, "Phase 2: scoring");
```

### Direct LogSender

For low-level call sites that don't have a `Logger` (the WIT host-import
implementation in `wasm::runtime::host::logging` and the bridge-level error
log in `wasm::bridge`), construct `LogItem` values directly:

```rust
log_sender.send(LogItem {
    seq: 0,                          // assigned by the logging task
    timestamp: SystemTime::now(),
    source: LogSource::Plugin(plugin_id.clone()),
    kind: LogItemKind::Message {
        level: LogLevel::Info,
        message: "...".into(),
        metadata: vec![],
        span_id: None,
    },
});
```

Prefer `Logger` for everything else.

### WIT host-import bridge

`PluginState` carries `log_sender: LogSender` and
`span_registry: Arc<SpanRegistry>` as foundational fields set at instance
construction. The `bindings::torchsnap::plugin::logging::Host` impl in
`wasm/runtime/host/logging.rs` translates the WIT enum into the host
`LogLevel`, tags every item with `LogSource::Plugin(<plugin_id>)`, and
forwards to the registry / sender. `span_start` emits the `SpanStart` item
and returns the registry-assigned ID; `span_end` returns silently if the ID
is unknown (already ended, or a sentinel `0` from a depth-rejected start).

---

## Frontend Developers (React)

Frontend code uses the `Logger` class from `src/lib/logger.ts`. Every method
is fire-and-forget — calls return synchronously while a singleton worker
drains an internal queue and performs the Tauri IPC.

### Acquiring a logger

Plugin views receive a pre-bound logger via props:

```typescript
function ClipboardView({ logger, sendMessage, ...props }: PluginViewProps) {
  logger.info("Clipboard view mounted");
}
```

Deeper components in a plugin tree can use the `useLogger()` hook —
plugin views are wrapped in `<LoggerProvider source={pluginId}>` by the
host:

```typescript
import { useLogger } from "../hooks/useLogger";

function DeepChild() {
  const logger = useLogger();
  logger.debug("Rendering deep child");
}
```

Non-plugin (host) UI code creates its own logger directly:

```typescript
import { createLogger } from "../lib/logger";
const logger = createLogger("host");
```

The string `"host"` is the only special source value — `commands::resolve_source`
maps it to `LogSource::Host`. Any other string is wrapped as
`LogSource::Plugin(<string>)`.

### Messages and spans

```typescript
logger.info("Search completed", [
  ["query", query],
  ["resultCount", String(results.length)],
]);

const span = logger.spanStart("fuzzy-search", undefined, [["query", query]]);
const results = await doSearch(query);
logger.spanEnd(span, [["resultCount", String(results.length)]]);

// Nesting: pass the parent's local ID
const inner = logger.spanStart("score", span);
logger.spanEnd(inner);
```

`logger.info(msg, metadata, spanId)` accepts a span ID for span-scoped
messages.

### How the worker maps span IDs

`spanStart()` must return synchronously, but the backend ID is allocated
asynchronously. The worker (`src/lib/logWorker.ts`) solves this with a
local-to-backend mapping:

1. `spanStart()` allocates a local ID from a frontend counter and returns it
   to the caller immediately.
2. The worker enqueues a `spanStart` command. When it fires, it `await`s
   `logger_span_start` and stores the returned backend ID in
   `localToBackendId: Map<number, number>`.
3. Subsequent `message` and `spanEnd` commands referencing the local ID are
   resolved to the backend ID before the IPC. Commands are processed in the
   order they were enqueued, so by the time a child or message references a
   parent, the parent's mapping is established.
4. If `logger_span_start` returns `0` (depth exceeded), no mapping is stored
   — subsequent `spanEnd` calls for that local ID are silently dropped.

Worker errors are caught and logged via `console.warn`; the logging system
is diagnostic and must not crash the UI.

---

## Developer Tools Console

The DevTools window is opened from the system tray menu. It shows a live,
filterable, virtualized log of every `LogItem` in the ring buffer in two
view modes:

- **Flat view** — chronological stream with depth-based indentation.
- **Tree view** — spans are collapsible nodes that group their child
  messages and nested spans.

Span rows show the duration (or an in-progress indicator) and the
parent/child relationships derived from `parent_id` and `depth`.

### Live streaming

1. On mount, the frontend calls `devtools_log_history(0, 10000)` to load
   existing items from the ring buffer.
2. It then calls `devtools_log_subscribe(channel)` to start receiving live
   updates.
3. The backend spawns a per-window task that reads from the broadcast
   channel, batches items over a 16ms window (one frame, capped at 500
   items per batch), and sends them as `DevToolsMessage::Entries`.
4. If the subscriber falls behind by more than `BROADCAST_CAPACITY` items,
   `RecvError::Lagged(n)` is converted into `DevToolsMessage::Dropped { n }`
   and the frontend can fetch missed items via `devtools_log_history`.
5. When the DevTools window is destroyed the channel send fails and the
   task exits.

---

## Tauri Commands

Registered in `src-tauri/src/lib.rs`:

| Command | Parameters | Returns | Purpose |
|---------|-----------|---------|---------|
| `devtools_log_history` | `after_seq: u64, limit: usize` | `Vec<LogItem>` | Initial load for DevTools |
| `devtools_log_subscribe` | `channel: Channel<DevToolsMessage>` | — | Begin live streaming |
| `devtools_log_clear` | — | — | Clear the ring buffer |
| `devtools_log_stats` | — | `LogStats` | `count`, `dropped`, `total_pushed` |
| `logger_emit` | `source, level, message, metadata, spanId` | — | Frontend message emission |
| `logger_span_start` | `source, name, parentId, metadata` | `u64` | Frontend span start; `0` on depth-limit reject |
| `logger_span_end` | `spanId, metadata` | — | Frontend span end |

Frontend code should not call `logger_*` directly — use the `Logger` class
so span ID allocation, ordering, and source binding are handled correctly.

---

## Constants

Defined in `src-tauri/src/wasm/logging/mod.rs`:

| Constant | Default | Effect |
|----------|---------|--------|
| `DEFAULT_RING_BUFFER_CAPACITY` | 10,000 | Items retained before eviction |
| `MAX_SPAN_NESTING` | 32 | Maximum parent-chain depth for spans |
| `CHANNEL_CAPACITY` | 1,024 | Bounded mpsc between producers and the logging task |
| `BROADCAST_CAPACITY` | 256 | Live broadcast channel; subscribers exceeding this lag |

Each `LogItem` is roughly 200–500 bytes. If `dropped` is consistently
non-zero in `LogStats`, increase `CHANNEL_CAPACITY`. If the DevTools console
needs longer scrollback, increase `DEFAULT_RING_BUFFER_CAPACITY`.
