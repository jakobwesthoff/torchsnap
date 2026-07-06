// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Span Registry and Host-Side Span Utilities
//
// Spans are explicit, caller-controlled timing regions.
// There are no implicit stacks or thread-local context —
// callers pass parent and span IDs explicitly.
//
// The SpanRegistry is a shared, thread-safe store of open
// spans. It generates unique IDs, records start times, and
// computes durations when spans end.
//
// Logger and SpanGuard provide ergonomic host-side usage
// with RAII-based span lifecycle management.
// =========================================================

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime};

use super::channel::LogSender;
use super::{LogItem, LogItemKind, LogLevel, LogSource, MAX_SPAN_NESTING};

// =========================================================
// Open Span State
// =========================================================

/// An in-flight span that has been started but not yet ended.
struct OpenSpan {
    name: String,
    parent_id: Option<u64>,
    source: LogSource,
    start: Instant,
    start_metadata: Vec<(String, String)>,
    /// Nesting depth: 0 for root spans, 1 for children of root, etc.
    depth: u32,
}

// =========================================================
// CompletedSpan
// =========================================================

/// Returned by `SpanRegistry::end()`. Contains everything
/// needed to construct a `LogItemKind::SpanEnd`.
///
/// This is an internal type — not serialized. The caller
/// converts it into `LogItemKind` via the `From` impl.
pub struct CompletedSpan {
    pub span_id: u64,
    pub name: String,
    pub parent_id: Option<u64>,
    pub depth: u32,
    pub duration_us: u64,
    /// Merged start + end metadata (end wins on key collision).
    pub metadata: Vec<(String, String)>,
    pub source: LogSource,
}

impl From<CompletedSpan> for LogItemKind {
    fn from(span: CompletedSpan) -> Self {
        LogItemKind::SpanEnd {
            span_id: span.span_id,
            name: span.name,
            parent_id: span.parent_id,
            depth: span.depth,
            duration_us: span.duration_us,
            metadata: span.metadata,
        }
    }
}

// =========================================================
// SpanRegistry
// =========================================================

/// Thread-safe registry of open spans.
///
/// Created once and shared via `Arc`. Any part of the app
/// (gadget host imports, bridge, runtime, host code) can
/// start and end spans through this registry.
pub struct SpanRegistry {
    id_gen: AtomicU64,
    open_spans: Mutex<HashMap<u64, OpenSpan>>,
}

impl SpanRegistry {
    /// Create an empty span registry.
    pub fn new() -> Self {
        Self {
            id_gen: AtomicU64::new(1),
            open_spans: Mutex::new(HashMap::new()),
        }
    }

    /// Start a new span. Returns `(span_id, depth)`, or `None`
    /// if the nesting depth would exceed `MAX_SPAN_NESTING`.
    pub fn start(
        &self,
        name: String,
        parent_id: Option<u64>,
        source: LogSource,
        metadata: Vec<(String, String)>,
    ) -> Option<(u64, u32)> {
        // Compute depth by walking the parent chain.
        let depth: u32 = if let Some(pid) = parent_id {
            let spans = self.open_spans.lock().expect("span registry not poisoned");
            let parent_depth = self.depth_of(pid, &spans);
            if parent_depth >= MAX_SPAN_NESTING {
                return None;
            }
            parent_depth as u32
        } else {
            0
        };

        let id = self.id_gen.fetch_add(1, Ordering::Relaxed);

        let span = OpenSpan {
            name,
            parent_id,
            source,
            start: Instant::now(),
            start_metadata: metadata,
            depth,
        };

        self.open_spans
            .lock()
            .expect("span registry not poisoned")
            .insert(id, span);

        Some((id, depth))
    }

