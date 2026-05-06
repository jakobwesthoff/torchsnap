// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// CachedComponent — disk-backed compiled component
//
// Wraps a single WASM gadget's compiled `Component` with a
// disk cache and lazy acquire/release semantics for
// in-memory residency. Delegates compilation and
// instantiation to `WasmRuntime`.
//
// On first `acquire()`, the component is either deserialized
// from a cached `.cwasm` file (cache hit) or compiled from
// the gadget source's WASM bytes, serialized to disk, and
// deserialized back as a file-backed (mmap'd) component
// (cache miss). `release()` drops the in-memory component
// without removing the cache file, so the next `acquire()`
// is a cheap deserialization.
//
// This component is also the orchestrator for the per-gadget
// span hierarchy. It owns the producer-side `LogContext`
// and emits the following span tree on `instantiate()`:
//
//   init                              (outer umbrella)
//   ├── acquire                       (compile / cache I/O)
//   │   ├── read-wasm                 (first acquire only)
//   │   ├── deserialize               (hit and miss converge)
//   │   ├── compile                   (miss only)
//   │   └── serialize-and-write       (miss only)
//   └── instantiate                   (linker + store + guest init)
//
// Standalone `acquire()` (e.g. in tests, or any caller that
// only needs the component but not an instance) emits the
// `acquire` subtree as a root.
// =========================================================

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use wasmtime::component::Component;

use super::WasmGadgetInstance;
use super::engine::WasmRuntime;
use crate::wasm::logging::channel::LogContext;
use crate::wasm::logging::spans::{Logger, SpanGuard};
use crate::wasm::logging::{LogLevel, LogSource};
use crate::wasm::source::GadgetSource;

/// Disk-backed compiled component with lazy acquire/release.
///
/// Owned by `WasmGadgetBridge`, one per gadget. Construction
/// is cheap (no I/O). The compile-or-deserialize step happens
/// on the first `acquire()` call.
pub struct CachedComponent {
    runtime: Arc<WasmRuntime>,
    log_ctx: LogContext,
    source: Arc<dyn GadgetSource + Send + Sync>,
    /// Cached at construction so span metadata and the per-
    /// gadget `Logger` don't re-walk the manifest on every
    /// call.
    gadget_id: String,
    /// Pre-built logger bound to `LogSource::Gadget(gadget_id)`.
    /// All host-emitted spans and warnings for this component
    /// flow through this handle.
    logger: Logger,
    cache_dir: PathBuf,
    /// Populated on first `acquire()` and retained across
    /// release/re-acquire cycles so the cache path is known
    /// without re-reading WASM bytes or re-hashing.
    resolved: Option<ResolvedCache>,
    component: Option<Component>,
}

/// Cached state from the first `acquire()` — the resolved
/// cache file path. Survives `release()` so re-acquire can
/// skip the hash computation and go straight to
/// `deserialize_file`.
struct ResolvedCache {
    cache_path: PathBuf,
}

impl CachedComponent {
    /// Create a new cached component scoped to the given
    /// gadget data directory. No disk I/O, no compilation.
    ///
    /// `gadget_data` is the gadget's home root
    /// (`<app_data_dir>/gadget-home/<gadget-id>/`). The cache
    /// directory is `<gadget_data>/compile-cache/`.
    pub fn new(
        runtime: Arc<WasmRuntime>,
        log_ctx: LogContext,
        source: Arc<dyn GadgetSource + Send + Sync>,
        gadget_data: PathBuf,
    ) -> Self {
        let gadget_id = source.manifest().gadget.id.as_str().to_string();
        let logger = log_ctx.logger(LogSource::Gadget(gadget_id.clone()));
        let cache_dir = gadget_data.join("compile-cache");
        Self {
            runtime,
            log_ctx,
            source,
            gadget_id,
            logger,
            cache_dir,
            resolved: None,
            component: None,
        }
    }

