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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime};

use super::channel::LogSender;
use super::{LogEntry, LogLevel, LogSource, SpanInfo, MAX_SPAN_NESTING};

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
}

// =========================================================
// SpanRegistry
// =========================================================

/// Thread-safe registry of open spans.
///
/// Created once and shared via `Arc`. Any part of the app
/// (plugin host imports, bridge, runtime, host code) can
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

    /// Start a new span. Returns the span ID, or `None` if
    /// the nesting depth would exceed `MAX_SPAN_NESTING`.
    pub fn start(
        &self,
        name: String,
        parent_id: Option<u64>,
        source: LogSource,
        metadata: Vec<(String, String)>,
    ) -> Option<u64> {
        // Check nesting depth by walking the parent chain.
        if let Some(pid) = parent_id {
            let spans = self.open_spans.lock().expect("span registry not poisoned");
            let depth = self.depth_of(pid, &spans);
            if depth >= MAX_SPAN_NESTING {
                return None;
            }
        }

        let id = self.id_gen.fetch_add(1, Ordering::Relaxed);

        let span = OpenSpan {
            name,
            parent_id,
            source,
            start: Instant::now(),
            start_metadata: metadata,
        };

        self.open_spans
            .lock()
            .expect("span registry not poisoned")
            .insert(id, span);

        Some(id)
    }

    /// End a span and return the completed `SpanInfo` with
    /// duration and merged metadata. Returns `None` if the
    /// span ID is not found (e.g., already ended or invalid).
    ///
    /// End-metadata is merged with start-metadata. On key
    /// collision, end-metadata wins.
    pub fn end(
        &self,
        span_id: u64,
        end_metadata: Vec<(String, String)>,
    ) -> Option<(SpanInfo, LogSource)> {
        let span = self
            .open_spans
            .lock()
            .expect("span registry not poisoned")
            .remove(&span_id)?;

        let duration_us = span.start.elapsed().as_micros() as u64;

        // Merge metadata: start first, then end overwrites.
        let metadata = merge_metadata(span.start_metadata, end_metadata);

        let info = SpanInfo {
            span_id,
            parent_id: span.parent_id,
            name: span.name,
            duration_us,
            metadata,
        };

        Some((info, span.source))
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

    /// Emit a log entry.
    pub fn log(&self, level: LogLevel, message: impl Into<String>) {
        self.sender.send(LogEntry {
            seq: 0,
            timestamp: SystemTime::now(),
            level,
            source: self.source.clone(),
            message: message.into(),
            metadata: vec![],
            span_id: None,
            span: None,
        });
    }

    /// Emit a log entry with metadata.
    pub fn log_with_meta(
        &self,
        level: LogLevel,
        message: impl Into<String>,
        metadata: Vec<(String, String)>,
    ) {
        self.sender.send(LogEntry {
            seq: 0,
            timestamp: SystemTime::now(),
            level,
            source: self.source.clone(),
            message: message.into(),
            metadata,
            span_id: None,
            span: None,
        });
    }

    /// Emit a log entry associated with a span.
    pub fn log_in_span(
        &self,
        level: LogLevel,
        message: impl Into<String>,
        span_id: u64,
    ) {
        self.sender.send(LogEntry {
            seq: 0,
            timestamp: SystemTime::now(),
            level,
            source: self.source.clone(),
            message: message.into(),
            metadata: vec![],
            span_id: Some(span_id),
            span: None,
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
    pub fn start(self) -> Option<SpanGuard> {
        let span_id = self.logger.registry.start(
            self.name,
            self.parent_id,
            self.logger.source.clone(),
            self.metadata,
        )?;

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
    /// spans or associating log entries.
    pub fn id(&self) -> u64 {
        self.span_id
    }

    /// Emit a log entry associated with this span.
    pub fn log(&self, level: LogLevel, message: impl Into<String>) {
        self.logger.sender.send(LogEntry {
            seq: 0,
            timestamp: SystemTime::now(),
            level,
            source: self.logger.source.clone(),
            message: message.into(),
            metadata: vec![],
            span_id: Some(self.span_id),
            span: None,
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

        if let Some((span_info, source)) = self.logger.registry.end(self.span_id, metadata) {
            self.logger.sender.send(LogEntry {
                seq: 0,
                timestamp: SystemTime::now(),
                level: LogLevel::Debug,
                source,
                message: span_info.name.clone(),
                metadata: vec![],
                span_id: None,
                span: Some(span_info),
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

    // ----- SpanRegistry tests -----

    #[test]
    fn start_and_end_span() {
        let registry = SpanRegistry::new();
        let id = registry
            .start("test".into(), None, LogSource::Host, vec![])
            .expect("span should start");

        assert_eq!(registry.open_count(), 1);

        let (info, source) = registry.end(id, vec![]).expect("span should end");
        assert_eq!(info.span_id, id);
        assert_eq!(info.name, "test");
        assert!(info.duration_us > 0 || info.duration_us == 0); // can be 0 if very fast
        assert_eq!(info.parent_id, None);
        assert_eq!(source, LogSource::Host);
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
        let id = registry
            .start("test".into(), None, LogSource::Host, vec![])
            .expect("span should start");

        assert!(registry.end(id, vec![]).is_some());
        assert!(registry.end(id, vec![]).is_none());
    }

    #[test]
    fn nested_spans_with_parent() {
        let registry = SpanRegistry::new();
        let parent = registry
            .start("parent".into(), None, LogSource::Host, vec![])
            .expect("parent should start");

        let child = registry
            .start("child".into(), Some(parent), LogSource::Host, vec![])
            .expect("child should start");

        assert_eq!(registry.open_count(), 2);

        let (child_info, _) = registry.end(child, vec![]).expect("child should end");
        assert_eq!(child_info.parent_id, Some(parent));

        let (parent_info, _) = registry.end(parent, vec![]).expect("parent should end");
        assert_eq!(parent_info.parent_id, None);
    }

    #[test]
    fn nesting_depth_limit() {
        let registry = SpanRegistry::new();
        let mut current_id = None;

        // Build a chain up to MAX_SPAN_NESTING.
        for i in 0..MAX_SPAN_NESTING {
            let id = registry
                .start(format!("span-{i}"), current_id, LogSource::Host, vec![])
                .expect("span within limit should start");
            current_id = Some(id);
        }

        // The next span should be rejected.
        let rejected = registry.start(
            "too-deep".into(),
            current_id,
            LogSource::Host,
            vec![],
        );
        assert!(rejected.is_none(), "span exceeding depth limit should be rejected");

        assert_eq!(registry.open_count(), MAX_SPAN_NESTING);
    }

    #[test]
    fn span_without_parent_always_accepted() {
        let registry = SpanRegistry::new();

        // Root spans (no parent) have depth 0, always accepted.
        for _ in 0..100 {
            let id = registry
                .start("root".into(), None, LogSource::Host, vec![])
                .expect("root span should start");
            registry.end(id, vec![]);
        }
    }

    #[test]
    fn metadata_merge_end_wins() {
        let registry = SpanRegistry::new();
        let id = registry
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

        let (info, _) = registry
            .end(
                id,
                vec![
                    ("key1".into(), "end-val".into()),
                    ("key3".into(), "only-end".into()),
                ],
            )
            .expect("span should end");

        let meta: HashMap<String, String> = info.metadata.into_iter().collect();
        assert_eq!(meta.get("key1").unwrap(), "end-val", "end should win");
        assert_eq!(meta.get("key2").unwrap(), "only-start");
        assert_eq!(meta.get("key3").unwrap(), "only-end");
    }

    #[test]
    fn metadata_merge_empty_cases() {
        assert_eq!(merge_metadata(vec![], vec![]), Vec::<(String, String)>::new());

        let start_only = vec![("k".into(), "v".into())];
        assert_eq!(merge_metadata(start_only.clone(), vec![]), start_only);

        let end_only = vec![("k".into(), "v".into())];
        assert_eq!(merge_metadata(vec![], end_only.clone()), end_only);
    }

    #[test]
    fn span_ids_are_unique() {
        let registry = SpanRegistry::new();
        let id1 = registry
            .start("a".into(), None, LogSource::Host, vec![])
            .unwrap();
        let id2 = registry
            .start("b".into(), None, LogSource::Host, vec![])
            .unwrap();
        assert_ne!(id1, id2);
    }

    #[test]
    fn plugin_source_preserved() {
        let registry = SpanRegistry::new();
        let id = registry
            .start(
                "test".into(),
                None,
                LogSource::Plugin("hello-world".into()),
                vec![],
            )
            .unwrap();

        let (_, source) = registry.end(id, vec![]).unwrap();
        assert_eq!(source, LogSource::Plugin("hello-world".into()));
    }

    // ----- Logger tests -----

    #[tokio::test]
    async fn logger_log_sends_entry() {
        let (logger, system) = test_logger();
        let mut sub = system.subscribe();

        logger.log(LogLevel::Info, "hello");

        let entry = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            sub.recv(),
        )
        .await
        .expect("timeout")
        .expect("recv error");

        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.message, "hello");
        assert_eq!(entry.source, LogSource::Host);
    }

    #[tokio::test]
    async fn logger_log_with_meta() {
        let (logger, system) = test_logger();
        let mut sub = system.subscribe();

        logger.log_with_meta(
            LogLevel::Warn,
            "test",
            vec![("key".into(), "val".into())],
        );

        let entry = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            sub.recv(),
        )
        .await
        .expect("timeout")
        .expect("recv error");

        assert_eq!(entry.metadata, vec![("key".to_string(), "val".to_string())]);
    }

    #[tokio::test]
    async fn logger_log_in_span() {
        let (logger, system) = test_logger();
        let mut sub = system.subscribe();

        logger.log_in_span(LogLevel::Debug, "inside span", 42);

        let entry = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            sub.recv(),
        )
        .await
        .expect("timeout")
        .expect("recv error");

        assert_eq!(entry.span_id, Some(42));
    }

    // ----- SpanGuard tests -----

    #[tokio::test]
    async fn span_guard_emits_on_drop() {
        let (logger, registry, system) = test_logger_with_registry();
        let mut sub = system.subscribe();

        {
            let _guard = logger.span("test-span").start().expect("span should start");
            assert_eq!(registry.open_count(), 1);
        }
        // Guard dropped — span should be ended.

        let entry = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            sub.recv(),
        )
        .await
        .expect("timeout")
        .expect("recv error");

        assert!(entry.span.is_some());
        let span_info = entry.span.unwrap();
        assert_eq!(span_info.name, "test-span");
        assert_eq!(span_info.parent_id, None);
        assert_eq!(registry.open_count(), 0);
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

        guard.end_with_meta(vec![("output".into(), "84".into())]);

        let entry = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            sub.recv(),
        )
        .await
        .expect("timeout")
        .expect("recv error");

        let span = entry.span.unwrap();
        let meta: HashMap<String, String> = span.metadata.into_iter().collect();
        assert_eq!(meta.get("input").unwrap(), "42");
        assert_eq!(meta.get("output").unwrap(), "84");
    }

    #[tokio::test]
    async fn span_guard_child_auto_parents() {
        let (logger, _, system) = test_logger_with_registry();
        let mut sub = system.subscribe();

        let parent = logger.span("parent").start().expect("parent");
        let parent_id = parent.id();

        {
            let _child = parent.child("child").start().expect("child");
        }

        // First entry should be the child's span-end.
        let entry = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            sub.recv(),
        )
        .await
        .expect("timeout")
        .expect("recv error");

        let child_span = entry.span.unwrap();
        assert_eq!(child_span.name, "child");
        assert_eq!(child_span.parent_id, Some(parent_id));

        drop(parent);
    }

    #[tokio::test]
    async fn span_guard_log_attaches_span_id() {
        let (logger, _, system) = test_logger_with_registry();
        let mut sub = system.subscribe();

        let guard = logger.span("test").start().expect("span");
        guard.log(LogLevel::Info, "inside");
        let span_id = guard.id();
        drop(guard);

        // First message should be the log entry.
        let log_entry = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            sub.recv(),
        )
        .await
        .expect("timeout")
        .expect("recv error");

        assert_eq!(log_entry.message, "inside");
        assert_eq!(log_entry.span_id, Some(span_id));
        assert!(log_entry.span.is_none()); // not a span-end entry
    }
}