    /// End a span and return a `CompletedSpan` with duration
    /// and merged metadata. Returns `None` if the span ID is
    /// not found (e.g., already ended or invalid).
    ///
    /// End-metadata is merged with start-metadata. On key
    /// collision, end-metadata wins.
    pub fn end(&self, span_id: u64, end_metadata: Vec<(String, String)>) -> Option<CompletedSpan> {
        let span = self
            .open_spans
            .lock()
            .expect("span registry not poisoned")
            .remove(&span_id)?;

        let duration_us = span.start.elapsed().as_micros() as u64;

        // Merge metadata: start first, then end overwrites.
        let metadata = merge_metadata(span.start_metadata, end_metadata);

        Some(CompletedSpan {
            span_id,
            name: span.name,
            parent_id: span.parent_id,
            depth: span.depth,
            duration_us,
            metadata,
            source: span.source,
        })
    }

    /// Walk the parent chain to compute nesting depth.
    /// Returns 0 for root spans, 1 for children of root, etc.
    fn depth_of(&self, span_id: u64, spans: &HashMap<u64, OpenSpan>) -> usize {
        let mut depth = 0;
        let mut current = Some(span_id);

        while let Some(id) = current {
            if let Some(span) = spans.get(&id) {
                depth += 1;
                current = span.parent_id;
            } else {
                // Parent not found (already ended or root).
                break;
            }
        }

        depth
    }

    /// Number of currently open spans (for testing/diagnostics).
    #[cfg(test)]
    pub fn open_count(&self) -> usize {
        self.open_spans
            .lock()
            .expect("span registry not poisoned")
            .len()
    }
}

/// Merge two metadata lists. `end` entries overwrite `start`
/// entries with the same key.
fn merge_metadata(
    start: Vec<(String, String)>,
    end: Vec<(String, String)>,
) -> Vec<(String, String)> {
    if end.is_empty() {
        return start;
    }
    if start.is_empty() {
        return end;
    }

    let mut merged: HashMap<String, String> = start.into_iter().collect();
    for (key, value) in end {
        merged.insert(key, value);
    }
    merged.into_iter().collect()
}

// =========================================================
// Logger — host-side logging handle
// =========================================================

/// Bundles a `LogSender`, `SpanRegistry`, and `LogSource`
/// for convenient host-side logging and span creation.
///
/// Obtained via `LoggingSystem::logger(source)`.
#[derive(Clone)]
pub struct Logger {
    sender: LogSender,
    registry: std::sync::Arc<SpanRegistry>,
    source: LogSource,
}

impl Logger {
    /// Create a new logger for the given source.
    pub fn new(
        sender: LogSender,
        registry: std::sync::Arc<SpanRegistry>,
        source: LogSource,
    ) -> Self {
        Self {
            sender,
            registry,
            source,
        }
    }

    /// Emit a log message.
    // Sibling of `log_with_meta` below, which `cached_component` uses
    // for the metadata-carrying case; this plain form is exercised only
    // by this crate's tests.
    #[allow(dead_code)]
    pub fn log(&self, level: LogLevel, message: impl Into<String>) {
        self.sender.send(LogItem {
            seq: 0,
            timestamp: SystemTime::now(),
            source: self.source.clone(),
            kind: LogItemKind::Message {
                level,
                message: message.into(),
                metadata: vec![],
                span_id: None,
            },
        });
    }

    /// Emit a log message with metadata.
    pub fn log_with_meta(
        &self,
        level: LogLevel,
        message: impl Into<String>,
        metadata: Vec<(String, String)>,
    ) {
        self.sender.send(LogItem {
            seq: 0,
            timestamp: SystemTime::now(),
            source: self.source.clone(),
            kind: LogItemKind::Message {
                level,
                message: message.into(),
                metadata,
                span_id: None,
            },
        });
    }

    /// Emit a log message associated with a span.
    // Sibling of `log` and `log_with_meta` above; lets a caller attach a
    // message to a span it only has the numeric id for, without holding
    // the `SpanGuard`. No in-repo caller needs that combination.
    #[allow(dead_code)]
    pub fn log_in_span(&self, level: LogLevel, message: impl Into<String>, span_id: u64) {
        self.sender.send(LogItem {
            seq: 0,
            timestamp: SystemTime::now(),
            source: self.source.clone(),
            kind: LogItemKind::Message {
                level,
                message: message.into(),
                metadata: vec![],
                span_id: Some(span_id),
            },
        });
    }

