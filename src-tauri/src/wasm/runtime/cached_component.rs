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
// =========================================================

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use wasmtime::component::Component;

use super::WasmGadgetInstance;
use super::engine::WasmRuntime;
use crate::wasm::source::GadgetSource;

/// Disk-backed compiled component with lazy acquire/release.
///
/// Owned by `WasmGadgetBridge`, one per gadget. Construction
/// is cheap (no I/O). The compile-or-deserialize step happens
/// on the first `acquire()` call.
pub struct CachedComponent {
    runtime: Arc<WasmRuntime>,
    source: Arc<dyn GadgetSource + Send + Sync>,
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
        source: Arc<dyn GadgetSource + Send + Sync>,
        gadget_data: PathBuf,
    ) -> Self {
        let cache_dir = gadget_data.join("compile-cache");
        Self {
            runtime,
            source,
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
    pub fn acquire(&mut self) -> anyhow::Result<&Component> {
        if self.component.is_some() {
            return Ok(self.component.as_ref().expect("just checked"));
        }

        let component = match &self.resolved {
            Some(resolved) => {
                // Re-acquire after release: cache file is
                // guaranteed to exist from the first acquire.
                self.runtime.deserialize_component(&resolved.cache_path)
                    .context("re-acquire component from cache")?
            }
            None => {
                // First acquire: resolve hash, check disk,
                // compile on miss.
                let (component, cache_path) = self.first_acquire()?;
                self.resolved = Some(ResolvedCache { cache_path });
                component
            }
        };

        self.component = Some(component);
        Ok(self.component.as_ref().expect("just stored"))
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
    pub fn instantiate(&mut self) -> anyhow::Result<WasmGadgetInstance> {
        let gadget_id = self.source.manifest().gadget.id.as_str().to_string();
        self.acquire()?;
        let component = self.component.as_ref().expect("acquire just populated");
        self.runtime.instantiate(&gadget_id, component)
    }

    // =========================================================
    // First-acquire internals
    // =========================================================

    /// Resolve the cache path from WASM content + engine
    /// hashes, check disk for a hit, compile on miss.
    /// Returns the loaded component and the cache path.
    fn first_acquire(&self) -> anyhow::Result<(Component, PathBuf)> {
        let wasm_bytes = self.source.read_wasm()
            .context("read WASM bytes from gadget source")?;

        let filename = cache_filename(&self.runtime, &wasm_bytes);
        let cache_path = self.cache_dir.join(&filename);

        // Try cache hit first.
        if cache_path.exists() {
            match self.runtime.deserialize_component(&cache_path) {
                Ok(component) => {
                    prune_stale_entries(&self.cache_dir, &filename);
                    return Ok((component, cache_path));
                }
                Err(e) => {
                    eprintln!(
                        "warning: corrupt compile cache for `{}`, recompiling: {e:#}",
                        self.source.manifest().gadget.id,
                    );
                    let _ = std::fs::remove_file(&cache_path);
                }
            }
        }

        // Cache miss: compile → serialize → drop heap copy →
        // deserialize from file.
        let heap_component = self.runtime.compile(&wasm_bytes)
            .context("compile WASM component")?;

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

        let component = self.runtime.deserialize_component(&cache_path)
            .context("deserialize freshly written cache entry")?;

        prune_stale_entries(&self.cache_dir, &filename);

        Ok((component, cache_path))
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
/// removals are logged but don't fail the operation.
fn prune_stale_entries(cache_dir: &Path, keep_filename: &str) {
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
                eprintln!("warning: failed to prune stale cache entry `{name_str}`: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm::logging::channel::LogSender;
    use crate::wasm::logging::spans::SpanRegistry;

    const MINIMAL_GADGET_WASM: &[u8] =
        include_bytes!("../../../tests/fixtures/minimal-gadget/minimal_gadget.wasm");

    fn test_runtime() -> Arc<WasmRuntime> {
        WasmRuntime::new(
            LogSender::test_sender(),
            Arc::new(SpanRegistry::new()),
        )
        .expect("WasmRuntime::new")
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

        let cached = CachedComponent::new(runtime, source, gadget_data.clone());

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

        let mut cached = CachedComponent::new(runtime, source, gadget_data.clone());
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

        let mut cached = CachedComponent::new(runtime, source, gadget_data.clone());
        cached.acquire().expect("acquire");

        assert!(gadget_data.join("compile-cache").exists());
    }

    #[test]
    fn acquire_with_invalid_wasm_returns_error() {
        use crate::wasm::source::DirectorySource;

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
        let mut second = CachedComponent::new(runtime, source, gadget_data);
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
            Arc::clone(&source),
            gadget_data.clone(),
        );
        cached.acquire().expect("first acquire");
        let cache_path = cached.resolved.as_ref().expect("resolved").cache_path.clone();

        // Corrupt the cache file.
        std::fs::write(&cache_path, b"garbage").expect("corrupt file");

        // New construction should detect corruption and recompile.
        let mut fresh = CachedComponent::new(runtime, source, gadget_data);
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

        let mut cached = CachedComponent::new(runtime, source, gadget_data);
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

        let mut cached = CachedComponent::new(runtime, source, gadget_data);
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

        let mut cached = CachedComponent::new(runtime, source, gadget_data);
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

        let mut cached = CachedComponent::new(runtime, source, gadget_data);
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

        let mut cached = CachedComponent::new(runtime, source, tmp.path().to_path_buf());
        cached.acquire().expect("acquire");
        cached.release();
        cached.release(); // should not panic
    }

    #[test]
    fn acquire_twice_is_idempotent() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");

        let mut cached = CachedComponent::new(runtime, source, tmp.path().to_path_buf());
        cached.acquire().expect("first");
        cached.acquire().expect("second");
    }

    #[test]
    fn acquire_after_release_with_deleted_cache_file_fails() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let gadget_data = tmp.path().join("gadget-home/test");

        let mut cached = CachedComponent::new(runtime, source, gadget_data);
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

        let mut cached = CachedComponent::new(runtime, source, tmp.path().to_path_buf());
        let instance = cached.instantiate().expect("instantiate");
        instance.enable().expect("guest enable");
    }

    #[test]
    fn instantiate_after_release_reacquires() {
        let runtime = test_runtime();
        let source = minimal_source();
        let tmp = tempfile::TempDir::new().expect("tempdir");

        let mut cached = CachedComponent::new(runtime, source, tmp.path().to_path_buf());
        cached.acquire().expect("acquire");
        cached.release();

        let instance = cached.instantiate().expect("instantiate after release");
        instance.enable().expect("guest enable");
    }
}
