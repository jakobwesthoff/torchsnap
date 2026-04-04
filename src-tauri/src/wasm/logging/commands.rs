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
use tauri::ipc::Channel;
use tauri::State;

use super::channel::LoggingSystem;
use super::LogItem;

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
    let storage = state.storage().lock().expect("logging storage not poisoned");
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
    let mut storage = state.storage().lock().expect("logging storage not poisoned");
    storage.clear();
}

/// Get current logging statistics.
#[tauri::command]
pub fn devtools_log_stats(state: State<'_, Arc<LoggingSystem>>) -> LogStats {
    let storage = state.storage().lock().expect("logging storage not poisoned");
    LogStats {
        count: storage.len(),
        dropped: state.dropped_count(),
        total_pushed: storage.total_pushed(),
    }
}
