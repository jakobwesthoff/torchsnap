// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Async Logging Channel
//
// Non-blocking log transport from producers to the logging
// task. Producers (gadget host imports, bridge, runtime) send
// log items via `LogSender::send()` which never blocks —
// if the bounded channel is full, the item is dropped and a
// counter is incremented.
//
// The logging task is a single async task that:
//   1. Receives items from the mpsc channel
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

use super::spans::SpanRegistry;
use super::storage::{LogStorage, RingBufferStorage};
use super::{BROADCAST_CAPACITY, CHANNEL_CAPACITY, DEFAULT_RING_BUFFER_CAPACITY, LogItem};

// =========================================================
// LogSender
// =========================================================

/// Cheaply cloneable handle for sending log items without
/// blocking. Held inside `LogContext`, `Logger`, and any
/// host code that needs to emit log items directly.
///
/// If the bounded channel is full, `send()` drops the item
/// and increments a counter. The frontend can query the
/// dropped count to display a warning.
#[derive(Clone)]
pub struct LogSender {
    tx: mpsc::Sender<LogItem>,
    dropped: Arc<AtomicU64>,
}

impl LogSender {
    /// Send a log item to the logging task. Never blocks.
    ///
    /// If the channel is full the item is silently dropped
    /// and the dropped counter is incremented.
    pub fn send(&self, item: LogItem) {
        if self.tx.try_send(item).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Number of items dropped due to channel backpressure
    /// since the logging system was started.
    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Discarding `LogSender` for tests. The receiver is
    /// leaked so the channel stays open for the test's
    /// lifetime — closing it would make every `send` fail
    /// and pollute the dropped counter.
    #[cfg(test)]
    pub fn test_sender() -> Self {
        let (tx, rx) = mpsc::channel::<LogItem>(CHANNEL_CAPACITY);
        Box::leak(Box::new(rx));
        Self {
            tx,
            dropped: Arc::new(AtomicU64::new(0)),
        }
    }
}

// =========================================================
// LogContext — producer-side handle
// =========================================================

/// Producer-side handle to the logging system.
///
/// Bundles exactly what a code path needs in order to emit
/// log items and start spans — `LogSender` plus the shared
/// `SpanRegistry`. Cheap to clone (each field is itself an
/// `Arc`-backed handle); pass by value at constructor
/// boundaries and store directly.
///
/// Consumers that need storage queries or broadcast
/// subscriptions (the devtools log commands) keep using
/// `Arc<LoggingSystem>` directly. Those are receiver-side
/// concerns and intentionally not part of this handle, so
/// producer code cannot reach for them by accident.
#[derive(Clone)]
pub struct LogContext {
    pub sender: LogSender,
    pub span_registry: Arc<SpanRegistry>,
}

impl LogContext {
    /// Build a `Logger` bound to a specific `LogSource`.
    /// Convenience over calling `Logger::new` with the three
    /// parts inline at every call site.
    pub fn logger(&self, source: super::LogSource) -> super::spans::Logger {
        super::spans::Logger::new(
            self.sender.clone(),
            Arc::clone(&self.span_registry),
            source,
        )
    }

