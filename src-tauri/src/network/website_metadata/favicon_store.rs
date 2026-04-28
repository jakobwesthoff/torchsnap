// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Favicon file storage for the website metadata service.
//!
//! Single `FileStorage` instance that handles both raster favicons
//! (converted to WebP via `icon_processing::process_icon`) and raw
//! favicons (SVG, exotic formats stored as-is). The file extension
//! is chosen at store time and recorded in the metadata DB so
//! resolution is a direct lookup — no filesystem probing needed.

use std::io::Cursor;
use std::path::PathBuf;

use image::ImageReader;

use crate::icons;
use crate::storage::{FileStorage, StorageKey};

/// File extension used for raster favicons converted to WebP.
const WEBP_EXT: &str = "webp";

// =========================================================
// FaviconStore
// =========================================================

/// Manages favicon image files on disk.
///
/// All favicons live in a single sharded directory:
/// ```text
/// <base_dir>/<first-2-hex>/<full-hash>.<ext>
/// ```
///
/// Raster images (PNG, ICO, JPEG, etc.) are decoded and re-encoded
/// as WebP for consistent sizing and format. SVGs and other formats
/// the `image` crate can't decode are stored with their original
/// extension.
pub struct FaviconStore {
    storage: FileStorage,
}

/// Result of storing a favicon, containing the information needed
/// to persist in the metadata DB and resolve later.
pub struct StoredFavicon {
    /// The `StorageKey` hex string (blake3 hash of the favicon URL).
    pub key: String,
    /// File extension used for this favicon (e.g. "webp", "svg").
    pub ext: String,
    /// Absolute filesystem path to the stored file.
    pub path: String,
}

impl FaviconStore {
    /// Create a new store rooted at the given directory.
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            storage: FileStorage::new(base_dir),
        }
    }

    /// Store a favicon image, converting rasters to WebP where possible.
    ///
    /// Returns the storage key, extension, and path on success. The
    /// caller should persist `key` and `ext` in the metadata DB so
    /// `resolve()` can find the file later without probing.
    pub fn store(
        &self,
        favicon_url: &str,
        image_bytes: &[u8],
        content_type: &str,
    ) -> Option<StoredFavicon> {
        let key = StorageKey::new(favicon_url);

        // Try to decode as a raster image and convert to WebP for
        // consistent sizing and format across all cached favicons.
        if let Some(stored) = self.try_store_as_webp(&key, image_bytes) {
            return Some(stored);
        }

        // Raster decode failed — store the raw bytes with an
        // extension derived from the content type.
        let ext = ext_from_content_type(content_type);
        self.store_raw(&key, image_bytes, &ext)
    }

    /// Resolve a previously stored favicon to its filesystem path.
    ///
    /// The `ext` must match what was returned by `store()` — this is
    /// a direct lookup, not a probe.
    pub fn resolve(&self, key_hex: &str, ext: &str) -> Option<String> {
        let key = StorageKey::from_raw(key_hex.to_string());
        let path = self.storage.resolve(&key, ext);
        if path.exists() {
            Some(path.to_string_lossy().into_owned())
        } else {
            None
        }
    }

    /// Total disk usage of all stored favicons (bytes).
    pub fn disk_usage(&self) -> u64 {
        self.storage.entries().map(|(_, _, meta)| meta.size).sum()
    }

    /// Remove all favicon files whose keys are not in the valid set.
    ///
    /// Called by the retention thread after evicting expired metadata
    /// rows from SQLite.
    pub fn cleanup(&self, valid_keys: &[String]) {
        let valid_set: std::collections::HashSet<&str> =
            valid_keys.iter().map(|k| k.as_str()).collect();

        for (key, ext, _meta) in self.storage.entries() {
            if !valid_set.contains(&*key) {
                let _ = self.storage.delete(&key, &ext);
            }
        }
    }

    /// Remove all stored favicon files.
    pub fn clear(&self) {
        for (key, ext, _meta) in self.storage.entries() {
            let _ = self.storage.delete(&key, &ext);
        }
    }

    // ---------------------------------------------------------
    // Internal helpers
    // ---------------------------------------------------------

    /// Attempt to decode image bytes as a raster format and store
    /// as WebP. Returns `None` if decoding fails (e.g. SVG input).
    fn try_store_as_webp(&self, key: &StorageKey, image_bytes: &[u8]) -> Option<StoredFavicon> {
        // Check if already cached.
        if self.storage.metadata(key, WEBP_EXT).is_some() {
            let path = self.storage.resolve(key, WEBP_EXT);
            return Some(StoredFavicon {
                key: key.to_string(),
                ext: WEBP_EXT.to_string(),
                path: path.to_string_lossy().into_owned(),
            });
        }

        let reader = ImageReader::new(Cursor::new(image_bytes))
            .with_guessed_format()
            .ok()?;
        let img = reader.decode().ok()?;
        let webp_bytes = icons::process_icon(img).ok()?;

        self.storage.store(key, &webp_bytes, WEBP_EXT).ok()?;
        let path = self.storage.resolve(key, WEBP_EXT);

        Some(StoredFavicon {
            key: key.to_string(),
            ext: WEBP_EXT.to_string(),
            path: path.to_string_lossy().into_owned(),
        })
    }

    /// Store raw bytes with the given extension.
    fn store_raw(&self, key: &StorageKey, data: &[u8], ext: &str) -> Option<StoredFavicon> {
        // Check if already cached.
        if self.storage.metadata(key, ext).is_some() {
            let path = self.storage.resolve(key, ext);
            return Some(StoredFavicon {
                key: key.to_string(),
                ext: ext.to_string(),
                path: path.to_string_lossy().into_owned(),
            });
        }

        match self.storage.store(key, data, ext) {
            Ok(()) => {
                let path = self.storage.resolve(key, ext);
                Some(StoredFavicon {
                    key: key.to_string(),
                    ext: ext.to_string(),
                    path: path.to_string_lossy().into_owned(),
                })
            }
            Err(e) => {
                eprintln!("write favicon {}.{}: {e:#}", &**key, ext);
                None
            }
        }
    }
}