    /// Ensure the compiled component is loaded into memory
    /// and return a reference to it.
    ///
    /// First call: reads WASM bytes from the gadget source,
    /// checks the disk cache, compiles on miss, and stores
    /// the file-backed component.
    ///
    /// Subsequent calls with component still loaded: returns
    /// the existing reference immediately.
    ///
    /// After `release()`: re-deserializes from the cache file
    /// without re-reading WASM bytes or re-hashing.
    ///
    /// Emits a root `acquire` span. Use through `instantiate`
    /// for the parented variant nested under `init`.
    pub fn acquire(&mut self) -> anyhow::Result<&Component> {
        self.acquire_inner(None)
    }

    /// Drop the in-memory component to free compiled code
    /// memory. The cache file on disk is retained so the
    /// next `acquire()` re-deserializes cheaply.
    ///
    /// Idempotent — calling on an already-released component
    /// is a no-op.
    pub fn release(&mut self) {
        self.component = None;
    }

    /// Acquire the component and instantiate it into a
    /// ready-to-call gadget instance.
    ///
    /// Wraps the entire flow in an `init` span with `acquire`
    /// and `instantiate` children, so the devtools log shows
    /// a single timeline entry for "from cold to running."
    pub fn instantiate(&mut self) -> anyhow::Result<WasmGadgetInstance> {
        let init = self
            .logger
            .span("init")
            .meta("gadget_id", self.gadget_id.as_str())
            .start();

        self.acquire_inner(init.as_ref())?;
        let component = self.component.as_ref().expect("acquire just populated");

        let _inst_span = init.as_ref().and_then(|g| {
            g.child("instantiate")
                .meta("gadget_id", self.gadget_id.as_str())
                .start()
        });

        self.runtime
            .instantiate(&self.gadget_id, component, &self.log_ctx)
    }

    // =========================================================
    // Internal acquire flow
    // =========================================================

    /// Shared body for `acquire()` and the `instantiate()`
    /// path. When `parent` is `Some`, the `acquire` span
    /// nests under the caller's umbrella; when `None`, it
    /// becomes a root span.
    fn acquire_inner(&mut self, parent: Option<&SpanGuard>) -> anyhow::Result<&Component> {
        if self.component.is_some() {
            return Ok(self.component.as_ref().expect("just checked"));
        }

        let span = match parent {
            Some(p) => p.child("acquire"),
            None => self.logger.span("acquire"),
        }
        .meta("gadget_id", self.gadget_id.as_str())
        .start();

        let (component, outcome) = match &self.resolved {
            Some(resolved) => {
                // Re-acquire after release: cache file is
                // guaranteed to exist from the first acquire.
                let c = self
                    .runtime
                    .deserialize_component(&resolved.cache_path)
                    .context("re-acquire component from cache")?;
                (c, "re-acquire")
            }
            None => {
                // First acquire: resolve hash, check disk,
                // compile on miss.
                let (c, cache_path, outcome) = self.first_acquire(span.as_ref())?;
                self.resolved = Some(ResolvedCache { cache_path });
                (c, outcome)
            }
        };

        self.component = Some(component);

        if let Some(s) = span {
            s.end_with_meta(vec![("outcome".to_string(), outcome.to_string())]);
        }

        Ok(self.component.as_ref().expect("just stored"))
    }

