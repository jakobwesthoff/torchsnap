// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Icon Extraction
//
// Platform-abstracted trait for extracting application icons
// as `image::DynamicImage` values. Each platform provides its
// own implementation:
//   - macOS: NSWorkspace → CGImage → raw pixel data
//   - Linux/Windows: stub (TODO)
//
// The trait is cfg-dispatched as `PlatformIconExtractor` in
// the parent module. Returned images are post-processed by
// `icon_processing::process_icon` (resize + WebP encode)
// before being written to disk by `IconCache`.
// =========================================================

use image::DynamicImage;
use std::path::Path;

/// Extracts application icons as decoded images.
///
/// Implementations return an `image::DynamicImage` with the full
/// resolution pixel data. Post-processing (resize, format
/// conversion) is handled by [`super::icon_processing::process_icon`].
///
/// Implementations must be `Send + Sync` because extraction runs
/// on background threads during plugin setup and cache refresh.
pub trait IconExtractor: Send + Sync {
    /// Extract the icon for the application at `app_path`.
    ///
    /// Returns `Ok(Some(image))` on success, `Ok(None)` if the
    /// platform doesn't support icon extraction, or `Err` on failure.
    fn extract(&self, app_path: &Path) -> anyhow::Result<Option<DynamicImage>>;
}