    /// Create a span builder for starting a new span.
    pub fn span(&self, name: impl Into<String>) -> SpanBuilder {
        SpanBuilder {
            name: name.into(),
            parent_id: None,
            metadata: vec![],
            logger: self.clone(),
        }
    }
}

// =========================================================
// SpanBuilder — fluent span construction
// =========================================================

/// Builder for starting a span with optional parent and
/// metadata.
pub struct SpanBuilder {
    name: String,
    parent_id: Option<u64>,
    metadata: Vec<(String, String)>,
    logger: Logger,
}

impl SpanBuilder {
    /// Set the parent span for nesting.
    // Sibling of `meta` below, which every span builder call in this
    // crate uses; explicit parenting by numeric id has no in-repo caller
    // because `SpanGuard::child` covers the common case of nesting under
    // a guard already in hand.
    #[allow(dead_code)]
    pub fn parent(mut self, parent_id: u64) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Add a metadata key-value pair.
    pub fn meta(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.push((key.into(), value.into()));
        self
    }

    /// Start the span and return an RAII guard.
    ///
    /// Returns `None` if the nesting depth would exceed the
    /// limit. In practice this means the span is silently
    /// skipped, which is acceptable for a diagnostic system.
    ///
    /// Emits a `SpanStart` log item so the frontend can track
    /// open spans in real time.
    pub fn start(self) -> Option<SpanGuard> {
        // Clone name and metadata before passing ownership to
        // the registry — we need them for the span-start item.
        let name = self.name.clone();
        let metadata = self.metadata.clone();
        let parent_id = self.parent_id;

        let (span_id, depth) = self.logger.registry.start(
            self.name,
            self.parent_id,
            self.logger.source.clone(),
            self.metadata,
        )?;

        // Emit span-start log item.
        self.logger.sender.send(LogItem {
            seq: 0,
            timestamp: SystemTime::now(),
            source: self.logger.source.clone(),
            kind: LogItemKind::SpanStart {
                span_id,
                name,
                parent_id,
                depth,
                metadata,
            },
        });

        Some(SpanGuard {
            span_id,
            logger: self.logger,
            ended: false,
        })
    }
}

// =========================================================
// SpanGuard — RAII span lifecycle
// =========================================================

/// RAII guard that ends a span on drop.
///
/// For explicit end-metadata, call `end_with_meta()` which
/// consumes the guard. Otherwise, the span is ended with
/// empty metadata on drop.
pub struct SpanGuard {
    span_id: u64,
    logger: Logger,
    ended: bool,
}

impl SpanGuard {
    /// The span ID for this guard. Useful for creating child
    /// spans or associating log messages.
    // Sibling of `child` and `end_with_meta` below, both of which are
    // used by `cached_component`; no in-repo caller needs the raw id
    // directly.
    #[allow(dead_code)]
    pub fn id(&self) -> u64 {
        self.span_id
    }

    /// Emit a log message associated with this span.
    // Sibling of `child` and `end_with_meta` below; no in-repo caller
    // logs through a `SpanGuard` directly rather than through `Logger`.
    #[allow(dead_code)]
    pub fn log(&self, level: LogLevel, message: impl Into<String>) {
        self.logger.sender.send(LogItem {
            seq: 0,
            timestamp: SystemTime::now(),
            source: self.logger.source.clone(),
            kind: LogItemKind::Message {
                level,
                message: message.into(),
                metadata: vec![],
                span_id: Some(self.span_id),
            },
        });
    }

    /// Create a child span builder that auto-parents to this span.
    pub fn child(&self, name: impl Into<String>) -> SpanBuilder {
        SpanBuilder {
            name: name.into(),
            parent_id: Some(self.span_id),
            metadata: vec![],
            logger: self.logger.clone(),
        }
    }

    /// End the span with metadata, consuming the guard.
    pub fn end_with_meta(mut self, metadata: Vec<(String, String)>) {
        self.end_inner(metadata);
    }