/// Derive a file extension from a MIME content type.
fn ext_from_content_type(content_type: &str) -> String {
    match content_type {
        "image/svg+xml" => "svg".to_string(),
        "image/png" => "png".to_string(),
        "image/x-icon" | "image/vnd.microsoft.icon" => "ico".to_string(),
        "image/jpeg" => "jpg".to_string(),
        "image/gif" => "gif".to_string(),
        "image/webp" => "webp".to_string(),
        _ => "bin".to_string(),
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Build a tiny solid-colour PNG at runtime via the `image` crate.
    /// We don't use a hand-rolled byte literal because the WebP encoder
    /// path requires a `DynamicImage` that survives `resize` and
    /// `write_to(WebP)` — easiest to start from a real image rather
    /// than a hand-crafted minimal stream.
    fn make_png_bytes() -> Vec<u8> {
        use image::{ImageBuffer, ImageFormat, Rgba};
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(16, 16, Rgba([255, 0, 0, 255]));
        let mut bytes: Vec<u8> = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
            .expect("encode test PNG");
        bytes
    }

    const SVG_BYTES: &[u8] = b"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 1 1'/>";

    fn store(tmp: &TempDir) -> FaviconStore {
        FaviconStore::new(tmp.path().to_path_buf())
    }

    #[test]
    fn store_converts_png_to_webp() {
        let tmp = TempDir::new().expect("temp dir");
        let s = store(&tmp);
        let stored = s
            .store("https://example.test/icon.png", &make_png_bytes(), "image/png")
            .expect("png stored");
        assert_eq!(stored.ext, "webp");
        // The on-disk file exists at the resolved path.
        assert!(std::path::Path::new(&stored.path).exists());
    }

    #[test]
    fn store_keeps_svg_as_is_when_decode_fails() {
        let tmp = TempDir::new().expect("temp dir");
        let s = store(&tmp);
        let stored = s
            .store(
                "https://example.test/icon.svg",
                SVG_BYTES,
                "image/svg+xml",
            )
            .expect("svg stored");
        assert_eq!(stored.ext, "svg");
        assert!(std::path::Path::new(&stored.path).exists());
    }

    #[test]
    fn store_uses_extension_from_unknown_content_type() {
        let tmp = TempDir::new().expect("temp dir");
        let s = store(&tmp);
        // Bytes that the image crate cannot decode and that aren't SVG.
        let blob = b"not an image at all";
        let stored = s
            .store("https://example.test/x", blob, "application/octet-stream")
            .expect("raw stored");
        assert_eq!(stored.ext, "bin");
    }

    #[test]
    fn store_is_idempotent_for_same_url() {
        let tmp = TempDir::new().expect("temp dir");
        let s = store(&tmp);
        let first = s
            .store("https://example.test/i.png", &make_png_bytes(), "image/png")
            .expect("first store");
        let second = s
            .store("https://example.test/i.png", &make_png_bytes(), "image/png")
            .expect("second store");
        assert_eq!(first.key, second.key);
        assert_eq!(first.path, second.path);
    }

    #[test]
    fn resolve_returns_path_for_stored_favicon() {
        let tmp = TempDir::new().expect("temp dir");
        let s = store(&tmp);
        let stored = s
            .store("https://example.test/i.png", &make_png_bytes(), "image/png")
            .expect("stored");
        let resolved = s.resolve(&stored.key, &stored.ext).expect("resolved");
        assert_eq!(resolved, stored.path);
    }

    #[test]
    fn resolve_returns_none_for_missing_favicon() {
        let tmp = TempDir::new().expect("temp dir");
        let s = store(&tmp);
        // Valid 64-char hex but never stored.
        let key = "0".repeat(64);
        assert!(s.resolve(&key, "webp").is_none());
    }

    #[test]
    fn cleanup_removes_files_not_in_valid_set() {
        let tmp = TempDir::new().expect("temp dir");
        let s = store(&tmp);
        let keep = s
            .store("https://example.test/keep.png", &make_png_bytes(), "image/png")
            .expect("keep stored");
        let drop = s
            .store("https://example.test/drop.svg", SVG_BYTES, "image/svg+xml")
            .expect("drop stored");

        s.cleanup(&[keep.key.clone()]);

        assert!(s.resolve(&keep.key, &keep.ext).is_some());
        assert!(s.resolve(&drop.key, &drop.ext).is_none());
    }

    #[test]
    fn disk_usage_reflects_total_bytes() {
        let tmp = TempDir::new().expect("temp dir");
        let s = store(&tmp);
        assert_eq!(s.disk_usage(), 0);
        s.store("https://example.test/a.png", &make_png_bytes(), "image/png")
            .expect("stored");
        s.store("https://example.test/b.svg", SVG_BYTES, "image/svg+xml")
            .expect("stored");
        assert!(s.disk_usage() > 0);
    }

    #[test]
    fn clear_empties_directory() {
        let tmp = TempDir::new().expect("temp dir");
        let s = store(&tmp);
        let stored = s
            .store("https://example.test/a.png", &make_png_bytes(), "image/png")
            .expect("stored");
        assert!(s.resolve(&stored.key, &stored.ext).is_some());

        s.clear();
        assert!(s.resolve(&stored.key, &stored.ext).is_none());
        assert_eq!(s.disk_usage(), 0);
    }

    // =========================================================
    // ext_from_content_type — pure mapping, exhaustively covered.
    // =========================================================

    #[test]
    fn ext_from_content_type_known_types() {
        assert_eq!(ext_from_content_type("image/svg+xml"), "svg");
        assert_eq!(ext_from_content_type("image/png"), "png");
        assert_eq!(ext_from_content_type("image/x-icon"), "ico");
        assert_eq!(ext_from_content_type("image/vnd.microsoft.icon"), "ico");
        assert_eq!(ext_from_content_type("image/jpeg"), "jpg");
        assert_eq!(ext_from_content_type("image/gif"), "gif");
        assert_eq!(ext_from_content_type("image/webp"), "webp");
    }

    #[test]
    fn ext_from_content_type_unknown_falls_back_to_bin() {
        assert_eq!(ext_from_content_type("image/heic"), "bin");
        assert_eq!(ext_from_content_type("application/json"), "bin");
        assert_eq!(ext_from_content_type(""), "bin");
    }
}