    /// Resolve the cache path from WASM content + engine
    /// hashes, check disk for a hit, compile on miss.
    /// Returns the loaded component, the cache path, and a
    /// short outcome label for the caller's span metadata.
    ///
    /// Sub-spans (`read-wasm`, `deserialize`, `compile`,
    /// `serialize-and-write`) are emitted as children of the
    /// outer `acquire` span when one is active.
    fn first_acquire(
        &self,
        parent: Option<&SpanGuard>,
    ) -> anyhow::Result<(Component, PathBuf, &'static str)> {
        let read_span = parent.and_then(|p| p.child("read-wasm").start());
        let wasm_bytes = self
            .source
            .read_wasm()
            .context("read WASM bytes from gadget source")?;
        if let Some(s) = read_span {
            s.end_with_meta(vec![(
                "wasm_bytes".to_string(),
                wasm_bytes.len().to_string(),
            )]);
        }

        let filename = cache_filename(&self.runtime, &wasm_bytes);
        let cache_path = self.cache_dir.join(&filename);

        // Try cache hit first.
        if cache_path.exists() {
            let deser_span = parent.and_then(|p| p.child("deserialize").start());
            match self.runtime.deserialize_component(&cache_path) {
                Ok(component) => {
                    drop(deser_span);
                    prune_stale_entries(&self.cache_dir, &filename, &self.logger);
                    return Ok((component, cache_path, "cache-hit"));
                }
                Err(e) => {
                    drop(deser_span);
                    self.logger.log_with_meta(
                        LogLevel::Warn,
                        format!("corrupt compile cache, recompiling: {e:#}"),
                        vec![("gadget_id".to_string(), self.gadget_id.clone())],
                    );
                    let _ = std::fs::remove_file(&cache_path);
                }
            }
        }

        // Cache miss: compile → serialize → drop heap copy →
        // deserialize from file.
        let compile_span = parent.and_then(|p| {
            p.child("compile")
                .meta("wasm_bytes", wasm_bytes.len().to_string())
                .start()
        });
        let heap_component = self
            .runtime
            .compile(&wasm_bytes)
            .context("compile WASM component")?;
        drop(compile_span);

        let write_span = parent.and_then(|p| p.child("serialize-and-write").start());
        let serialized = WasmRuntime::serialize_component(&heap_component)
            .context("serialize compiled component")?;

        // Explicitly free the heap-allocated compiled image
        // before deserializing the file-backed version.
        drop(heap_component);

        std::fs::create_dir_all(&self.cache_dir)
            .context("create compile-cache directory")?;

        let tmp_path = cache_path.with_extension("cwasm.tmp");
        std::fs::write(&tmp_path, &serialized)
            .context("write serialized component to temp file")?;
        std::fs::rename(&tmp_path, &cache_path)
            .context("rename temp file to cache entry")?;

        if let Some(s) = write_span {
            s.end_with_meta(vec![(
                "cwasm_bytes".to_string(),
                serialized.len().to_string(),
            )]);
        }

        let deser_span = parent.and_then(|p| p.child("deserialize").start());
        let component = self
            .runtime
            .deserialize_component(&cache_path)
            .context("deserialize freshly written cache entry")?;
        drop(deser_span);

        prune_stale_entries(&self.cache_dir, &filename, &self.logger);

        Ok((component, cache_path, "cache-miss"))
    }
}

// =========================================================
// Free functions
// =========================================================

/// Compute the cache filename from WASM bytes and runtime config.
///
/// Format: `<blake3_of_wasm>.<engine_compat_hash>.cwasm`
fn cache_filename(runtime: &WasmRuntime, wasm_bytes: &[u8]) -> String {
    let wasm_hash = blake3::hash(wasm_bytes).to_hex();
    let engine_hash = runtime.precompile_compatibility_hex();
    format!("{wasm_hash}.{engine_hash}.cwasm")
}

