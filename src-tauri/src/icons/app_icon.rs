// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// App Icon Resolver
//
// Turns a platform-native application identifier (a bundle
// identifier on macOS) into the path of a cached, host-rendered
// icon file. Sits between the WASM bridge's `ResolveAppIcon`
// trait and the shared `IconCache`: it looks up the
// application's on-disk location via the platform layer, then
// asks the cache for an icon, extracting one only on a cold
// miss or after the application has been updated.
// =========================================================

use std::sync::Arc;

use image::DynamicImage;

use crate::storage::StorageKey;

use super::IconCache;

/// Resolves application identifiers to cached icon paths on
/// behalf of WASM gadgets holding the `icon-cache` capability.
pub struct AppIconResolver {
    cache: Arc<IconCache>,
}

impl AppIconResolver {
    pub fn new(cache: Arc<IconCache>) -> Self {
        Self { cache }
    }
}

/// Core resolution logic, parameterized over the platform lookup
/// and extraction steps so tests can inject fakes without
/// touching real application bundles.
///
/// The cache key combines the identifier with its resolved path
/// (mirroring the app-launcher's `path:bundle_id` key) so that an
/// application relocated to a new path re-extracts its icon;
/// mtime-based staleness in `IconCache` handles in-place updates.
fn resolve_with(
    cache: &IconCache,
    gadget_id: &str,
    identifier: &str,
    lookup: impl FnOnce(&str) -> Option<String>,
    extract: impl FnOnce(&str) -> anyhow::Result<Option<DynamicImage>>,
) -> Option<String> {
    let path = lookup(identifier)?;
    let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let key = StorageKey::new(&format!("app-icon:{identifier}:{path}"));
    cache.ensure_icon(gadget_id, &key, mtime, || extract(&path))
}

impl crate::wasm::bindings::ResolveAppIcon for AppIconResolver {
    fn resolve(&self, gadget_id: &str, identifier: &str) -> Option<String> {
        resolve_with(
            &self.cache,
            gadget_id,
            identifier,
            crate::platform::app_path_for_identifier,
            crate::platform::icon_image_for_path,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use tempfile::TempDir;

    fn test_image() -> DynamicImage {
        let img: image::ImageBuffer<image::Rgba<u8>, Vec<u8>> =
            image::ImageBuffer::from_pixel(8, 8, image::Rgba([10, 20, 30, 255]));
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn resolve_with_returns_none_when_lookup_fails() {
        let tmp = TempDir::new().expect("temp dir");
        let cache = IconCache::new(tmp.path().to_path_buf());
        let extract_calls = Cell::new(0);

        let result = resolve_with(
            &cache,
            "test-gadget",
            "com.example.missing",
            |_| None,
            |_| {
                extract_calls.set(extract_calls.get() + 1);
                Ok(Some(test_image()))
            },
        );

        assert!(result.is_none());
        assert_eq!(
            extract_calls.get(),
            0,
            "extract must not run when lookup fails"
        );
    }

    #[test]
    fn resolve_with_returns_none_when_extract_errors() {
        let tmp = TempDir::new().expect("temp dir");
        let cache = IconCache::new(tmp.path().to_path_buf());

        let result = resolve_with(
            &cache,
            "test-gadget",
            "com.example.app",
            |_| Some("/Applications/Example.app".to_string()),
            |_| Err(anyhow::anyhow!("extraction failed")),
        );

        assert!(result.is_none());
    }

    #[test]
    fn resolve_with_returns_none_when_extract_finds_no_icon() {
        let tmp = TempDir::new().expect("temp dir");
        let cache = IconCache::new(tmp.path().to_path_buf());

        let result = resolve_with(
            &cache,
            "test-gadget",
            "com.example.app",
            |_| Some("/Applications/Example.app".to_string()),
            |_| Ok(None),
        );

        assert!(result.is_none());
    }

    #[test]
    fn resolve_with_caches_on_cold_miss() {
        let tmp = TempDir::new().expect("temp dir");
        let cache = IconCache::new(tmp.path().to_path_buf());
        let extract_calls = Cell::new(0);

        let result = resolve_with(
            &cache,
            "test-gadget",
            "com.example.app",
            |_| Some("/Applications/Example.app".to_string()),
            |_| {
                extract_calls.set(extract_calls.get() + 1);
                Ok(Some(test_image()))
            },
        );

        let path = result.expect("should resolve to a cached path");
        assert!(path.ends_with(".webp"), "cached file should be WebP");
        assert!(
            std::path::Path::new(&path).is_absolute(),
            "cached path should be absolute"
        );
        assert!(
            std::path::Path::new(&path).exists(),
            "cached file should exist on disk"
        );
        assert_eq!(extract_calls.get(), 1);
    }

    #[test]
    fn resolve_with_reuses_cache_on_warm_hit() {
        let tmp = TempDir::new().expect("temp dir");
        let cache = IconCache::new(tmp.path().to_path_buf());
        let extract_calls = Cell::new(0);

        let first = resolve_with(
            &cache,
            "test-gadget",
            "com.example.app",
            |_| Some("/Applications/Example.app".to_string()),
            |_| {
                extract_calls.set(extract_calls.get() + 1);
                Ok(Some(test_image()))
            },
        )
        .expect("first resolve should populate the cache");

        let second = resolve_with(
            &cache,
            "test-gadget",
            "com.example.app",
            |_| Some("/Applications/Example.app".to_string()),
            |_| {
                extract_calls.set(extract_calls.get() + 1);
                Ok(Some(test_image()))
            },
        )
        .expect("second resolve should hit the cache");

        assert_eq!(first, second, "warm hit should return the same path");
        assert_eq!(
            extract_calls.get(),
            1,
            "extract must not run again on a warm hit"
        );
    }

    #[test]
    fn resolve_with_different_identifiers_produce_different_files() {
        let tmp = TempDir::new().expect("temp dir");
        let cache = IconCache::new(tmp.path().to_path_buf());

        let path_a = resolve_with(
            &cache,
            "test-gadget",
            "com.example.a",
            |_| Some("/Applications/A.app".to_string()),
            |_| Ok(Some(test_image())),
        )
        .expect("resolve a");

        let path_b = resolve_with(
            &cache,
            "test-gadget",
            "com.example.b",
            |_| Some("/Applications/B.app".to_string()),
            |_| Ok(Some(test_image())),
        )
        .expect("resolve b");

        assert_ne!(
            path_a, path_b,
            "different identifiers should cache to different files"
        );
    }

    /// End-to-end check against the real macOS platform layer:
    /// resolves Finder's bundle identifier, extracts its real
    /// icon, and caches it through a temp-dir-backed `IconCache`.
    #[cfg(target_os = "macos")]
    #[test]
    fn app_icon_resolver_resolves_finder_via_real_platform_functions() {
        use crate::wasm::bindings::ResolveAppIcon;

        let tmp = TempDir::new().expect("temp dir");
        let cache = Arc::new(IconCache::new(tmp.path().to_path_buf()));
        let resolver = AppIconResolver::new(cache);

        let path = resolver
            .resolve("test-gadget", "com.apple.finder")
            .expect("Finder icon should resolve on this machine");

        assert!(path.ends_with(".webp"));
        assert!(std::path::Path::new(&path).exists());
    }
}
