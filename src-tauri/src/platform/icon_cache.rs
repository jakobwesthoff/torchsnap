// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Icon Cache
//
// Manages a disk-based cache of application icons. Platform
// extractors provide raw image bytes, which are resized and
// encoded to WebP by `icon_processing::process_icon` before
// being stored on disk with blake3-hashed filenames.
//
// Cache invalidation is mtime-based: if the cached icon is
// newer than the app bundle, it's considered valid. Orphaned
// icons (from uninstalled apps) are cleaned up after each
// full discovery pass.
// =========================================================

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::app_discovery::DiscoveredApp;
use super::icon_extraction::IconExtractor;
use super::icon_processing;

pub struct IconCache {
    cache_dir: PathBuf,
    extractor: Box<dyn IconExtractor>,
}

impl IconCache {
    pub fn new(cache_dir: PathBuf, extractor: Box<dyn IconExtractor>) -> Self {
        Self {
            cache_dir,
            extractor,
        }
    }

    /// Compute the cache key for a discovered app: blake3 hash of
    /// `"{path}:{bundle_id_or_empty}"`.
    pub fn cache_key(app: &DiscoveredApp) -> String {
        let input = format!(
            "{}:{}",
            app.path.display(),
            app.bundle_id.as_deref().unwrap_or("")
        );
        blake3::hash(input.as_bytes()).to_hex().to_string()
    }

    /// Absolute path to the cached icon for a given cache key.
    fn cache_path(&self, key: &str) -> PathBuf {
        self.cache_dir.join(format!("{key}.webp"))
    }

    /// Return the absolute path to a valid cached icon, extracting
    /// and processing it first if needed.
    ///
    /// Returns `None` if extraction fails or the platform doesn't
    /// support icon extraction.
    pub fn ensure_icon(&self, app: &DiscoveredApp) -> Option<String> {
        let key = Self::cache_key(app);
        let icon_path = self.cache_path(&key);

        // If the cached icon exists and is at least as new as the
        // app bundle, skip re-extraction. This handles the common
        // case where the cache is already populated.
        if self.is_cache_valid(&icon_path, &app.path) {
            return Some(icon_path.to_string_lossy().into_owned());
        }

        // Cache miss or stale — extract the icon as a DynamicImage,
        // then resize and encode to WebP via the shared processor.
        match self.extractor.extract(&app.path) {
            Ok(Some(extracted_image)) => {
                let webp_bytes = match icon_processing::process_icon(extracted_image) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        eprintln!("process icon for {}: {e:#}", app.path.display());
                        return None;
                    }
                };

                if let Err(e) = fs::create_dir_all(&self.cache_dir) {
                    eprintln!("create icon cache dir: {e:#}");
                    return None;
                }
                if let Err(e) = fs::write(&icon_path, &webp_bytes) {
                    eprintln!("write icon cache {}: {e:#}", icon_path.display());
                    return None;
                }
                Some(icon_path.to_string_lossy().into_owned())
            }
            Ok(None) => None,
            Err(e) => {
                eprintln!("extract icon for {}: {e:#}", app.path.display());
                None
            }
        }
    }

    /// Remove cached icons not referenced by any discovered app.
    ///
    /// Call this after a full discovery pass with the set of valid
    /// cache keys. Any `.webp` file in the cache dir whose stem is
    /// not in the valid set gets deleted.
    pub fn cleanup(&self, valid_keys: &HashSet<String>) {
        let entries = match fs::read_dir(&self.cache_dir) {
            Ok(e) => e,
            // Cache dir doesn't exist yet — nothing to clean up.
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();

            let is_cached_icon = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| ext == "webp" || ext == "png");

            if !is_cached_icon {
                continue;
            }

            if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                && !valid_keys.contains(stem)
            {
                let _ = fs::remove_file(&path);
            }
        }
    }

    /// Check whether a cached icon file exists and is at least as
    /// new as the source app bundle.
    fn is_cache_valid(&self, icon_path: &Path, app_path: &Path) -> bool {
        let (icon_meta, app_meta) = match (fs::metadata(icon_path), fs::metadata(app_path)) {
            (Ok(i), Ok(a)) => (i, a),
            _ => return false,
        };

        let icon_mtime = icon_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let app_mtime = app_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);

        icon_mtime >= app_mtime
    }
}
