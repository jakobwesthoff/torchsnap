// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WASM Gadget Logging
//
// Structured logging infrastructure for WASM gadgets.
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

pub mod channel;
pub mod commands;
pub mod spans;
pub mod storage;

use std::time::SystemTime;

use serde::{Deserialize, Serialize, Serializer};

/// Serialize `SystemTime` as milliseconds since Unix epoch
/// so the frontend can use `new Date(ms)` directly.
fn serialize_timestamp_ms<S: Serializer>(
    time: &SystemTime,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let ms = duration.as_millis() as u64;
    serializer.serialize_u64(ms)
}

// =========================================================
// Constants
// =========================================================

/// Maximum number of log items retained in the ring buffer.
pub const DEFAULT_RING_BUFFER_CAPACITY: usize = 10_000;

/// Maximum span nesting depth. `span_start` rejects spans
/// whose parent chain exceeds this limit.
pub const MAX_SPAN_NESTING: usize = 32;

/// Bounded channel capacity between log producers and the
/// logging task. Sized for burst absorption without excessive
/// memory use (~200–500 bytes per item).
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Where a log item originated.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum LogSource {
    /// From a WASM gadget via the logging WIT import.
    Gadget(String),
    /// From host-side code (runtime, bridge, loader, etc.).
    Host,
}

// =========================================================
// LogItem — the universal log stream element
// =========================================================

/// A single item in the log stream. Shared envelope fields
/// plus a discriminated payload.
///
/// There are three kinds of items:
/// - **Messages**: regular log entries with level and text.
/// - **Span-start**: marks the beginning of a timed region.
/// - **Span-end**: marks the completion with duration and
///   merged metadata.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogItem {
    /// Monotonically increasing sequence number assigned by
    /// the logging task. Used for deduplication on reconnect
    /// and as stable keys in the frontend virtualizer.
    ///
    /// Set to 0 by producers; the logging task assigns the
    /// real value before storing.
    pub seq: u64,

    /// Wall-clock timestamp as milliseconds since Unix epoch.
    /// Serialized as a number for direct use with `new Date(ms)`
    /// on the frontend.
    #[serde(serialize_with = "serialize_timestamp_ms")]
    pub timestamp: SystemTime,

    /// Who produced this item.
    pub source: LogSource,

    /// The payload — message, span-start, or span-end.
    pub kind: LogItemKind,
}

