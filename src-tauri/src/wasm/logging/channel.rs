// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Async Logging Channel
//
// Non-blocking log transport from producers to the logging
// task. Producers (plugin host imports, bridge, runtime) send
// log entries via `LogSender::send()` which never blocks —
// if the bounded channel is full, the entry is dropped and a
// counter is incremented.
//
// The logging task is a single async task that:
//   1. Receives entries from the mpsc channel
//   2. Assigns monotonic sequence numbers
//   3. Pushes into the ring buffer storage
//   4. Broadcasts to live subscribers (devtools console)
//
// Architecture:
//   LogSender (cloneable, any thread)
//     │ try_send (bounded mpsc)
//     ▼
//   Logging Task (single async task)
//     ├── seq assignment
//     ├── RingBufferStorage::push
//     └── broadcast::Sender::send
// =========================================================

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, mpsc};

use super::storage::{LogStorage, RingBufferStorage};
use super::{LogEntry, BROADCAST_CAPACITY, CHANNEL_CAPACITY, DEFAULT_RING_BUFFER_CAPACITY};

// =========================================================
// LogSender
// =========================================================

/// Cheaply cloneable handle for sending log entries without
/// blocking. Held by `PluginState`, `Logger`, and any host
/// code that needs to emit log entries.
///
/// If the bounded channel is full, `send()` drops the entry
/// and increments a counter. The frontend can query the
/// dropped count to display a warning.
#[derive(Clone)]
pub struct LogSender {
    tx: mpsc::Sender<LogEntry>,
    dropped: Arc<AtomicU64>,
}

impl LogSender {
    /// Send a log entry to the logging task. Never blocks.
    ///
    /// If the channel is full the entry is silently dropped
    /// and the dropped counter is incremented.
    pub fn send(&self, entry: LogEntry) {
        if self.tx.try_send(entry).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Number of entries dropped due to channel backpressure
    /// since the logging system was started.
    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

// =========================================================
// LoggingSystem
// =========================================================

/// Central logging coordinator. Created once during app
/// `setup()`, before any plugins are loaded.
///
/// Owns the ring buffer storage (behind a Mutex for Tauri
/// command access) and the broadcast sender for live
/// subscribers. The async logging task runs for the lifetime
/// of the app.
pub struct LoggingSystem {
    sender: LogSender,
    storage: Arc<Mutex<Box<dyn LogStorage>>>,
    broadcast_tx: broadcast::Sender<LogEntry>,
}

impl LoggingSystem {
    /// Start the logging system, spawning the background
    /// logging task onto the tokio runtime.
    ///
    /// The logging task runs until the sender side of the mpsc
    /// channel is dropped (i.e., all `LogSender` clones are
    /// gone), which happens naturally at app shutdown.
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel::<LogEntry>(CHANNEL_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        let storage: Arc<Mutex<Box<dyn LogStorage>>> = Arc::new(Mutex::new(Box::new(
            RingBufferStorage::new(DEFAULT_RING_BUFFER_CAPACITY),
        )));
        let (broadcast_tx, _) = broadcast::channel::<LogEntry>(BROADCAST_CAPACITY);

        let task_storage = Arc::clone(&storage);
        let task_broadcast = broadcast_tx.clone();

        tokio::spawn(logging_task(rx, task_storage, task_broadcast));

        Self {
            sender: LogSender { tx, dropped },
            storage,
            broadcast_tx,
        }
    }

    /// Get a cloneable sender for producing log entries.
    pub fn sender(&self) -> LogSender {
        self.sender.clone()
    }

    /// Subscribe to live log entries. Returns a broadcast
    /// receiver that yields each entry as it is stored.
    ///
    /// If the subscriber falls behind by more than
    /// `BROADCAST_CAPACITY` entries, it will receive
    /// `RecvError::Lagged(n)` and can catch up via
    /// `storage().entries_after()`.
    pub fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
        self.broadcast_tx.subscribe()
    }

    /// Access the underlying storage for queries (history,
    /// stats, clear). The caller must lock the mutex.
    pub fn storage(&self) -> &Arc<Mutex<Box<dyn LogStorage>>> {
        &self.storage
    }

    /// Number of entries dropped due to channel backpressure.
    pub fn dropped_count(&self) -> u64 {
        self.sender.dropped_count()
    }
}

// =========================================================
// Logging Task
// =========================================================

/// Background task that drains the mpsc channel, assigns
/// sequence numbers, stores entries, and broadcasts them.
async fn logging_task(
    mut rx: mpsc::Receiver<LogEntry>,
    storage: Arc<Mutex<Box<dyn LogStorage>>>,
    broadcast_tx: broadcast::Sender<LogEntry>,
) {
    let mut seq_counter: u64 = 0;

    while let Some(mut entry) = rx.recv().await {
        seq_counter += 1;
        entry.seq = seq_counter;

        // Store the entry in the ring buffer.
        {
            let mut store = storage.lock().expect("logging storage lock poisoned");
            store.push(entry.clone());
        }

        // Broadcast to live subscribers. Ignore errors — they
        // occur when no subscribers are connected, which is
        // normal (devtools window may not be open).
        let _ = broadcast_tx.send(entry);
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;
    use crate::wasm::logging::{LogLevel, LogSource};

    fn test_entry() -> LogEntry {
        LogEntry {
            seq: 0, // assigned by the logging task
            timestamp: SystemTime::now(),
            level: LogLevel::Info,
            source: LogSource::Host,
            message: "test message".to_string(),
            metadata: vec![],
            span_id: None,
            span: None,
        }
    }

    #[tokio::test]
    async fn send_and_receive() {
        let system = LoggingSystem::start();
        let sender = system.sender();

        sender.send(test_entry());
        sender.send(test_entry());

        // Give the logging task time to process.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let storage = system.storage().lock().expect("lock");
        assert_eq!(storage.len(), 2);

        let entries = storage.tail(10);
        assert_eq!(entries[0].seq, 1);
        assert_eq!(entries[1].seq, 2);
    }

    #[tokio::test]
    async fn broadcast_delivers_to_subscriber() {
        let system = LoggingSystem::start();
        let sender = system.sender();
        let mut subscriber = system.subscribe();

        sender.send(test_entry());

        let received = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            subscriber.recv(),
        )
        .await
        .expect("timeout waiting for broadcast")
        .expect("broadcast recv error");

        assert_eq!(received.seq, 1);
        assert_eq!(received.message, "test message");
    }

    #[tokio::test]
    async fn dropped_count_increments() {
        // Create a system with a tiny channel to force drops.
        let (tx, rx) = mpsc::channel::<LogEntry>(1);
        let dropped = Arc::new(AtomicU64::new(0));
        let storage: Arc<Mutex<Box<dyn LogStorage>>> =
            Arc::new(Mutex::new(Box::new(RingBufferStorage::new(100))));
        let (broadcast_tx, _) = broadcast::channel::<LogEntry>(16);

        // Don't spawn the logging task — entries stay in the
        // channel and it fills up immediately.
        let _rx = rx; // keep receiver alive

        let sender = LogSender {
            tx,
            dropped: Arc::clone(&dropped),
        };

        // Fill the channel (capacity = 1).
        sender.send(test_entry());
        // This one should be dropped.
        sender.send(test_entry());

        assert_eq!(sender.dropped_count(), 1);

        // Suppress unused variable warnings.
        let _ = storage;
        let _ = broadcast_tx;
    }
}
