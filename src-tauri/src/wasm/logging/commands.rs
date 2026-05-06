// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Tauri Commands for Developer Tools Console
//
// These commands expose the logging system to the devtools
// frontend. The frontend fetches history on mount, then
// subscribes to a live stream of new items via a Tauri
// channel.
//
// Commands:
//   devtools_log_history   — fetch items after a given seq
//   devtools_log_subscribe — start streaming live items
//   devtools_log_clear     — clear the ring buffer
//   devtools_log_stats     — current item count + dropped
// =========================================================

use std::sync::Arc;

use serde::Serialize;
use tauri::State;
use tauri::ipc::Channel;

use super::channel::LoggingSystem;
use super::spans::SpanRegistry;
use super::{LogItem, LogItemKind, LogLevel, LogSource};

// =========================================================
// Response Types
// =========================================================

/// Messages streamed to the devtools frontend over a Tauri
/// channel. Items are batched to reduce IPC overhead during
/// burst logging.
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DevToolsMessage {
    /// A batch of new log items.
    Entries { entries: Vec<LogItem> },
    /// Notification that items were dropped due to channel
    /// backpressure. The frontend can display a warning.
    Dropped { count: u64 },
}

/// Current logging statistics.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogStats {
    /// Number of items currently in the ring buffer.
    pub count: usize,
    /// Total items dropped due to channel backpressure
    /// since the logging system started.
    pub dropped: u64,
    /// Total items ever pushed (including evicted ones).
    pub total_pushed: u64,
}

// =========================================================
// Commands
// =========================================================

/// Fetch log items with `seq > after_seq`, up to `limit`.
///
/// Used by the frontend on initial mount to load existing
/// items before subscribing to live updates.
#[tauri::command]
pub fn devtools_log_history(
    after_seq: u64,
    limit: usize,
    state: State<'_, Arc<LoggingSystem>>,
) -> Vec<LogItem> {
    let storage = state
        .storage()
        .lock()
        .expect("logging storage not poisoned");
    storage.entries_after(after_seq, limit)
}

/// Subscribe to live log items via a Tauri channel.
///
/// Spawns a background task that reads from the broadcast
/// channel, batches items over a 16ms window, and sends
/// them to the frontend. Returns immediately.
///
/// The task runs until the Tauri channel is closed (e.g.,
/// when the devtools window is destroyed).
#[tauri::command]
pub fn devtools_log_subscribe(
    channel: Channel<DevToolsMessage>,
    state: State<'_, Arc<LoggingSystem>>,
) {
    let mut rx = state.subscribe();
    let storage = Arc::clone(state.storage());

    tauri::async_runtime::spawn(async move {
        loop {
            // Wait for the first item.
            let item = match rx.recv().await {
                Ok(item) => item,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    // We fell behind — notify the frontend and
                    // continue. The frontend can fetch missed
                    // items via devtools_log_history if needed.
                    if channel.send(DevToolsMessage::Dropped { count: n }).is_err() {
                        break;
                    }
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };

            // Batch additional items that arrived during the
            // 16ms window to reduce IPC overhead.
            let mut batch = vec![item];
            tokio::time::sleep(std::time::Duration::from_millis(16)).await;

            while let Ok(item) = rx.try_recv() {
                batch.push(item);
                // Cap batch size to avoid huge IPC messages.
                if batch.len() >= 500 {
                    break;
                }
            }

            if channel
                .send(DevToolsMessage::Entries { entries: batch })
                .is_err()
            {
                // Channel closed — frontend disconnected.
                break;
            }
        }

        // Clean up: nothing to do, the broadcast receiver is
        // dropped automatically.
        let _ = storage; // keep storage alive for potential catch-up queries
    });
}

/// Clear all items from the ring buffer.
#[tauri::command]
pub fn devtools_log_clear(state: State<'_, Arc<LoggingSystem>>) {
    let mut storage = state
        .storage()
        .lock()
        .expect("logging storage not poisoned");
    storage.clear();
}

/// Get current logging statistics.
#[tauri::command]
pub fn devtools_log_stats(state: State<'_, Arc<LoggingSystem>>) -> LogStats {
    let storage = state
        .storage()
        .lock()
        .expect("logging storage not poisoned");
    LogStats {
        count: storage.len(),
        dropped: state.dropped_count(),
        total_pushed: storage.total_pushed(),
    }
}

// =========================================================
// Frontend Logging Commands
//
// These commands allow frontend code (gadget React views,
// app UI) to emit log messages and spans into the same
// log stream used by the backend.
// =========================================================

/// Resolve a source string to a `LogSource`.
pub(crate) fn resolve_source(source: &str) -> LogSource {
    if source == "host" {
        LogSource::Host
    } else {
        LogSource::Gadget(source.to_string())
    }
}

/// Emit a log message from the frontend into the log stream.
#[tauri::command]
pub fn logger_emit(
    source: String,
    level: LogLevel,
    message: String,
    metadata: Vec<(String, String)>,
    span_id: Option<u64>,
    state: State<'_, Arc<LoggingSystem>>,
) {
    state.sender().send(LogItem {
        seq: 0,
        timestamp: std::time::SystemTime::now(),
        source: resolve_source(&source),
        kind: LogItemKind::Message {
            level,
            message,
            metadata,
            span_id,
        },
    });
}