    /// Build a `LogContext` with a discarding sender and a
    /// fresh registry. For tests that don't read the log
    /// stream back.
    #[cfg(test)]
    pub fn test_context() -> Self {
        Self {
            sender: LogSender::test_sender(),
            span_registry: Arc::new(SpanRegistry::new()),
        }
    }
}

// =========================================================
// LoggingSystem
// =========================================================

/// Central logging coordinator. Created once during app
/// `setup()`, before any gadgets are loaded.
///
/// Owns the ring buffer storage (behind a Mutex for Tauri
/// command access), the broadcast sender for live
/// subscribers, and the shared `SpanRegistry`. The async
/// logging task runs for the lifetime of the app.
///
/// Producer-side code does not consume `LoggingSystem`
/// directly — see `LogContext` for the producer-side handle.
pub struct LoggingSystem {
    sender: LogSender,
    storage: Arc<Mutex<Box<dyn LogStorage>>>,
    broadcast_tx: broadcast::Sender<LogItem>,
    span_registry: Arc<SpanRegistry>,
}

impl LoggingSystem {
    /// Start the logging system, spawning the background
    /// logging task onto the tokio runtime.
    ///
    /// The logging task runs until the sender side of the mpsc
    /// channel is dropped (i.e., all `LogSender` clones are
    /// gone), which happens naturally at app shutdown.
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel::<LogItem>(CHANNEL_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        let storage: Arc<Mutex<Box<dyn LogStorage>>> = Arc::new(Mutex::new(Box::new(
            RingBufferStorage::new(DEFAULT_RING_BUFFER_CAPACITY),
        )));
        let (broadcast_tx, _) = broadcast::channel::<LogItem>(BROADCAST_CAPACITY);

        let task_storage = Arc::clone(&storage);
        let task_broadcast = broadcast_tx.clone();

        // Use Tauri's async runtime spawn instead of tokio::spawn
        // directly, because setup() runs before the tokio runtime
        // context is available on the current thread.
        tauri::async_runtime::spawn(logging_task(rx, task_storage, task_broadcast));

        Self {
            sender: LogSender { tx, dropped },
            storage,
            broadcast_tx,
            span_registry: Arc::new(SpanRegistry::new()),
        }
    }

    /// Get a cloneable sender for producing log items.
    pub fn sender(&self) -> LogSender {
        self.sender.clone()
    }

    /// Shared span registry. The same `Arc` is handed out to
    /// every consumer so span ids are unique process-wide.
    pub fn span_registry(&self) -> &Arc<SpanRegistry> {
        &self.span_registry
    }

    /// Producer-side handle bundling `LogSender` plus the
    /// shared `SpanRegistry`. Threaded into orchestrators
    /// (`WasmGadgetBridge`, `CachedComponent`) and on into
    /// `WasmRuntime::instantiate` and `GadgetState::new`.
    pub fn context(&self) -> LogContext {
        LogContext {
            sender: self.sender.clone(),
            span_registry: Arc::clone(&self.span_registry),
        }
    }

    /// Subscribe to live log items. Returns a broadcast
    /// receiver that yields each item as it is stored.
    ///
    /// If the subscriber falls behind by more than
    /// `BROADCAST_CAPACITY` items, it will receive
    /// `RecvError::Lagged(n)` and can catch up via
    /// `storage().entries_after()`.
    pub fn subscribe(&self) -> broadcast::Receiver<LogItem> {
        self.broadcast_tx.subscribe()
    }

    /// Access the underlying storage for queries (history,
    /// stats, clear). The caller must lock the mutex.
    pub fn storage(&self) -> &Arc<Mutex<Box<dyn LogStorage>>> {
        &self.storage
    }

