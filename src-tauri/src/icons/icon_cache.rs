// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Icon Cache
//
// Generic, multi-plugin disk cache for processed icon images.
// Each plugin scopes its icons under a separate subdirectory,
// and files are sharded by the first two hex characters of
// the cache key to avoid large flat directories.
//
// Callers provide:
//   - A plugin ID (directory scope)
//   - A typesafe `StorageKey` (blake3 hash of arbitrary input)
//   - An optional source mtime for staleness checks
//   - A lazy closure that produces a `DynamicImage` on cache miss
//
// File I/O is delegated to `FileStorage` from the storage
// module. The icon-specific logic (WebP encoding, per-plugin
// subdirectories, mtime-based invalidation) stays here.
// =========================================================

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::SystemTime;

use image::DynamicImage;

use crate::storage::{FileStorage, StorageKey};

use super::icon_processing;

/// Extension used for all cached icon files.
const ICON_EXT: &str = "webp";

// =========================================================
// IconCache
// =========================================================

/// Disk-based icon cache shared across gadgets.
///
/// Directory layout (managed by `FileStorage`):
/// ```text
/// base_dir/
///   <gadget_id>/
///     <first-2-hex>/
///       <full-hash>.webp
/// ```
pub struct IconCache {
    storage: FileStorage,
}

impl IconCache {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            storage: FileStorage::new(base_dir),
        }
    }

    /// Return the absolute path to a valid cached icon, processing
    /// and storing the result of `image_fn` on cache miss.
    ///
    /// - `gadget_id` scopes the cache subdirectory.
    /// - `key` is a blake3-hashed `StorageKey`.
    /// - `source_mtime` controls staleness: `Some(t)` means the
    ///   cached icon must be at least as new as `t`; `None` means
    ///   any existing cached file is valid (for immutable sources
    ///   like SF Symbols).
    /// - `image_fn` is called lazily only on cache miss. It should
    ///   return `Ok(Some(img))` on success, `Ok(None)` if no icon
    ///   is available, or `Err` on failure.
    pub fn ensure_icon(
        &self,
        gadget_id: &str,
        key: &StorageKey,
        source_mtime: Option<SystemTime>,
        image_fn: impl FnOnce() -> anyhow::Result<Option<DynamicImage>>,
    ) -> Option<String> {
        let gadget_storage = self.storage.scoped(gadget_id);

        if self.is_cache_valid(&gadget_storage, key, source_mtime) {
            let icon_path = gadget_storage.resolve(key, ICON_EXT);
            return Some(icon_path.to_string_lossy().into_owned());
        }

        // Cache miss or stale — call the image provider, then
        // resize and encode to WebP via the shared processor.
        match image_fn() {
            Ok(Some(image)) => {
                let webp_bytes = match icon_processing::process_icon(image) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        eprintln!("process icon for cache key {}: {e:#}", &**key);
                        return None;
                    }
                };

                match gadget_storage.store(key, &webp_bytes, ICON_EXT) {
                    Ok(()) => {
                        let icon_path = gadget_storage.resolve(key, ICON_EXT);
                        Some(icon_path.to_string_lossy().into_owned())
                    }
                    Err(e) => {
                        eprintln!("write icon cache: {e:#}");
                        None
                    }
                }
            }
            Ok(None) => None,
            Err(e) => {
                eprintln!("extract icon for cache key {}: {e:#}", &**key);
                None
            }
        }
    }

    /// Remove cached icons not referenced by any active entry.
    ///
    /// Only touches the subtree for `gadget_id`. Walks all shard
    /// subdirectories and deletes files whose key is not in the
    /// valid set.
    pub fn cleanup(&self, gadget_id: &str, valid_keys: &HashSet<StorageKey>) {
        let gadget_storage = self.storage.scoped(gadget_id);

        // Collect valid hex strings for fast lookup.
        let valid_stems: HashSet<&str> = valid_keys.iter().map(|k| &**k).collect();

        for (key, ext, _meta) in gadget_storage.entries() {
            if !valid_stems.contains(&*key) {
                // Best effort — ignore errors during cleanup.
                let _ = gadget_storage.delete(&key, &ext);
            }
        }
    }

    /// Check whether a cached icon file is still valid.
    ///
    /// When `source_mtime` is `None`, any existing file is valid
    /// (for immutable sources like system-provided symbols).
    /// When `Some(t)`, the cached icon must have an mtime >= t.
    fn is_cache_valid(
        &self,
        storage: &FileStorage,
        key: &StorageKey,
        source_mtime: Option<SystemTime>,
    ) -> bool {
        let entry_meta = match storage.metadata(key, ICON_EXT) {
            Some(m) => m,
            None => return false,
        };

        match source_mtime {
            None => true,
            Some(source_time) => entry_meta.modified >= source_time,
        }
    }
}
