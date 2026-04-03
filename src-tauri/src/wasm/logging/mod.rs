// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WASM Plugin Logging
//
// Structured logging infrastructure for WASM plugins.
// Provides a non-blocking async channel, ring buffer storage,
// span-based timing, and Tauri commands for the Developer
// Tools console.
//
// Architecture:
//   LogSender (cloneable, non-blocking)
//    └── mpsc channel → Logging Task
//                         ├── assigns seq numbers
//                         ├── pushes into RingBufferStorage
//                         └── broadcasts to subscribers
//
//   SpanRegistry (shared, thread-safe)
//    └── tracks open spans for duration computation
//
//   Logger (per-subsystem handle)
//    └── bundles LogSender + SpanRegistry + LogSource
//        └── SpanGuard (RAII span lifecycle)
// =========================================================

pub mod storage;

use std::time::SystemTime;

use serde::Serialize;

// =========================================================
// Constants
// =========================================================

/// Maximum number of log entries retained in the ring buffer.
pub const DEFAULT_RING_BUFFER_CAPACITY: usize = 10_000;

/// Maximum span nesting depth. `span_start` rejects spans
/// whose parent chain exceeds this limit.
pub const MAX_SPAN_NESTING: usize = 32;

/// Bounded channel capacity between log producers and the
/// logging task. Sized for burst absorption without excessive
/// memory use (~200–500 bytes per entry).
pub const CHANNEL_CAPACITY: usize = 1024;

/// Broadcast channel capacity for live subscribers (e.g., the
/// Developer Tools console). Subscribers that fall behind
/// receive `RecvError::Lagged` and can catch up via
/// `entries_after` on the storage.
pub const BROADCAST_CAPACITY: usize = 256;

// =========================================================
// Core Types
// =========================================================

/// Log severity level, matching the WIT `log-level` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Where a log entry originated.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum LogSource {
    /// From a WASM plugin via the logging WIT import.
    Plugin(String),
    /// From host-side code (runtime, bridge, loader, etc.).
    Host,
}

/// Completed span record, emitted when `span_end` is called.
///
/// Metadata is the merge of start-metadata and end-metadata,
/// with end-metadata winning on key collisions.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpanInfo {
    /// Unique span identifier.
    pub span_id: u64,
    /// Parent span, if this span was nested inside another.
    pub parent_id: Option<u64>,
    /// Human-readable span name (e.g., "search", "execute").
    pub name: String,
    /// Wall-clock duration in microseconds.
    pub duration_us: u64,
    /// Merged key-value metadata from span start and end.
    pub metadata: Vec<(String, String)>,
}

/// A single log entry in the ring buffer.
///
/// There are two kinds of entries:
/// - **Regular log messages**: `span` is `None`, `span_id` may
///   optionally associate the message with an active span.
/// - **Span-end timing entries**: `span` carries the completed
///   `SpanInfo` with duration and merged metadata.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    /// Monotonically increasing sequence number assigned by
    /// the logging task. Used for deduplication on reconnect
    /// and as stable keys in the frontend virtualizer.
    ///
    /// Set to 0 by producers; the logging task assigns the
    /// real value before storing.
    pub seq: u64,
    /// Wall-clock timestamp for display.
    pub timestamp: SystemTime,
    /// Severity level.
    pub level: LogLevel,
    /// Who produced this entry.
    pub source: LogSource,
    /// Human-readable log message.
    pub message: String,
    /// Structured key-value metadata attached to this entry.
    pub metadata: Vec<(String, String)>,
    /// If set, associates this log message with an active span.
    pub span_id: Option<u64>,
    /// Populated only on span-end entries: carries completed
    /// timing and merged metadata.
    pub span: Option<SpanInfo>,
}