/// Discriminated payload for log items.
///
/// Uses `serde(tag = "type")` so the frontend sees a nested
/// object with a `type` discriminator:
/// `{ type: "message" | "spanStart" | "spanEnd", ... }`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LogItemKind {
    /// A regular log message.
    #[serde(rename_all = "camelCase")]
    Message {
        level: LogLevel,
        message: String,
        metadata: Vec<(String, String)>,
        /// Associates this message with an active span.
        span_id: Option<u64>,
    },

    /// A span has started. Emitted when `span_start` is called
    /// so the frontend can track open spans in real time.
    #[serde(rename_all = "camelCase")]
    SpanStart {
        span_id: u64,
        name: String,
        parent_id: Option<u64>,
        /// Nesting depth: 0 for root spans, 1 for children of
        /// root, etc. Provided so the frontend doesn't need to
        /// walk parent chains.
        depth: u32,
        metadata: Vec<(String, String)>,
    },

    /// A span has ended. Carries the wall-clock duration and
    /// merged metadata (start + end, end wins on key collision).
    #[serde(rename_all = "camelCase")]
    SpanEnd {
        span_id: u64,
        name: String,
        parent_id: Option<u64>,
        depth: u32,
        duration_us: u64,
        /// Merged start + end metadata (end wins on key collision).
        metadata: Vec<(String, String)>,
    },
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ----- LogLevel serde -----

    #[test]
    fn log_level_serializes_as_camel_case() {
        assert_eq!(
            serde_json::to_string(&LogLevel::Trace).unwrap(),
            "\"trace\""
        );
        assert_eq!(
            serde_json::to_string(&LogLevel::Debug).unwrap(),
            "\"debug\""
        );
        assert_eq!(serde_json::to_string(&LogLevel::Info).unwrap(), "\"info\"");
        assert_eq!(serde_json::to_string(&LogLevel::Warn).unwrap(), "\"warn\"");
        assert_eq!(
            serde_json::to_string(&LogLevel::Error).unwrap(),
            "\"error\""
        );
    }

    #[test]
    fn log_level_deserializes_from_camel_case() {
        assert_eq!(
            serde_json::from_str::<LogLevel>("\"trace\"").unwrap(),
            LogLevel::Trace
        );
        assert_eq!(
            serde_json::from_str::<LogLevel>("\"debug\"").unwrap(),
            LogLevel::Debug
        );
        assert_eq!(
            serde_json::from_str::<LogLevel>("\"info\"").unwrap(),
            LogLevel::Info
        );
        assert_eq!(
            serde_json::from_str::<LogLevel>("\"warn\"").unwrap(),
            LogLevel::Warn
        );
        assert_eq!(
            serde_json::from_str::<LogLevel>("\"error\"").unwrap(),
            LogLevel::Error
        );
    }

    #[test]
    fn log_level_rejects_invalid_value() {
        assert!(serde_json::from_str::<LogLevel>("\"fatal\"").is_err());
        assert!(serde_json::from_str::<LogLevel>("\"INFO\"").is_err());
        assert!(serde_json::from_str::<LogLevel>("42").is_err());
    }

    #[test]
    fn log_level_roundtrip() {
        for level in [
            LogLevel::Trace,
            LogLevel::Debug,
            LogLevel::Info,
            LogLevel::Warn,
            LogLevel::Error,
        ] {
            let json = serde_json::to_string(&level).unwrap();
            let back: LogLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(back, level);
        }
    }

    // ----- LogSource serde -----

    #[test]
    fn log_source_plugin_serialization() {
        let source = LogSource::Gadget("hello-world".into());
        let json = serde_json::to_value(&source).unwrap();
        assert_eq!(json["type"], "gadget");
        assert_eq!(json["value"], "hello-world");
    }

    #[test]
    fn log_source_host_serialization() {
        let source = LogSource::Host;
        let json = serde_json::to_value(&source).unwrap();
        assert_eq!(json["type"], "host");
    }

    // ----- LogItemKind serde (JSON shape must match TypeScript types) -----

    #[test]
    fn message_kind_json_shape() {
        let kind = LogItemKind::Message {
            level: LogLevel::Info,
            message: "hello".into(),
            metadata: vec![("k".into(), "v".into())],
            span_id: Some(42),
        };
        let json = serde_json::to_value(&kind).unwrap();

        assert_eq!(json["type"], "message");
        assert_eq!(json["level"], "info");
        assert_eq!(json["message"], "hello");
        assert_eq!(json["spanId"], 42);
        assert_eq!(json["metadata"][0][0], "k");
        assert_eq!(json["metadata"][0][1], "v");
    }

    #[test]
    fn message_kind_null_span_id() {
        let kind = LogItemKind::Message {
            level: LogLevel::Debug,
            message: "test".into(),
            metadata: vec![],
            span_id: None,
        };
        let json = serde_json::to_value(&kind).unwrap();
        assert!(json["spanId"].is_null());
    }

    #[test]
    fn span_start_kind_json_shape() {
        let kind = LogItemKind::SpanStart {
            span_id: 1,
            name: "search".into(),
            parent_id: None,
            depth: 0,
            metadata: vec![("query".into(), "foo".into())],
        };
        let json = serde_json::to_value(&kind).unwrap();

        assert_eq!(json["type"], "spanStart");
        assert_eq!(json["spanId"], 1);
        assert_eq!(json["name"], "search");
        assert!(json["parentId"].is_null());
        assert_eq!(json["depth"], 0);
        assert_eq!(json["metadata"][0][0], "query");
    }

    #[test]
    fn span_start_kind_with_parent() {
        let kind = LogItemKind::SpanStart {
            span_id: 5,
            name: "score".into(),
            parent_id: Some(1),
            depth: 1,
            metadata: vec![],
        };
        let json = serde_json::to_value(&kind).unwrap();

        assert_eq!(json["parentId"], 1);
        assert_eq!(json["depth"], 1);
    }

    #[test]
    fn span_end_kind_json_shape() {
        let kind = LogItemKind::SpanEnd {
            span_id: 1,
            name: "search".into(),
            parent_id: None,
            depth: 0,
            duration_us: 12345,
            metadata: vec![("count".into(), "42".into())],
        };
        let json = serde_json::to_value(&kind).unwrap();

        assert_eq!(json["type"], "spanEnd");
        assert_eq!(json["spanId"], 1);
        assert_eq!(json["name"], "search");
        assert!(json["parentId"].is_null());
        assert_eq!(json["depth"], 0);
        assert_eq!(json["durationUs"], 12345);
        assert_eq!(json["metadata"][0][0], "count");
    }

    // ----- Full LogItem serde -----

    #[test]
    fn log_item_json_has_nested_kind() {
        let item = LogItem {
            seq: 7,
            timestamp: SystemTime::UNIX_EPOCH,
            source: LogSource::Gadget("test".into()),
            kind: LogItemKind::Message {
                level: LogLevel::Error,
                message: "fail".into(),
                metadata: vec![],
                span_id: None,
            },
        };
        let json = serde_json::to_value(&item).unwrap();

        // Envelope fields.
        assert_eq!(json["seq"], 7);
        assert_eq!(json["timestamp"], 0); // epoch = 0ms
        assert_eq!(json["source"]["type"], "gadget");
        assert_eq!(json["source"]["value"], "test");

        // Kind is a nested object with discriminator.
        assert_eq!(json["kind"]["type"], "message");
        assert_eq!(json["kind"]["level"], "error");
        assert_eq!(json["kind"]["message"], "fail");
    }
}
