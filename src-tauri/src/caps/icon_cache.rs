// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub type IconCacheCap = crate::icons::IconCache;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StorageKey;
    use image::{DynamicImage, ImageBuffer, Rgba};
    use tempfile::TempDir;

    fn test_image() -> DynamicImage {
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(32, 32, Rgba([255, 0, 0, 255]));
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn ensure_icon_caches_on_miss() {
        let tmp = TempDir::new().expect("temp dir");
        let cache = IconCacheCap::new(tmp.path().to_path_buf());
        let key = StorageKey::new("test-app");

        let path = cache.ensure_icon("test-gadget", &key, None, || Ok(Some(test_image())));
        assert!(path.is_some(), "should return a cached path");
        let path = path.unwrap();
        assert!(path.ends_with(".webp"), "cached file should be WebP");
        assert!(
            std::path::Path::new(&path).exists(),
            "cached file should exist on disk"
        );
    }

    #[test]
    fn ensure_icon_returns_cached_on_hit() {
        let tmp = TempDir::new().expect("temp dir");
        let cache = IconCacheCap::new(tmp.path().to_path_buf());
        let key = StorageKey::new("test-app");

        let first = cache
            .ensure_icon("test-gadget", &key, None, || Ok(Some(test_image())))
            .expect("first call");

        let mut called = false;
        let second = cache
            .ensure_icon("test-gadget", &key, None, || {
                called = true;
                Ok(Some(test_image()))
            })
            .expect("second call");

        assert_eq!(first, second, "should return same path on cache hit");
        assert!(!called, "image_fn should not be called on cache hit");
    }

    #[test]
    fn ensure_icon_returns_none_when_image_fn_returns_none() {
        let tmp = TempDir::new().expect("temp dir");
        let cache = IconCacheCap::new(tmp.path().to_path_buf());
        let key = StorageKey::new("no-icon");

        let path = cache.ensure_icon("test-gadget", &key, None, || Ok(None));
        assert!(path.is_none(), "should return None when no icon available");
    }

    #[test]
    fn ensure_icon_returns_none_when_image_fn_errors() {
        let tmp = TempDir::new().expect("temp dir");
        let cache = IconCacheCap::new(tmp.path().to_path_buf());
        let key = StorageKey::new("error-icon");

        let path = cache.ensure_icon("test-gadget", &key, None, || {
            Err(anyhow::anyhow!("image extraction failed"))
        });
        assert!(path.is_none(), "should return None on error");
    }

    #[test]
    fn cleanup_removes_orphaned_entries() {
        let tmp = TempDir::new().expect("temp dir");
        let cache = IconCacheCap::new(tmp.path().to_path_buf());
        let keep_key = StorageKey::new("keep");
        let orphan_key = StorageKey::new("orphan");

        cache
            .ensure_icon("test-gadget", &keep_key, None, || Ok(Some(test_image())))
            .expect("cache keep");
        let orphan_path = cache
            .ensure_icon("test-gadget", &orphan_key, None, || Ok(Some(test_image())))
            .expect("cache orphan");

        assert!(std::path::Path::new(&orphan_path).exists());

        let mut valid_keys = std::collections::HashSet::new();
        valid_keys.insert(keep_key);
        cache.cleanup("test-gadget", &valid_keys);

        assert!(
            !std::path::Path::new(&orphan_path).exists(),
            "orphaned icon should be removed after cleanup"
        );
    }
}
