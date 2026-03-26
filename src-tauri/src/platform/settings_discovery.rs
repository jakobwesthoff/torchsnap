// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Settings Discovery
//
// Platform-abstracted trait for discovering system settings
// panes. Each platform provides its own implementation:
//   - macOS: ExtensionKit .appex bundles + SF Symbols
//   - Linux/Windows: stub (TODO)
//
// The trait is cfg-dispatched as `PlatformSettingsDiscovery` in
// the parent module, following the same pattern as
// `PlatformAppDiscovery` and `PlatformIconExtractor`.
// =========================================================

/// A single discovered system settings pane.
#[derive(Clone)]
pub struct SettingsPane {
    /// Platform-specific identifier used to open this pane.
    /// macOS: `CFBundleIdentifier` (e.g., `"com.apple.Network-Settings.extension"`).
    /// Windows: `ms-settings:` URI. Linux: `.desktop` file path.
    pub id: String,

    /// User-facing display name, localized to the system language.
    pub name: String,

    /// Platform-specific icon reference that the plugin resolves
    /// to an image via platform helpers. On macOS this is an
    /// SF Symbol name (e.g., `"network"`); on other platforms it
    /// may be an icon theme name or path.
    pub icon_source: Option<String>,

    /// Absolute filesystem path to the cached icon file. Populated
    /// by `IconCache::ensure_icon()` during plugin setup; `None`
    /// until then or if icon rendering failed.
    pub icon_path: Option<String>,
}

/// Discovers and manages system settings panes on the current
/// platform.
///
/// Implementations must be `Send + Sync` because discovery runs
/// on a background thread during plugin setup.
pub trait SettingsDiscovery: Send + Sync {
    /// Scan the system for available settings panes.
    ///
    /// Returns the list of panes that can be opened by the user.
    /// Platform-specific filtering (e.g., requiring URL scheme
    /// support on macOS) is handled by the implementation.
    fn discover(&self) -> anyhow::Result<Vec<SettingsPane>>;

    /// Render the icon for a settings pane from its platform-
    /// specific `icon_source`.
    ///
    /// On macOS this rasterizes an SF Symbol. On other platforms
    /// it may load from an icon theme or return `Ok(None)` to
    /// use the fallback icon.
    fn render_icon(
        &self,
        icon_source: &str,
    ) -> anyhow::Result<Option<image::DynamicImage>>;

    /// Open a settings pane by its platform-specific ID.
    fn open(&self, pane_id: &str, app: &tauri::AppHandle) -> anyhow::Result<()>;
}
