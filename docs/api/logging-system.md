# Logging System

The Torchsnap logging system provides structured, non-blocking logging and
timing spans for WASM plugins and host-side code. All log entries flow into an
in-memory ring buffer and stream live to the Developer Tools console window.

This guide covers both perspectives:

- **Plugin authors** who want to emit logs and measure performance from WASM
  guest code via the WIT logging interface.
- **Host developers** who work on the Rust backend and need to instrument
  runtime operations, add new subsystems, or extend the logging infrastructure.

## Contents

- [Overview](#overview)
- [For Plugin Authors](#for-plugin-authors)
  - [Logging Messages](#logging-messages)
  - [Structured Metadata](#structured-metadata)
  - [Timing with Spans](#timing-with-spans)
  - [Nested Spans](#nested-spans)
  - [Complete Plugin Example](#complete-plugin-example)
- [For Host Developers](#for-host-developers)
  - [Getting a Logger](#getting-a-logger)
  - [Logging Messages (Host)](#logging-messages-host)
  - [Spans with RAII Guards](#spans-with-raii-guards)
  - [Nested Spans (Host)](#nested-spans-host)
  - [Using LogSender Directly](#using-logsender-directly)
- [Architecture](#architecture)
  - [Data Flow](#data-flow)
  - [Core Types](#core-types)
  - [Non-Blocking Guarantees](#non-blocking-guarantees)
  - [Ring Buffer and Eviction](#ring-buffer-and-eviction)
  - [Span Registry](#span-registry)
- [Developer Tools Console](#developer-tools-console)
  - [Tauri Commands](#tauri-commands)
  - [Live Streaming Protocol](#live-streaming-protocol)
- [Constants and Tuning](#constants-and-tuning)

---

## Overview

```
WASM Plugin (guest)                Host Code (Rust)
  │                                  │
  │ logging::log(...)                │ logger.log(...)
  │ logging::span_start(...)         │ logger.span("name").start()
  │ logging::span_end(...)           │   // SpanGuard auto-ends on drop
  │                                  │
  └──────────────┬───────────────────┘
                 │
                 ▼
           LogSender::send()          ◄── non-blocking, never panics
                 │
                 ▼ bounded mpsc channel (1024 entries)
                 │
           Logging Task (async, background)
                 ├── assigns monotonic seq numbers
                 ├── stores in RingBufferStorage (10k entries)
                 └── broadcasts to live subscribers
                          │
                          ▼
                 Developer Tools Console
```

Every log entry — whether from a plugin or the host — follows the same path:
into a `LogSender`, through a bounded channel, into the ring buffer, and out
to any connected Developer Tools windows.

---

## For Plugin Authors

The logging interface is available to every WASM plugin as a host import. The
`wit_bindgen` macro generates Rust wrappers that you call as free functions
in the `torchsnap::plugin::logging` module.

### Logging Messages

The simplest usage — emit a log message with a severity level:

```rust
use torchsnap::plugin::logging;

logging::log(
    logging::LogLevel::Info,
    "Plugin initialized successfully",
    &[],    // no metadata
    None,   // no span association
);
```

**Log levels**, from least to most severe:

| Level | Use for |
|-------|---------|
| `Trace` | Very fine-grained diagnostic output (usually disabled in the console) |
| `Debug` | Internal state useful during development |
| `Info` | Normal operational events worth noting |
| `Warn` | Unexpected conditions that don't prevent operation |
| `Error` | Failures that affect functionality |

### Structured Metadata

Attach key-value pairs to any log message for structured filtering and
inspection in the Developer Tools console:

```rust
logging::log(
    logging::LogLevel::Info,
    "Search completed",
    &[
        ("query".into(), query.clone()),
        ("result_count".into(), results.len().to_string()),
    ],
    None,
);
```

Metadata appears in the console as expandable `key=value` pairs below the
message when you click a log entry row.

> **Note:** Metadata keys and values are `String` in the WIT-generated
> bindings. Use `.into()` or `.to_string()` to convert from `&str` or numeric
> types.

### Timing with Spans

Spans measure the wall-clock duration of an operation. Start a span, do work,
end the span — the host records how long it took.

```rust
// Start a span — returns an opaque handle (u64).
let span = logging::span_start("fuzzy-search", None, &[]);

// ... do the expensive work ...

// End the span — the host computes the duration and emits
// a timing entry to the Developer Tools console.
logging::span_end(span, &[]);
```

In the console, span entries appear with a "SPAN" label and a color-coded
duration badge (green for fast, amber for medium, red for slow).

#### Attaching Metadata to Spans

Spans accept metadata at both start and end. This is useful for recording
inputs when the span opens and outputs when it closes:

```rust
let span = logging::span_start(
    "fuzzy-search",
    None,
    &[("query".into(), query.clone())],  // start metadata: inputs
);

let results = do_fuzzy_search(&query);

logging::span_end(
    span,
    &[("result_count".into(), results.len().to_string())],  // end metadata: outputs
);
```

If both start and end metadata contain the same key, the end value wins. The
merged metadata is visible in the console when you expand the span entry.

### Nested Spans

Spans can be nested by passing a parent handle. This creates a parent-child
relationship visible in the Developer Tools console:

```rust
let outer = logging::span_start("search", None, &[]);

// The inner span references the outer span as its parent.
let inner = logging::span_start("score-candidates", Some(outer), &[]);
// ... scoring work ...
logging::span_end(inner, &[]);

logging::span_end(outer, &[]);
```

Nesting is explicit — you control the parent by passing the handle. There is
no implicit "current span" stack. This keeps the API simple and predictable
in single-threaded WASM execution.

The maximum nesting depth is 32. Spans exceeding this limit are silently
skipped (the `span_start` call returns 0, and the corresponding `span_end`
is a no-op).

### Associating Log Messages with Spans

Pass a span handle as the last argument to `log()` to associate a message
with an active span:

```rust
let span = logging::span_start("enable", None, &[]);

logging::log(
    logging::LogLevel::Info,
    "Loading 50,000 petnames into memory",
    &[],
    Some(span),  // this message belongs to the "enable" span
);

// ... do work ...

logging::span_end(span, &[]);
```

### Complete Plugin Example

This example from the `hello-world` plugin demonstrates logging, spans, and
metadata together:

```rust
use torchsnap::plugin::logging;

fn enable() {
    let span = logging::span_start("enable", None, &[]);

    let names = generate_petnames(50_000);

    logging::log(
        logging::LogLevel::Info,
        &format!("Plugin enabled with {} petnames", names.len()),
        &[],
        Some(span),
    );

    PETNAMES.with(|cell| *cell.borrow_mut() = names);

    logging::span_end(span, &[("petname_count".into(), "50000".into())]);
}

fn search(query: String) -> SearchResponse {
    let span = logging::span_start(
        "fuzzy-search",
        None,
        &[("query".into(), query.clone())],
    );

    let results = fuzzy_search(&query);
    let count = results.len();

    logging::span_end(
        span,
        &[("result_count".into(), count.to_string())],
    );

    if results.is_empty() {
        SearchResponse::Nothing
    } else {
        SearchResponse::Results(results)
    }
}

fn execute(entry_id: String, _action_id: ActionId) -> Result<PostAction, String> {
    logging::log(
        logging::LogLevel::Info,
        &format!("Executed: {entry_id}"),
        &[("entry_id".into(), entry_id.clone())],
        None,
    );
    Ok(PostAction::Dismiss)
}
```

---

## For Host Developers

Host-side code uses the `Logger` struct, which provides an ergonomic Rust API
with RAII-based span management. You never construct `LogEntry` values
directly — `Logger` handles that.

### Getting a Logger

A `Logger` is created from a `LogSender`, a shared `SpanRegistry`, and a
`LogSource` that identifies who is logging:

```rust
use crate::wasm::logging::spans::Logger;
use crate::wasm::logging::LogSource;

// Typically obtained from the LoggingSystem during setup:
let logger = Logger::new(
    logging_system.sender(),
    Arc::clone(&span_registry),
    LogSource::Plugin("my-plugin".into()),  // or LogSource::Host
);
```

In practice, loggers are created during initialization and passed to
subsystems. The `WasmRuntime` creates a per-plugin logger for each plugin
instance. Host subsystems can create their own with `LogSource::Host`.

`Logger` is `Clone` — clone it freely to pass into different code paths.

### Logging Messages (Host)

```rust
// Simple message.
logger.log(LogLevel::Info, "Plugin loaded successfully");

// Message with structured metadata.
logger.log_with_meta(
    LogLevel::Debug,
    "Cache miss",
    vec![("key".into(), "favicon-example.com".into())],
);

// Message associated with a span (by ID).
logger.log_in_span(LogLevel::Debug, "Scoring candidates", span_id);
```

### Spans with RAII Guards

The `SpanGuard` pattern ensures spans are always properly ended, even when
the code returns early or panics:

```rust
// Start a span — returns Option<SpanGuard>.
// Returns None only if nesting depth exceeds 32 (very unlikely).
let _span = logger.span("search")
    .meta("query", &query)
    .start();

// ... do work ...

// The span ends automatically when `_span` is dropped.
// Duration is measured from start() to drop.
```

To attach end-metadata, consume the guard explicitly:

```rust
let span = logger.span("compile")
    .meta("plugin_id", plugin_id)
    .start()
    .expect("span within nesting limit");

let result = compile_component(bytes);

span.end_with_meta(vec![
    ("artifact_size".into(), result.len().to_string()),
]);
// Guard is consumed — no double-end on drop.
```

### Nested Spans (Host)

Use `child()` on a `SpanGuard` to create a nested span that auto-parents:

```rust
let load_span = logger.span("load")
    .meta("plugin_id", plugin_id)
    .start();

// child() auto-sets parent_id to load_span's ID.
{
    let _compile = load_span.as_ref()
        .and_then(|s| s.child("compile").start());
    compile_component(bytes);
}
// compile span ended on drop here.

{
    let _instantiate = load_span.as_ref()
        .and_then(|s| s.child("instantiate").start());
    instantiate_component(&component);
}
// instantiate span ended on drop here.

drop(load_span);
// load span ended — its duration encompasses both children.
```

### Logging Within a Span Guard

The guard has a `log()` method that automatically associates messages with its
span:

```rust
let span = logger.span("search").start().expect("span");

span.log(LogLevel::Debug, "Starting candidate scoring");
// ... scoring work ...
span.log(LogLevel::Debug, "Scoring complete");

// span ends on drop
```

### Using LogSender Directly

For low-level use cases (e.g., inside a `PluginState` host import where you
don't have a `Logger`), you can send raw `LogEntry` values:

```rust
log_sender.send(LogEntry {
    seq: 0,  // always 0 — the logging task assigns the real value
    timestamp: SystemTime::now(),
    level: LogLevel::Info,
    source: LogSource::Plugin(plugin_id.clone()),
    message: "Something happened".into(),
    metadata: vec![],
    span_id: None,
    span: None,
});
```

This is what the WIT host import implementations do internally. Prefer
`Logger` when possible — it's less error-prone.

---

## Architecture

### Data Flow

1. **Producers** (plugins via WIT imports, host code via `Logger`) create
   `LogEntry` values and hand them to a `LogSender`.

2. **LogSender** uses `try_send` on a bounded mpsc channel. If the channel is
   full, the entry is silently dropped and a counter is incremented. This
   guarantees that logging never blocks plugin execution.

3. **The logging task** (a single async task spawned at startup) receives
   entries, assigns monotonically increasing sequence numbers, stores them in
   the ring buffer, and broadcasts them to any live subscribers.

4. **The ring buffer** (`RingBufferStorage`) holds up to 10,000 entries. When
   full, the oldest entry is evicted on each push.

5. **Subscribers** (Developer Tools windows) receive entries via a
   `tokio::sync::broadcast` channel. If a subscriber falls behind, it receives
   a `Lagged(n)` error and can catch up via the `entries_after` query on the
   storage.

### Core Types

| Type | Purpose |
|------|---------|
| `LogEntry` | Universal container for all log data — messages and span-end timing entries |
| `LogLevel` | Severity: Trace, Debug, Info, Warn, Error |
| `LogSource` | Origin: `Plugin("name")` or `Host` |
| `SpanInfo` | Completed span data: ID, parent, name, duration, merged metadata |
| `LogSender` | Cloneable, non-blocking entry producer |
| `LoggingSystem` | Central coordinator: owns storage, broadcast, sender |
| `SpanRegistry` | Thread-safe open-span tracker: start → ID, end → SpanInfo |
| `Logger` | Per-subsystem handle bundling sender + registry + source |
| `SpanGuard` | RAII span lifecycle — ends the span on drop |

### Non-Blocking Guarantees

The logging system is designed to never impact plugin latency:

- `LogSender::send()` uses `try_send()` — it returns immediately whether or
  not the channel has capacity. No mutex, no allocation on the hot path
  (beyond the `LogEntry` construction itself).
- If the channel is full, the entry is dropped and `dropped_count` is
  incremented. The Developer Tools console displays a warning when drops occur.
- The `SpanRegistry` uses a `Mutex<HashMap>`, but contention is minimal:
  plugins run serialized per-instance (the wasmtime `Store` mutex), and
  span start/end are fast operations.

### Ring Buffer and Eviction

The `RingBufferStorage` uses a `VecDeque` with a fixed capacity (default
10,000 entries). When full, `push` evicts the oldest entry via `pop_front`.

The `entries_after(after_seq, limit)` method uses binary search on the
monotonically increasing `seq` field for O(log n) seek time. This is the
primary query used by the frontend when reconnecting after a lag.

`total_pushed` is a lifetime counter that is not reset by `clear()`. It
enables the frontend to detect and report evicted entries.

### Span Registry

The `SpanRegistry` is a thread-safe `HashMap<u64, OpenSpan>` behind a mutex,
with an `AtomicU64` for ID generation.

- **Start**: generates a unique ID, validates nesting depth by walking the
  parent chain (capped at 32), records `Instant::now()` for duration
  measurement.
- **End**: removes the open span, computes `duration_us` from the elapsed
  `Instant`, merges start and end metadata (end wins on key collision),
  returns the completed `SpanInfo`.

The registry is shared between all plugins and host code via `Arc`. Plugin
spans and host spans coexist in the same registry, enabling the Developer
Tools console to show a unified view.

---

## Developer Tools Console

The Developer Tools window is opened from the system tray menu ("Developer
Tools..."). It displays a live, filterable, virtualized log of all entries.

### Tauri Commands

Four commands expose the logging system to the frontend:

| Command | Parameters | Returns | Purpose |
|---------|-----------|---------|---------|
| `devtools_log_history` | `after_seq: u64, limit: usize` | `Vec<LogEntry>` | Fetch entries for initial load |
| `devtools_log_subscribe` | `channel: Channel<DevToolsMessage>` | — | Start live streaming |
| `devtools_log_clear` | — | — | Clear the ring buffer |
| `devtools_log_stats` | — | `LogStats` | Entry count, dropped count, total pushed |

### Live Streaming Protocol

1. The frontend calls `devtools_log_history(0, 10000)` on mount to load
   existing entries.
2. It then calls `devtools_log_subscribe(channel)` to start receiving live
   updates.
3. The backend spawns a task that reads from the broadcast channel, batches
   entries over a 16ms window (one frame), and sends them as
   `DevToolsMessage::Entries { entries }`.
4. If the subscriber falls behind, it receives
   `DevToolsMessage::Dropped { count }` and the frontend displays a warning
   banner.
5. Batches are capped at 500 entries to prevent oversized IPC messages.
6. When the frontend disconnects (window closed), the channel send fails and
   the task exits.

---

## Constants and Tuning

All constants are defined in `src-tauri/src/wasm/logging/mod.rs` and can be
adjusted as needed:

| Constant | Default | Purpose |
|----------|---------|---------|
| `DEFAULT_RING_BUFFER_CAPACITY` | 10,000 | Max entries before eviction |
| `MAX_SPAN_NESTING` | 32 | Max parent-chain depth |
| `CHANNEL_CAPACITY` | 1,024 | Bounded mpsc between producers and logging task |
| `BROADCAST_CAPACITY` | 256 | Broadcast channel for live subscribers |

The current defaults are sized for interactive debugging. If profiling shows
that the channel is frequently full (high `dropped_count`), increase
`CHANNEL_CAPACITY`. If the console needs more scrollback history, increase
`DEFAULT_RING_BUFFER_CAPACITY` (each entry is roughly 200–500 bytes in
memory).