    /// Number of items dropped due to channel backpressure.
    pub fn dropped_count(&self) -> u64 {
        self.sender.dropped_count()
    }
}

// =========================================================
// Logging Task
// =========================================================

/// Background task that drains the mpsc channel, assigns
/// sequence numbers, stores items, and broadcasts them.
async fn logging_task(
    mut rx: mpsc::Receiver<LogItem>,
    storage: Arc<Mutex<Box<dyn LogStorage>>>,
    broadcast_tx: broadcast::Sender<LogItem>,
) {
    let mut seq_counter: u64 = 0;

    while let Some(mut item) = rx.recv().await {
        seq_counter += 1;
        item.seq = seq_counter;

        // Store the item in the ring buffer.
        {
            let mut store = storage.lock().expect("logging storage lock poisoned");
            store.push(item.clone());
        }

        // Broadcast to live subscribers. Ignore errors — they
        // occur when no subscribers are connected, which is
        // normal (devtools window may not be open).
        let _ = broadcast_tx.send(item);
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;
    use crate::wasm::logging::{LogItemKind, LogLevel, LogSource};

    fn test_item() -> LogItem {
        LogItem {
            seq: 0, // assigned by the logging task
            timestamp: SystemTime::now(),
            source: LogSource::Host,
            kind: LogItemKind::Message {
                level: LogLevel::Info,
                message: "test message".to_string(),
                metadata: vec![],
                span_id: None,
            },
        }
    }

    #[tokio::test]
    async fn send_and_receive() {
        let system = LoggingSystem::start();
        let sender = system.sender();

        sender.send(test_item());
        sender.send(test_item());

        // Give the logging task time to process.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let storage = system.storage().lock().expect("lock");
        assert_eq!(storage.len(), 2);

        let items = storage.tail(10);
        assert_eq!(items[0].seq, 1);
        assert_eq!(items[1].seq, 2);
    }

    #[tokio::test]
    async fn broadcast_delivers_to_subscriber() {
        let system = LoggingSystem::start();
        let sender = system.sender();
        let mut subscriber = system.subscribe();

        sender.send(test_item());

        let received =
            tokio::time::timeout(std::time::Duration::from_millis(100), subscriber.recv())
                .await
                .expect("timeout waiting for broadcast")
                .expect("broadcast recv error");

        assert_eq!(received.seq, 1);
        match &received.kind {
            LogItemKind::Message { message, .. } => assert_eq!(message, "test message"),
            other => panic!("expected Message, got {other:?}"),
        }
    }

    // ---------------------------------------------------------
    // LogContext + LoggingSystem::context()
    // ---------------------------------------------------------

    #[tokio::test]
    async fn log_context_logger_emits_with_provided_source() {
        let system = LoggingSystem::start();
        let ctx = system.context();

        // The logger built from a context binds to the source
        // we pass in, regardless of any other source already
        // in use elsewhere.
        let logger = ctx.logger(LogSource::Gadget("test-gadget".to_string()));
        logger.log(LogLevel::Info, "hello");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let storage = system.storage().lock().expect("lock");
        let items = storage.tail(10);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].source, LogSource::Gadget("test-gadget".to_string()));
        match &items[0].kind {
            LogItemKind::Message { message, level, .. } => {
                assert_eq!(message, "hello");
                assert_eq!(*level, LogLevel::Info);
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn logging_system_context_shares_sender_and_registry() {
        let system = LoggingSystem::start();
        let ctx = system.context();

        // The registry handed out by `context()` must be the
        // same `Arc` the system holds — span ids would
        // otherwise collide between code paths that started
        // spans through different registries.
        assert!(Arc::ptr_eq(&ctx.span_registry, system.span_registry()));

        // Items sent through the context's sender must land
        // in the system's storage (proves the sender is wired
        // to the same logging task, not a fresh one).
        ctx.sender.send(test_item());
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(system.storage().lock().expect("lock").len(), 1);
    }

    #[test]
    fn log_context_clone_shares_underlying_state() {
        // `LogContext` is `Clone` and clones must share the
        // underlying sender + registry. This is what makes it
        // safe to thread copies through every constructor that
        // touches logging.
        let ctx = LogContext::test_context();
        let cloned = ctx.clone();

        assert!(Arc::ptr_eq(&ctx.span_registry, &cloned.span_registry));
    }

    #[tokio::test]
    async fn dropped_count_increments() {
        // Create a system with a tiny channel to force drops.
        let (tx, rx) = mpsc::channel::<LogItem>(1);
        let dropped = Arc::new(AtomicU64::new(0));
        let storage: Arc<Mutex<Box<dyn LogStorage>>> =
            Arc::new(Mutex::new(Box::new(RingBufferStorage::new(100))));
        let (broadcast_tx, _) = broadcast::channel::<LogItem>(16);

        // Don't spawn the logging task — items stay in the
        // channel and it fills up immediately.
        let _rx = rx; // keep receiver alive

        let sender = LogSender {
            tx,
            dropped: Arc::clone(&dropped),
        };

        // Fill the channel (capacity = 1).
        sender.send(test_item());
        // This one should be dropped.
        sender.send(test_item());

        assert_eq!(sender.dropped_count(), 1);

        // Suppress unused variable warnings.
        let _ = storage;
        let _ = broadcast_tx;
    }
}
