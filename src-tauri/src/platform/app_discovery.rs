// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Application Discovery
//
// Platform-abstracted trait for discovering installed
// applications. Each platform provides its own implementation:
//   - macOS: Spotlight (mdfind) + Info.plist parsing
//   - Linux/Windows: stub (TODO)
//
// The trait is cfg-dispatched as `PlatformAppDiscovery` in
// the parent module, following the same pattern as
// `PlatformLauncherPanel` and `PlatformTray`.
// =========================================================

use std::path::PathBuf;

/// A single discovered application on the system.
#[derive(Clone)]
pub struct DiscoveredApp {
    /// Unique identifier for this application. On macOS this is the
    /// absolute path to the `.app` bundle; other platforms use their
    /// own natural key (e.g., `.desktop` file path on Linux).
    pub id: String,

    /// User-facing display name (e.g., "Safari", "Visual Studio Code").
    pub name: String,

    /// Filesystem path to the application. Used as subtitle in the
    /// result list and for launching/revealing.
    pub path: PathBuf,

    /// Platform-specific application identifier (e.g., macOS
    /// `CFBundleIdentifier` like `"com.apple.Safari"`). Used as part
    /// of the icon cache key. `None` on platforms that don't have
    /// bundle identifiers.
    pub bundle_id: Option<String>,

    /// Absolute filesystem path to the cached icon file. Populated
    /// by `IconCache::ensure_icon()` during plugin setup; `None`
    /// until then or if icon extraction failed.
    pub icon_path: Option<String>,
}

/// Discovers installed applications on the current platform.
///
/// Implementations must be `Send + Sync` because the discovery may
/// be invoked from a background thread during cache refresh.
pub trait AppDiscovery: Send + Sync {
    /// Scan the system for installed applications.
    ///
    /// Returns the full list of user-visible applications. Background
    /// agents, UI-less helpers, and other non-launchable bundles
    /// should be filtered out by the implementation.
    fn discover(&self) -> anyhow::Result<Vec<DiscoveredApp>>;
}
