// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Icon Extraction
//
// Platform-abstracted trait for extracting application icons
// as PNG bytes. Each platform provides its own implementation:
//   - macOS: NSWorkspace icon API
//   - Linux/Windows: stub (TODO)
//
// The trait is cfg-dispatched as `PlatformIconExtractor` in
// the parent module. The returned PNG bytes are written to
// disk by `IconCache` — extractors don't manage caching.
// =========================================================

use std::path::Path;

/// Extracts application icons as PNG image bytes.
///
/// Implementations must be `Send + Sync` because extraction runs
/// on background threads during plugin setup and cache refresh.
pub trait IconExtractor: Send + Sync {
    /// Extract the icon for the application at `app_path` as PNG bytes.
    ///
    /// Returns `Ok(Some(bytes))` on success, `Ok(None)` if the platform
    /// doesn't support icon extraction, or `Err` on failure.
    fn extract(&self, app_path: &Path) -> anyhow::Result<Option<Vec<u8>>>;
}