    /// Internal end logic shared by drop and end_with_meta.
    fn end_inner(&mut self, metadata: Vec<(String, String)>) {
        if self.ended {
            return;
        }
        self.ended = true;

        if let Some(completed) = self.logger.registry.end(self.span_id, metadata) {
            self.logger.sender.send(LogItem {
                seq: 0,
                timestamp: SystemTime::now(),
                source: completed.source.clone(),
                kind: completed.into(),
            });
        }
    }
}

impl Drop for SpanGuard {
    fn drop(&mut self) {
        self.end_inner(vec![]);
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::wasm::logging::channel::LoggingSystem;

    fn test_logger() -> (Logger, Arc<LoggingSystem>) {
        let system = Arc::new(LoggingSystem::start());
        let registry = Arc::new(SpanRegistry::new());
        let logger = Logger::new(system.sender(), registry, LogSource::Host);
        (logger, system)
    }

    fn test_logger_with_registry() -> (Logger, Arc<SpanRegistry>, Arc<LoggingSystem>) {
        let system = Arc::new(LoggingSystem::start());
        let registry = Arc::new(SpanRegistry::new());
        let logger = Logger::new(system.sender(), Arc::clone(&registry), LogSource::Host);
        (logger, registry, system)
    }

    /// Helper: receive the next item from the broadcast channel
    /// with a 100ms timeout.
    async fn recv(sub: &mut tokio::sync::broadcast::Receiver<LogItem>) -> LogItem {
        tokio::time::timeout(std::time::Duration::from_millis(100), sub.recv())
            .await
            .expect("timeout waiting for log item")
            .expect("broadcast recv error")
    }

    // ----- SpanRegistry tests -----

    #[test]
    fn start_returns_id_and_depth() {
        let registry = SpanRegistry::new();

        let (root_id, root_depth) = registry
            .start("root".into(), None, LogSource::Host, vec![])
            .expect("root span should start");
        assert_eq!(root_depth, 0);

        let (child_id, child_depth) = registry
            .start("child".into(), Some(root_id), LogSource::Host, vec![])
            .expect("child span should start");
        assert_eq!(child_depth, 1);
        assert_ne!(root_id, child_id);

        let (_grandchild_id, grandchild_depth) = registry
            .start("grandchild".into(), Some(child_id), LogSource::Host, vec![])
            .expect("grandchild should start");
        assert_eq!(grandchild_depth, 2);

        assert_eq!(registry.open_count(), 3);
    }

    #[test]
    fn end_returns_completed_span() {
        let registry = SpanRegistry::new();
        let (id, _) = registry
            .start("test".into(), None, LogSource::Host, vec![])
            .expect("span should start");

        assert_eq!(registry.open_count(), 1);

        let completed = registry.end(id, vec![]).expect("span should end");
        assert_eq!(completed.span_id, id);
        assert_eq!(completed.name, "test");
        assert_eq!(completed.parent_id, None);
        assert_eq!(completed.depth, 0);
        assert_eq!(completed.source, LogSource::Host);
        assert_eq!(registry.open_count(), 0);
    }

    #[test]
    fn end_nonexistent_span_returns_none() {
        let registry = SpanRegistry::new();
        assert!(registry.end(9999, vec![]).is_none());
    }

    #[test]
    fn double_end_returns_none_second_time() {
        let registry = SpanRegistry::new();
        let (id, _) = registry
            .start("test".into(), None, LogSource::Host, vec![])
            .expect("span should start");

        assert!(registry.end(id, vec![]).is_some());
        assert!(registry.end(id, vec![]).is_none());
    }

    #[test]
    fn nested_spans_with_parent() {
        let registry = SpanRegistry::new();
        let (parent, _) = registry
            .start("parent".into(), None, LogSource::Host, vec![])
            .expect("parent should start");

        let (child, child_depth) = registry
            .start("child".into(), Some(parent), LogSource::Host, vec![])
            .expect("child should start");
        assert_eq!(child_depth, 1);

        assert_eq!(registry.open_count(), 2);

        let child_completed = registry.end(child, vec![]).expect("child should end");
        assert_eq!(child_completed.parent_id, Some(parent));
        assert_eq!(child_completed.depth, 1);

        let parent_completed = registry.end(parent, vec![]).expect("parent should end");
        assert_eq!(parent_completed.parent_id, None);
        assert_eq!(parent_completed.depth, 0);
    }

    #[test]
    fn nesting_depth_limit() {
        let registry = SpanRegistry::new();
        let mut current_id = None;

        // Build a chain up to MAX_SPAN_NESTING.
        for i in 0..MAX_SPAN_NESTING {
            let (id, _) = registry
                .start(format!("span-{i}"), current_id, LogSource::Host, vec![])
                .expect("span within limit should start");
            current_id = Some(id);
        }

        // The next span should be rejected.
        let rejected = registry.start("too-deep".into(), current_id, LogSource::Host, vec![]);
        assert!(
            rejected.is_none(),
            "span exceeding depth limit should be rejected"
        );

        assert_eq!(registry.open_count(), MAX_SPAN_NESTING);
    }

    #[test]
    fn span_without_parent_always_accepted() {
        let registry = SpanRegistry::new();

        // Root spans (no parent) have depth 0, always accepted.
        for _ in 0..100 {
            let (id, depth) = registry
                .start("root".into(), None, LogSource::Host, vec![])
                .expect("root span should start");
            assert_eq!(depth, 0);
            registry.end(id, vec![]);
        }
    }

    #[test]
    fn metadata_merge_end_wins() {
        let registry = SpanRegistry::new();
        let (id, _) = registry
            .start(
                "test".into(),
                None,
                LogSource::Host,
                vec![
                    ("key1".into(), "start-val".into()),
                    ("key2".into(), "only-start".into()),
                ],
            )
            .expect("span should start");

        let completed = registry
            .end(
                id,
                vec![
                    ("key1".into(), "end-val".into()),
                    ("key3".into(), "only-end".into()),
                ],
            )
            .expect("span should end");

        let meta: HashMap<String, String> = completed.metadata.into_iter().collect();
        assert_eq!(meta.get("key1").unwrap(), "end-val", "end should win");
        assert_eq!(meta.get("key2").unwrap(), "only-start");
        assert_eq!(meta.get("key3").unwrap(), "only-end");
    }

    #[test]
    fn metadata_merge_empty_cases() {
        assert_eq!(
            merge_metadata(vec![], vec![]),
            Vec::<(String, String)>::new()
        );

        let start_only = vec![("k".into(), "v".into())];
        assert_eq!(merge_metadata(start_only.clone(), vec![]), start_only);

        let end_only = vec![("k".into(), "v".into())];
        assert_eq!(merge_metadata(vec![], end_only.clone()), end_only);
    }

    #[test]
    fn span_ids_are_unique() {
        let registry = SpanRegistry::new();
        let (id1, _) = registry
            .start("a".into(), None, LogSource::Host, vec![])
            .unwrap();
        let (id2, _) = registry
            .start("b".into(), None, LogSource::Host, vec![])
            .unwrap();
        assert_ne!(id1, id2);
    }

    #[test]
    fn gadget_source_preserved() {
        let registry = SpanRegistry::new();
        let (id, _) = registry
            .start(
                "test".into(),
                None,
                LogSource::Gadget("hello-world".into()),
                vec![],
            )
            .unwrap();

        let completed = registry.end(id, vec![]).unwrap();
        assert_eq!(completed.source, LogSource::Gadget("hello-world".into()));
    }

    #[test]
    fn completed_span_into_log_item_kind() {
        let completed = CompletedSpan {
            span_id: 42,
            name: "search".into(),
            parent_id: Some(1),
            depth: 1,
            duration_us: 12345,
            metadata: vec![("k".into(), "v".into())],
            source: LogSource::Host,
        };

        let kind: LogItemKind = completed.into();
        match kind {
            LogItemKind::SpanEnd {
                span_id,
                name,
                parent_id,
                depth,
                duration_us,
                metadata,
            } => {
                assert_eq!(span_id, 42);
                assert_eq!(name, "search");
                assert_eq!(parent_id, Some(1));
                assert_eq!(depth, 1);
                assert_eq!(duration_us, 12345);
                assert_eq!(metadata, vec![("k".to_string(), "v".to_string())]);
            }
            other => panic!("expected SpanEnd, got {other:?}"),
        }
    }

    // ----- Logger tests -----

    #[tokio::test]
    async fn logger_log_sends_message() {
        let (logger, system) = test_logger();
        let mut sub = system.subscribe();

        logger.log(LogLevel::Info, "hello");

        let item = recv(&mut sub).await;
        assert_eq!(item.source, LogSource::Host);
        match &item.kind {
            LogItemKind::Message { level, message, .. } => {
                assert_eq!(*level, LogLevel::Info);
                assert_eq!(message, "hello");
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn logger_log_with_meta() {
        let (logger, system) = test_logger();
        let mut sub = system.subscribe();

        logger.log_with_meta(LogLevel::Warn, "test", vec![("key".into(), "val".into())]);

        let item = recv(&mut sub).await;
        match &item.kind {
            LogItemKind::Message {
                level, metadata, ..
            } => {
                assert_eq!(*level, LogLevel::Warn);
                assert_eq!(*metadata, vec![("key".to_string(), "val".to_string())]);
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn logger_log_in_span() {
        let (logger, system) = test_logger();
        let mut sub = system.subscribe();

        logger.log_in_span(LogLevel::Debug, "inside span", 42);

        let item = recv(&mut sub).await;
        match &item.kind {
            LogItemKind::Message {
                span_id, message, ..
            } => {
                assert_eq!(*span_id, Some(42));
                assert_eq!(message, "inside span");
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    // ----- SpanGuard tests -----

    #[tokio::test]
    async fn span_emits_start_and_end() {
        let (logger, registry, system) = test_logger_with_registry();
        let mut sub = system.subscribe();

        {
            let _guard = logger.span("test-span").start().expect("span should start");
            assert_eq!(registry.open_count(), 1);

            // First item: span-start.
            let start_item = recv(&mut sub).await;
            match &start_item.kind {
                LogItemKind::SpanStart {
                    name,
                    depth,
                    parent_id,
                    ..
                } => {
                    assert_eq!(name, "test-span");
                    assert_eq!(*depth, 0);
                    assert_eq!(*parent_id, None);
                }
                other => panic!("expected SpanStart, got {other:?}"),
            }
        }
        // Guard dropped — span should be ended.

        // Second item: span-end.
        let end_item = recv(&mut sub).await;
        match &end_item.kind {
            LogItemKind::SpanEnd {
                name,
                depth,
                parent_id,
                ..
            } => {
                assert_eq!(name, "test-span");
                assert_eq!(*depth, 0);
                assert_eq!(*parent_id, None);
            }
            other => panic!("expected SpanEnd, got {other:?}"),
        }
        assert_eq!(registry.open_count(), 0);
    }

    #[tokio::test]
    async fn span_start_carries_metadata() {
        let (logger, _, system) = test_logger_with_registry();
        let mut sub = system.subscribe();

        let guard = logger
            .span("test")
            .meta("input", "42")
            .start()
            .expect("span should start");

        let start_item = recv(&mut sub).await;
        match &start_item.kind {
            LogItemKind::SpanStart { metadata, .. } => {
                assert_eq!(*metadata, vec![("input".to_string(), "42".to_string())]);
            }
            other => panic!("expected SpanStart, got {other:?}"),
        }

        drop(guard);
    }

    #[tokio::test]
    async fn span_guard_end_with_meta() {
        let (logger, _, system) = test_logger_with_registry();
        let mut sub = system.subscribe();

        let guard = logger
            .span("test")
            .meta("input", "42")
            .start()
            .expect("span should start");

        // Skip the span-start item.
        let _start = recv(&mut sub).await;

        guard.end_with_meta(vec![("output".into(), "84".into())]);

        let end_item = recv(&mut sub).await;
        match &end_item.kind {
            LogItemKind::SpanEnd { metadata, .. } => {
                let meta: HashMap<String, String> = metadata.clone().into_iter().collect();
                assert_eq!(meta.get("input").unwrap(), "42");
                assert_eq!(meta.get("output").unwrap(), "84");
            }
            other => panic!("expected SpanEnd, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn span_guard_child_auto_parents() {
        let (logger, _, system) = test_logger_with_registry();
        let mut sub = system.subscribe();

        let parent = logger.span("parent").start().expect("parent");
        let parent_id = parent.id();

        // Skip parent span-start.
        let _parent_start = recv(&mut sub).await;

        {
            let _child = parent.child("child").start().expect("child");

            // Child span-start should reference parent.
            let child_start = recv(&mut sub).await;
            match &child_start.kind {
                LogItemKind::SpanStart {
                    parent_id: pid,
                    depth,
                    name,
                    ..
                } => {
                    assert_eq!(*pid, Some(parent_id));
                    assert_eq!(*depth, 1);
                    assert_eq!(name, "child");
                }
                other => panic!("expected SpanStart, got {other:?}"),
            }
        }

        // Child span-end should reference parent.
        let child_end = recv(&mut sub).await;
        match &child_end.kind {
            LogItemKind::SpanEnd {
                parent_id: pid,
                name,
                depth,
                ..
            } => {
                assert_eq!(*pid, Some(parent_id));
                assert_eq!(name, "child");
                assert_eq!(*depth, 1);
            }
            other => panic!("expected SpanEnd, got {other:?}"),
        }

        drop(parent);
    }

    #[tokio::test]
    async fn span_guard_log_attaches_span_id() {
        let (logger, _, system) = test_logger_with_registry();
        let mut sub = system.subscribe();

        let guard = logger.span("test").start().expect("span");
        let span_id = guard.id();

        // Skip span-start.
        let _start = recv(&mut sub).await;

        guard.log(LogLevel::Info, "inside");

        let log_item = recv(&mut sub).await;
        match &log_item.kind {
            LogItemKind::Message {
                message,
                span_id: sid,
                ..
            } => {
                assert_eq!(message, "inside");
                assert_eq!(*sid, Some(span_id));
            }
            other => panic!("expected Message, got {other:?}"),
        }

        drop(guard);
    }

    #[tokio::test]
    async fn nested_span_depths_in_items() {
        let (logger, _, system) = test_logger_with_registry();
        let mut sub = system.subscribe();

        let outer = logger.span("outer").start().expect("outer");

        let outer_start = recv(&mut sub).await;
        match &outer_start.kind {
            LogItemKind::SpanStart { depth, .. } => assert_eq!(*depth, 0),
            other => panic!("expected SpanStart, got {other:?}"),
        }

        {
            let inner = outer.child("inner").start().expect("inner");

            let inner_start = recv(&mut sub).await;
            match &inner_start.kind {
                LogItemKind::SpanStart { depth, .. } => assert_eq!(*depth, 1),
                other => panic!("expected SpanStart, got {other:?}"),
            }

            {
                let _deep = inner.child("deep").start().expect("deep");

                let deep_start = recv(&mut sub).await;
                match &deep_start.kind {
                    LogItemKind::SpanStart { depth, .. } => assert_eq!(*depth, 2),
                    other => panic!("expected SpanStart, got {other:?}"),
                }
            }
            // deep dropped → end emitted
            let deep_end = recv(&mut sub).await;
            match &deep_end.kind {
                LogItemKind::SpanEnd { depth, .. } => assert_eq!(*depth, 2),
                other => panic!("expected SpanEnd, got {other:?}"),
            }

            drop(inner);
        }

        let inner_end = recv(&mut sub).await;
        match &inner_end.kind {
            LogItemKind::SpanEnd { depth, .. } => assert_eq!(*depth, 1),
            other => panic!("expected SpanEnd, got {other:?}"),
        }

        drop(outer);

        let outer_end = recv(&mut sub).await;
        match &outer_end.kind {
            LogItemKind::SpanEnd { depth, .. } => assert_eq!(*depth, 0),
            other => panic!("expected SpanEnd, got {other:?}"),
        }
    }
}
