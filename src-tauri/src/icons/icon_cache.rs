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
//   - A typesafe `IconCacheKey` (blake3 hash of arbitrary input)
//   - An optional source mtime for staleness checks
//   - A lazy closure that produces a `DynamicImage` on cache miss
//
// The cache handles directory creation, WebP encoding via
// `icon_processing::process_icon`, and per-plugin cleanup.
// =========================================================

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use image::DynamicImage;

use super::icon_processing;

// =========================================================
// IconCacheKey
// =========================================================

/// Typesafe cache key — prevents accidentally passing raw
/// strings where a hashed key is expected.
///
/// Constructed via `IconCacheKey::new(input)`, which blake3-
/// hashes the input into a 64-character hex string.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IconCacheKey(String);

impl IconCacheKey {
    /// Hash an arbitrary input string to produce a cache key.
    pub fn new(input: &str) -> Self {
        Self(blake3::hash(input.as_bytes()).to_hex().to_string())
    }

    /// The hex string representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// =========================================================
// IconCache
// =========================================================

/// Disk-based icon cache shared across plugins.
///
/// Directory layout:
/// ```text
/// base_dir/
///   <plugin_id>/
///     <first-2-hex>/
///       <full-hash>.webp
/// ```
pub struct IconCache {
    base_dir: PathBuf,
}

impl IconCache {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Absolute path to the cached icon for a given plugin and key.
    ///
    /// Shards into subdirectories using the first two hex characters
    /// of the key to keep any single directory small.
    fn cache_path(&self, plugin_id: &str, key: &IconCacheKey) -> PathBuf {
        let hex = key.as_str();
        self.base_dir
            .join(plugin_id)
            .join(&hex[..2])
            .join(format!("{hex}.webp"))
    }

    /// Return the absolute path to a valid cached icon, processing
    /// and storing the result of `image_fn` on cache miss.
    ///
    /// - `plugin_id` scopes the cache subdirectory.
    /// - `key` is a blake3-hashed `IconCacheKey`.
    /// - `source_mtime` controls staleness: `Some(t)` means the
    ///   cached icon must be at least as new as `t`; `None` means
    ///   any existing cached file is valid (for immutable sources
    ///   like SF Symbols).
    /// - `image_fn` is called lazily only on cache miss. It should
    ///   return `Ok(Some(img))` on success, `Ok(None)` if no icon
    ///   is available, or `Err` on failure.
    pub fn ensure_icon(
        &self,
        plugin_id: &str,
        key: &IconCacheKey,
        source_mtime: Option<SystemTime>,
        image_fn: impl FnOnce() -> anyhow::Result<Option<DynamicImage>>,
    ) -> Option<String> {
        let icon_path = self.cache_path(plugin_id, key);

        if self.is_cache_valid(&icon_path, source_mtime) {
            return Some(icon_path.to_string_lossy().into_owned());
        }

        // Cache miss or stale — call the image provider, then
        // resize and encode to WebP via the shared processor.
        match image_fn() {
            Ok(Some(image)) => {
                let webp_bytes = match icon_processing::process_icon(image) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        eprintln!("process icon for cache key {}: {e:#}", key.as_str());
                        return None;
                    }
                };

                if let Some(parent) = icon_path.parent() {
                    if let Err(e) = fs::create_dir_all(parent) {
                        eprintln!("create icon cache dir: {e:#}");
                        return None;
                    }
                }
                if let Err(e) = fs::write(&icon_path, &webp_bytes) {
                    eprintln!("write icon cache {}: {e:#}", icon_path.display());
                    return None;
                }
                Some(icon_path.to_string_lossy().into_owned())
            }
            Ok(None) => None,
            Err(e) => {
                eprintln!("extract icon for cache key {}: {e:#}", key.as_str());
                None
            }
        }
    }

    /// Remove cached icons not referenced by any active entry.
    ///
    /// Only touches the subtree for `plugin_id`. Walks all shard
    /// subdirectories and deletes `.webp` files whose stem is not
    /// in the valid set.
    pub fn cleanup(&self, plugin_id: &str, valid_keys: &HashSet<IconCacheKey>) {
        let plugin_dir = self.base_dir.join(plugin_id);
        let shard_dirs = match fs::read_dir(&plugin_dir) {
            Ok(entries) => entries,
            // Plugin dir doesn't exist yet — nothing to clean up.
            Err(_) => return,
        };

        // Collect valid hex strings for fast lookup.
        let valid_stems: HashSet<&str> = valid_keys.iter().map(|k| k.as_str()).collect();

        for shard_entry in shard_dirs.flatten() {
            let shard_path = shard_entry.path();
            if !shard_path.is_dir() {
                continue;
            }

            let files = match fs::read_dir(&shard_path) {
                Ok(f) => f,
                Err(_) => continue,
            };

            for file_entry in files.flatten() {
                let path = file_entry.path();

                let is_cached_icon = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|ext| ext == "webp");

                if !is_cached_icon {
                    continue;
                }

                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if !valid_stems.contains(stem) {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
    }

    /// Check whether a cached icon file is still valid.
    ///
    /// When `source_mtime` is `None`, any existing file is valid
    /// (for immutable sources like system-provided symbols).
    /// When `Some(t)`, the cached icon must have an mtime >= t.
    fn is_cache_valid(&self, icon_path: &Path, source_mtime: Option<SystemTime>) -> bool {
        let icon_meta = match fs::metadata(icon_path) {
            Ok(m) => m,
            Err(_) => return false,
        };

        match source_mtime {
            None => true,
            Some(source_time) => {
                let icon_mtime = icon_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                icon_mtime >= source_time
            }
        }
    }
}