/// Remove `.cwasm` files in `cache_dir` that don't match
/// `keep_filename`. Best-effort: errors on individual
/// removals are reported via the per-gadget logger but don't
/// fail the operation.
fn prune_stale_entries(cache_dir: &Path, keep_filename: &str, logger: &Logger) {
    let entries = match std::fs::read_dir(cache_dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if name_str == keep_filename {
            continue;
        }
        if name_str.ends_with(".cwasm") {
            if let Err(e) = std::fs::remove_file(entry.path()) {
                logger.log_with_meta(
                    LogLevel::Warn,
                    format!("failed to prune stale cache entry `{name_str}`: {e}"),
                    vec![("entry".to_string(), name_str.to_string())],
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm::logging::channel::LogContext;

    fn test_runtime() -> Arc<WasmRuntime> {
        WasmRuntime::new().expect("WasmRuntime::new")
    }

    fn minimal_source() -> Arc<dyn GadgetSource + Send + Sync> {
        use crate::wasm::source::DirectorySource;
        let fixture_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/minimal-gadget");
        Arc::new(DirectorySource::open(fixture_root).expect("open minimal fixture"))
    }

    // ---- Construction ----------------------------------------

    #[test]
    fn new_is_cheap_no_disk_io() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let gadget_data = tmp.path().join("gadget-home/test");

        let cached =
            CachedComponent::new(runtime, LogContext::test_context(), source, gadget_data.clone());

        assert!(cached.component.is_none());
        assert!(cached.resolved.is_none());
        assert!(!gadget_data.join("compile-cache").exists());
    }

    // ---- First acquire / cache miss --------------------------

    #[test]
    fn acquire_compiles_and_writes_cache_file() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let gadget_data = tmp.path().join("gadget-home/test");

        let mut cached =
            CachedComponent::new(runtime, LogContext::test_context(), source, gadget_data.clone());
        cached.acquire().expect("first acquire");

        let cache_dir = gadget_data.join("compile-cache");
        assert!(cache_dir.exists());

        let entries: Vec<_> = std::fs::read_dir(&cache_dir)
            .expect("read cache dir")
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 1);
        assert!(entries[0]
            .file_name()
            .to_str()
            .expect("utf8")
            .ends_with(".cwasm"));
    }

    #[test]
    fn acquire_creates_cache_dir_if_absent() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let gadget_data = tmp.path().join("deep/nested/gadget-home/test");

        let mut cached =
            CachedComponent::new(runtime, LogContext::test_context(), source, gadget_data.clone());
        cached.acquire().expect("acquire");

        assert!(gadget_data.join("compile-cache").exists());
    }

    #[test]
    fn acquire_with_invalid_wasm_returns_error() {
        let runtime = test_runtime();
        // The template fixture has valid WASM but let's test with a source
        // that would produce invalid bytes — we can't easily mock GadgetSource,
        // so we test the runtime's compile path directly instead.
        assert!(runtime.compile(b"not valid wasm").is_err());
    }

    // ---- Cache hit -------------------------------------------

    #[test]
    fn acquire_from_existing_cache_skips_recompile() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let gadget_data = tmp.path().join("gadget-home/test");

        // First construction + acquire: populates cache.
        let mut first = CachedComponent::new(
            Arc::clone(&runtime),
            LogContext::test_context(),
            Arc::clone(&source),
            gadget_data.clone(),
        );
        first.acquire().expect("first acquire");

        let cache_dir = gadget_data.join("compile-cache");
        let cwasm_path = std::fs::read_dir(&cache_dir)
            .expect("read")
            .filter_map(|e| e.ok())
            .next()
            .expect("one entry")
            .path();
        let mtime_before = std::fs::metadata(&cwasm_path)
            .expect("metadata")
            .modified()
            .expect("mtime");

        // Second construction + acquire: should hit cache.
        let mut second =
            CachedComponent::new(runtime, LogContext::test_context(), source, gadget_data);
        second.acquire().expect("second acquire (cache hit)");

        let mtime_after = std::fs::metadata(&cwasm_path)
            .expect("metadata")
            .modified()
            .expect("mtime");
        assert_eq!(mtime_before, mtime_after, "file should not have been rewritten");
    }

    #[test]
    fn corrupt_cache_file_triggers_recompile() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let gadget_data = tmp.path().join("gadget-home/test");

        // First acquire to get the correct filename.
        let mut cached = CachedComponent::new(
            Arc::clone(&runtime),
            LogContext::test_context(),
            Arc::clone(&source),
            gadget_data.clone(),
        );
        cached.acquire().expect("first acquire");
        let cache_path = cached.resolved.as_ref().expect("resolved").cache_path.clone();

        // Corrupt the cache file.
        std::fs::write(&cache_path, b"garbage").expect("corrupt file");

        // New construction should detect corruption and recompile.
        let mut fresh =
            CachedComponent::new(runtime, LogContext::test_context(), source, gadget_data);
        fresh.acquire().expect("acquire after corruption");

        // File should now be valid again.
        let size = std::fs::metadata(&cache_path).expect("metadata").len();
        assert!(size > 10, "cache file should be larger than garbage bytes");
    }

    // ---- Hash invalidation / stale cleanup -------------------

    #[test]
    fn stale_entries_from_old_engine_pruned() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let gadget_data = tmp.path().join("gadget-home/test");
        let cache_dir = gadget_data.join("compile-cache");

        std::fs::create_dir_all(&cache_dir).expect("create dir");
        let stale = cache_dir.join("deadbeef.0000000000000000.cwasm");
        std::fs::write(&stale, b"old").expect("write stale");

        let mut cached =
            CachedComponent::new(runtime, LogContext::test_context(), source, gadget_data);
        cached.acquire().expect("acquire");

        assert!(!stale.exists(), "stale entry should have been pruned");
    }

    #[test]
    fn unrelated_files_not_deleted() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let gadget_data = tmp.path().join("gadget-home/test");
        let cache_dir = gadget_data.join("compile-cache");

        std::fs::create_dir_all(&cache_dir).expect("create dir");
        let unrelated = cache_dir.join("notes.txt");
        std::fs::write(&unrelated, b"keep me").expect("write");

        let mut cached =
            CachedComponent::new(runtime, LogContext::test_context(), source, gadget_data);
        cached.acquire().expect("acquire");

        assert!(unrelated.exists(), "non-.cwasm files should be left alone");
    }

    // ---- acquire / release lifecycle -------------------------

    #[test]
    fn release_drops_component() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let gadget_data = tmp.path().join("gadget-home/test");

        let mut cached =
            CachedComponent::new(runtime, LogContext::test_context(), source, gadget_data);
        cached.acquire().expect("acquire");
        assert!(cached.component.is_some());

        cached.release();
        assert!(cached.component.is_none());
        assert!(cached.resolved.is_some(), "resolved path survives release");
    }

    #[test]
    fn acquire_after_release_reloads_from_disk() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let gadget_data = tmp.path().join("gadget-home/test");

        let mut cached =
            CachedComponent::new(runtime, LogContext::test_context(), source, gadget_data);
        cached.acquire().expect("first acquire");
        cached.release();
        cached.acquire().expect("re-acquire after release");
        assert!(cached.component.is_some());
    }

    #[test]
    fn release_after_release_is_idempotent() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");

        let mut cached = CachedComponent::new(
            runtime,
            LogContext::test_context(),
            source,
            tmp.path().to_path_buf(),
        );
        cached.acquire().expect("acquire");
        cached.release();
        cached.release(); // should not panic
    }

    #[test]
    fn acquire_twice_is_idempotent() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");

        let mut cached = CachedComponent::new(
            runtime,
            LogContext::test_context(),
            source,
            tmp.path().to_path_buf(),
        );
        cached.acquire().expect("first");
        cached.acquire().expect("second");
    }

    #[test]
    fn acquire_after_release_with_deleted_cache_file_fails() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let gadget_data = tmp.path().join("gadget-home/test");

        let mut cached =
            CachedComponent::new(runtime, LogContext::test_context(), source, gadget_data);
        cached.acquire().expect("first acquire");

        let cache_path = cached.resolved.as_ref().expect("resolved").cache_path.clone();
        cached.release();
        std::fs::remove_file(&cache_path).expect("delete cache file");

        assert!(
            cached.acquire().is_err(),
            "re-acquire should fail when cache file is deleted"
        );
    }

    // ---- instantiate -----------------------------------------

    #[test]
    fn instantiate_produces_working_instance() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");

        let mut cached = CachedComponent::new(
            runtime,
            LogContext::test_context(),
            source,
            tmp.path().to_path_buf(),
        );
        let instance = cached.instantiate().expect("instantiate");
        instance.enable().expect("guest enable");
    }

    // =========================================================
    // Span emission tests
    //
    // These exercise the host-side observability that
    // `CachedComponent` emits on top of the cache lifecycle.
    // They go through a real `LoggingSystem` (with the async
    // logging task) so we can assert against the items that
    // landed in the ring buffer storage.
    // =========================================================

    use crate::wasm::logging::LogItemKind;
    use crate::wasm::logging::channel::LoggingSystem;
    use crate::wasm::logging::{LogItem, LogLevel};

    /// Drain the ring buffer after letting the async logging
    /// task catch up. 50 ms is the cadence used elsewhere in
    /// the codebase for the same pattern.
    async fn drain_items(system: &LoggingSystem) -> Vec<LogItem> {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        system
            .storage()
            .lock()
            .expect("storage lock")
            .tail(usize::MAX)
    }

    /// Find the `SpanEnd` item for a given span name. Returns
    /// `None` if no span with that name has been ended yet.
    fn find_span_end<'a>(items: &'a [LogItem], name: &str) -> Option<&'a LogItem> {
        items.iter().find(|i| {
            matches!(&i.kind, LogItemKind::SpanEnd { name: n, .. } if n == name)
        })
    }

    /// Find every `SpanStart` item for a given span name.
    fn find_span_starts<'a>(items: &'a [LogItem], name: &str) -> Vec<&'a LogItem> {
        items
            .iter()
            .filter(|i| {
                matches!(&i.kind, LogItemKind::SpanStart { name: n, .. } if n == name)
            })
            .collect()
    }

    /// Read a metadata value off a `SpanEnd` item.
    fn span_end_meta<'a>(item: &'a LogItem, key: &str) -> Option<&'a str> {
        match &item.kind {
            LogItemKind::SpanEnd { metadata, .. } => metadata
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str()),
            _ => None,
        }
    }

    /// Extract `parent_id` from a `SpanStart` item.
    fn span_start_parent(item: &LogItem) -> Option<u64> {
        match &item.kind {
            LogItemKind::SpanStart { parent_id, .. } => *parent_id,
            _ => None,
        }
    }

    /// Extract `span_id` from a `SpanStart` item.
    fn span_start_id(item: &LogItem) -> u64 {
        match &item.kind {
            LogItemKind::SpanStart { span_id, .. } => *span_id,
            _ => panic!("expected SpanStart, got {:?}", item.kind),
        }
    }

    // ---- outcome metadata on the acquire span ----------------

    #[tokio::test]
    async fn acquire_emits_acquire_span_with_cache_miss_outcome() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let system = LoggingSystem::start();

        let mut cached = CachedComponent::new(
            runtime,
            system.context(),
            source,
            tmp.path().to_path_buf(),
        );
        cached.acquire().expect("first acquire (miss)");

        let items = drain_items(&system).await;
        let end = find_span_end(&items, "acquire").expect("acquire span ended");
        assert_eq!(span_end_meta(end, "outcome"), Some("cache-miss"));
        // The gadget_id metadata from the start should also be
        // merged into the end item (end metadata wins on
        // collision but here it doesn't collide, so both are
        // visible).
        assert_eq!(
            span_end_meta(end, "gadget_id"),
            Some("minimal-gadget"),
            "acquire span should carry gadget_id metadata"
        );
    }

    #[tokio::test]
    async fn acquire_emits_cache_hit_outcome_on_second_construction() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let gadget_data = tmp.path().to_path_buf();

        // Pre-populate the on-disk cache with a fresh
        // CachedComponent that uses a discarding context. The
        // actual assertions run against the *second* component,
        // which uses the captured logging system.
        {
            let mut warmup = CachedComponent::new(
                Arc::clone(&runtime),
                LogContext::test_context(),
                Arc::clone(&source),
                gadget_data.clone(),
            );
            warmup.acquire().expect("warmup acquire");
        }

        let system = LoggingSystem::start();
        let mut cached =
            CachedComponent::new(runtime, system.context(), source, gadget_data);
        cached.acquire().expect("acquire after warmup");

        let items = drain_items(&system).await;
        let end = find_span_end(&items, "acquire").expect("acquire span ended");
        assert_eq!(span_end_meta(end, "outcome"), Some("cache-hit"));
    }

    #[tokio::test]
    async fn re_acquire_after_release_reports_re_acquire_outcome() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let system = LoggingSystem::start();

        let mut cached = CachedComponent::new(
            runtime,
            system.context(),
            source,
            tmp.path().to_path_buf(),
        );
        cached.acquire().expect("first acquire");
        cached.release();
        cached.acquire().expect("re-acquire after release");

        let items = drain_items(&system).await;
        // Two `acquire` spans expected — first is cache-miss,
        // second is re-acquire. Pull both ends in order.
        let ends: Vec<&LogItem> = items
            .iter()
            .filter(|i| {
                matches!(&i.kind, LogItemKind::SpanEnd { name, .. } if name == "acquire")
            })
            .collect();
        assert_eq!(ends.len(), 2, "expected two acquire span ends");
        assert_eq!(span_end_meta(ends[0], "outcome"), Some("cache-miss"));
        assert_eq!(span_end_meta(ends[1], "outcome"), Some("re-acquire"));
    }

    // ---- sub-span coverage by path --------------------------

    #[tokio::test]
    async fn first_acquire_emits_full_sub_span_set() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let system = LoggingSystem::start();

        let mut cached = CachedComponent::new(
            runtime,
            system.context(),
            source,
            tmp.path().to_path_buf(),
        );
        cached.acquire().expect("first acquire");

        let items = drain_items(&system).await;

        // Cache miss path: every sub-span must appear exactly
        // once and parent under the outer `acquire` span.
        let acquire_start = find_span_starts(&items, "acquire");
        assert_eq!(acquire_start.len(), 1);
        let acquire_id = span_start_id(acquire_start[0]);

        for name in ["read-wasm", "compile", "serialize-and-write", "deserialize"] {
            let starts = find_span_starts(&items, name);
            assert_eq!(starts.len(), 1, "{name} should start exactly once");
            assert_eq!(
                span_start_parent(starts[0]),
                Some(acquire_id),
                "{name} should be a child of the acquire span"
            );
        }
    }

    #[tokio::test]
    async fn cache_hit_skips_compile_and_serialize_sub_spans() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let gadget_data = tmp.path().to_path_buf();

        // Warm up the cache silently.
        {
            let mut warmup = CachedComponent::new(
                Arc::clone(&runtime),
                LogContext::test_context(),
                Arc::clone(&source),
                gadget_data.clone(),
            );
            warmup.acquire().expect("warmup");
        }

        let system = LoggingSystem::start();
        let mut cached =
            CachedComponent::new(runtime, system.context(), source, gadget_data);
        cached.acquire().expect("hit acquire");

        let items = drain_items(&system).await;

        // Hit path runs read-wasm + deserialize but neither
        // compile nor serialize-and-write.
        assert_eq!(find_span_starts(&items, "read-wasm").len(), 1);
        assert_eq!(find_span_starts(&items, "deserialize").len(), 1);
        assert!(
            find_span_starts(&items, "compile").is_empty(),
            "cache hit should not emit a compile span"
        );
        assert!(
            find_span_starts(&items, "serialize-and-write").is_empty(),
            "cache hit should not emit a serialize-and-write span"
        );
    }

    #[tokio::test]
    async fn re_acquire_emits_acquire_span_only_no_sub_spans() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let system = LoggingSystem::start();

        let mut cached = CachedComponent::new(
            runtime,
            system.context(),
            source,
            tmp.path().to_path_buf(),
        );
        cached.acquire().expect("first");
        cached.release();
        // Drain the items from the first acquire so the
        // assertions below only see the re-acquire's emissions.
        let _ = drain_items(&system).await;
        system.storage().lock().expect("lock").clear();

        cached.acquire().expect("re-acquire");
        let items = drain_items(&system).await;

        // Re-acquire skips `first_acquire` entirely — no sub-
        // spans should appear, only the outer `acquire` span.
        assert_eq!(find_span_starts(&items, "acquire").len(), 1);
        for name in ["read-wasm", "compile", "serialize-and-write", "deserialize"] {
            assert!(
                find_span_starts(&items, name).is_empty(),
                "{name} should not appear on re-acquire"
            );
        }
    }

    // ---- instantiate umbrella -------------------------------

    #[tokio::test]
    async fn instantiate_emits_init_with_acquire_and_instantiate_children() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let system = LoggingSystem::start();

        let mut cached = CachedComponent::new(
            runtime,
            system.context(),
            source,
            tmp.path().to_path_buf(),
        );
        let _instance = cached.instantiate().expect("instantiate");

        let items = drain_items(&system).await;

        let init_starts = find_span_starts(&items, "init");
        assert_eq!(init_starts.len(), 1, "init umbrella span should start once");
        let init_id = span_start_id(init_starts[0]);

        let acquire_starts = find_span_starts(&items, "acquire");
        assert_eq!(acquire_starts.len(), 1);
        assert_eq!(
            span_start_parent(acquire_starts[0]),
            Some(init_id),
            "acquire should nest under init"
        );

        let instantiate_starts = find_span_starts(&items, "instantiate");
        assert_eq!(instantiate_starts.len(), 1);
        assert_eq!(
            span_start_parent(instantiate_starts[0]),
            Some(init_id),
            "instantiate should nest under init"
        );
    }

    #[tokio::test]
    async fn standalone_acquire_emits_root_span_no_parent() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let system = LoggingSystem::start();

        let mut cached = CachedComponent::new(
            runtime,
            system.context(),
            source,
            tmp.path().to_path_buf(),
        );
        cached.acquire().expect("acquire");

        let items = drain_items(&system).await;
        let starts = find_span_starts(&items, "acquire");
        assert_eq!(starts.len(), 1);
        assert_eq!(
            span_start_parent(starts[0]),
            None,
            "standalone acquire should be a root span"
        );
    }

    // ---- corrupt cache warning emission ---------------------

    #[tokio::test]
    async fn corrupt_cache_emits_warn_log_item() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let gadget_data = tmp.path().to_path_buf();

        // Populate the cache so we know its filename.
        let cache_path = {
            let mut warmup = CachedComponent::new(
                Arc::clone(&runtime),
                LogContext::test_context(),
                Arc::clone(&source),
                gadget_data.clone(),
            );
            warmup.acquire().expect("warmup");
            warmup
                .resolved
                .as_ref()
                .expect("resolved set after first acquire")
                .cache_path
                .clone()
        };

        // Corrupt the .cwasm file so deserialize fails on the
        // recompile path.
        std::fs::write(&cache_path, b"garbage").expect("corrupt file");

        let system = LoggingSystem::start();
        let mut cached =
            CachedComponent::new(runtime, system.context(), source, gadget_data);
        cached.acquire().expect("acquire after corruption");

        let items = drain_items(&system).await;
        let warn = items.iter().find(|i| {
            matches!(
                &i.kind,
                LogItemKind::Message { level: LogLevel::Warn, message, .. }
                    if message.contains("corrupt compile cache")
            )
        });
        assert!(
            warn.is_some(),
            "expected a warn log item for corrupt cache, got items: {items:#?}"
        );
        // The warning should be tagged with the gadget's
        // source so it shows up in the gadget's log view.
        assert_eq!(
            warn.unwrap().source,
            LogSource::Gadget("minimal-gadget".to_string())
        );
    }

    #[test]
    fn instantiate_after_release_reacquires() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");

        let mut cached = CachedComponent::new(
            runtime,
            LogContext::test_context(),
            source,
            tmp.path().to_path_buf(),
        );
        cached.acquire().expect("acquire");
        cached.release();

        let instance = cached.instantiate().expect("instantiate after release");
        instance.enable().expect("guest enable");
    }
}