/// Start a span from the frontend. Returns the backend span ID
/// so the frontend worker can map local IDs to backend IDs.
#[tauri::command]
pub fn logger_span_start(
    source: String,
    name: String,
    parent_id: Option<u64>,
    metadata: Vec<(String, String)>,
    state: State<'_, Arc<LoggingSystem>>,
    span_registry: State<'_, Arc<SpanRegistry>>,
) -> u64 {
    let log_source = resolve_source(&source);

    match span_registry.start(
        name.clone(),
        parent_id,
        log_source.clone(),
        metadata.clone(),
    ) {
        Some((id, depth)) => {
            state.sender().send(LogItem {
                seq: 0,
                timestamp: std::time::SystemTime::now(),
                source: log_source,
                kind: LogItemKind::SpanStart {
                    span_id: id,
                    name,
                    parent_id,
                    depth,
                    metadata,
                },
            });
            id
        }
        // Nesting depth exceeded — return 0 as a sentinel.
        None => 0,
    }
}

/// End a span from the frontend. The backend computes duration
/// and emits a `SpanEnd` item.
#[tauri::command]
pub fn logger_span_end(
    span_id: u64,
    metadata: Vec<(String, String)>,
    state: State<'_, Arc<LoggingSystem>>,
    span_registry: State<'_, Arc<SpanRegistry>>,
) {
    if let Some(completed) = span_registry.end(span_id, metadata) {
        state.sender().send(LogItem {
            seq: 0,
            timestamp: std::time::SystemTime::now(),
            source: completed.source.clone(),
            kind: completed.into(),
        });
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm::logging::channel::LoggingSystem;

    /// Helper: receive the next item from the broadcast channel.
    async fn recv(sub: &mut tokio::sync::broadcast::Receiver<LogItem>) -> LogItem {
        tokio::time::timeout(std::time::Duration::from_millis(100), sub.recv())
            .await
            .expect("timeout waiting for log item")
            .expect("broadcast recv error")
    }

    // ----- resolve_source -----

    #[test]
    fn resolve_source_host() {
        assert_eq!(resolve_source("host"), LogSource::Host);
    }

    #[test]
    fn resolve_source_plugin() {
        assert_eq!(
            resolve_source("hello-world"),
            LogSource::Gadget("hello-world".into()),
        );
    }

    #[test]
    fn resolve_source_empty_string_is_plugin() {
        // An empty string is not "host" — treated as a gadget
        // with an empty ID. Not a useful case, but the behavior
        // should be defined.
        assert_eq!(resolve_source(""), LogSource::Gadget("".into()),);
    }

    // ----- Frontend log emission (exercising the same code paths
    //       as the Tauri commands without the State wrapper) -----

    #[tokio::test]
    async fn emit_message_gadget_source() {
        let system = LoggingSystem::start();
        let mut sub = system.subscribe();

        system.sender().send(LogItem {
            seq: 0,
            timestamp: std::time::SystemTime::now(),
            source: resolve_source("my-plugin"),
            kind: LogItemKind::Message {
                level: LogLevel::Info,
                message: "hello from frontend".into(),
                metadata: vec![("key".into(), "val".into())],
                span_id: None,
            },
        });

        let item = recv(&mut sub).await;
        assert_eq!(item.source, LogSource::Gadget("my-plugin".into()));
        match &item.kind {
            LogItemKind::Message {
                level,
                message,
                metadata,
                span_id,
            } => {
                assert_eq!(*level, LogLevel::Info);
                assert_eq!(message, "hello from frontend");
                assert_eq!(*metadata, vec![("key".to_string(), "val".to_string())]);
                assert_eq!(*span_id, None);
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn emit_message_host_source() {
        let system = LoggingSystem::start();
        let mut sub = system.subscribe();

        system.sender().send(LogItem {
            seq: 0,
            timestamp: std::time::SystemTime::now(),
            source: resolve_source("host"),
            kind: LogItemKind::Message {
                level: LogLevel::Warn,
                message: "host warning".into(),
                metadata: vec![],
                span_id: None,
            },
        });

        let item = recv(&mut sub).await;
        assert_eq!(item.source, LogSource::Host);
    }

    #[tokio::test]
    async fn emit_message_with_span_association() {
        let system = LoggingSystem::start();
        let registry = SpanRegistry::new();
        let mut sub = system.subscribe();

        // Start a span to get a valid ID.
        let (span_id, _) = registry
            .start(
                "test-span".into(),
                None,
                LogSource::Gadget("p".into()),
                vec![],
            )
            .expect("span should start");

        system.sender().send(LogItem {
            seq: 0,
            timestamp: std::time::SystemTime::now(),
            source: resolve_source("p"),
            kind: LogItemKind::Message {
                level: LogLevel::Debug,
                message: "inside span".into(),
                metadata: vec![],
                span_id: Some(span_id),
            },
        });

        let item = recv(&mut sub).await;
        match &item.kind {
            LogItemKind::Message { span_id: sid, .. } => {
                assert_eq!(*sid, Some(span_id));
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    // ----- Frontend span lifecycle -----

    #[tokio::test]
    async fn span_start_emits_item_and_returns_id() {
        let system = LoggingSystem::start();
        let registry = SpanRegistry::new();
        let mut sub = system.subscribe();

        let source = resolve_source("my-plugin");
        let (id, depth) = registry
            .start(
                "frontend-span".into(),
                None,
                source.clone(),
                vec![("k".into(), "v".into())],
            )
            .expect("span should start");

        system.sender().send(LogItem {
            seq: 0,
            timestamp: std::time::SystemTime::now(),
            source: source.clone(),
            kind: LogItemKind::SpanStart {
                span_id: id,
                name: "frontend-span".into(),
                parent_id: None,
                depth,
                metadata: vec![("k".into(), "v".into())],
            },
        });

        assert!(id > 0);
        assert_eq!(depth, 0);

        let item = recv(&mut sub).await;
        match &item.kind {
            LogItemKind::SpanStart {
                span_id,
                name,
                parent_id,
                depth: d,
                metadata,
            } => {
                assert_eq!(*span_id, id);
                assert_eq!(name, "frontend-span");
                assert_eq!(*parent_id, None);
                assert_eq!(*d, 0);
                assert_eq!(*metadata, vec![("k".to_string(), "v".to_string())]);
            }
            other => panic!("expected SpanStart, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn span_end_emits_item_with_duration() {
        let system = LoggingSystem::start();
        let registry = SpanRegistry::new();
        let mut sub = system.subscribe();

        let (id, _) = registry
            .start(
                "timed-op".into(),
                None,
                LogSource::Gadget("p".into()),
                vec![],
            )
            .expect("span should start");

        // Small delay to ensure measurable duration.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        let completed = registry
            .end(id, vec![("result".into(), "ok".into())])
            .expect("span should end");

        system.sender().send(LogItem {
            seq: 0,
            timestamp: std::time::SystemTime::now(),
            source: completed.source.clone(),
            kind: completed.into(),
        });

        let item = recv(&mut sub).await;
        match &item.kind {
            LogItemKind::SpanEnd {
                span_id,
                name,
                duration_us,
                metadata,
                ..
            } => {
                assert_eq!(*span_id, id);
                assert_eq!(name, "timed-op");
                assert!(
                    *duration_us >= 4000,
                    "duration should be at least ~5ms ({duration_us}μs)"
                );
                let meta: std::collections::HashMap<String, String> =
                    metadata.clone().into_iter().collect();
                assert_eq!(meta.get("result").unwrap(), "ok");
            }
            other => panic!("expected SpanEnd, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn span_end_invalid_id_is_noop() {
        let registry = SpanRegistry::new();
        // Ending a span that was never started should return None.
        assert!(registry.end(99999, vec![]).is_none());
    }

    #[tokio::test]
    async fn nested_frontend_spans() {
        let system = LoggingSystem::start();
        let registry = SpanRegistry::new();
        let mut sub = system.subscribe();

        let source = resolve_source("p");

        let (parent_id, parent_depth) = registry
            .start("parent".into(), None, source.clone(), vec![])
            .expect("parent");
        assert_eq!(parent_depth, 0);

        let (child_id, child_depth) = registry
            .start("child".into(), Some(parent_id), source.clone(), vec![])
            .expect("child");
        assert_eq!(child_depth, 1);

        // Emit both span-start items.
        for (id, name, pid, depth) in [
            (parent_id, "parent", None, 0u32),
            (child_id, "child", Some(parent_id), 1u32),
        ] {
            system.sender().send(LogItem {
                seq: 0,
                timestamp: std::time::SystemTime::now(),
                source: source.clone(),
                kind: LogItemKind::SpanStart {
                    span_id: id,
                    name: name.into(),
                    parent_id: pid,
                    depth,
                    metadata: vec![],
                },
            });
        }

        // Verify parent start.
        let p_start = recv(&mut sub).await;
        match &p_start.kind {
            LogItemKind::SpanStart {
                depth, parent_id, ..
            } => {
                assert_eq!(*depth, 0);
                assert_eq!(*parent_id, None);
            }
            other => panic!("expected SpanStart, got {other:?}"),
        }

        // Verify child start.
        let c_start = recv(&mut sub).await;
        match &c_start.kind {
            LogItemKind::SpanStart {
                depth,
                parent_id: pid,
                ..
            } => {
                assert_eq!(*depth, 1);
                assert_eq!(*pid, Some(parent_id));
            }
            other => panic!("expected SpanStart, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn span_start_depth_exceeded_returns_zero() {
        let registry = SpanRegistry::new();
        let source = LogSource::Gadget("p".into());

        // Build a chain up to MAX_SPAN_NESTING.
        let mut current = None;
        for i in 0..super::super::MAX_SPAN_NESTING {
            let (id, _) = registry
                .start(format!("span-{i}"), current, source.clone(), vec![])
                .expect("should start");
            current = Some(id);
        }

        // The next one should fail.
        let result = registry.start("too-deep".into(), current, source, vec![]);
        assert!(result.is_none());
    }
}
