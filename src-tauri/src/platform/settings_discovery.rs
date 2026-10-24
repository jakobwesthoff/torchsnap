// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Settings Discovery
//
// Platform-abstracted trait for discovering system settings
// panes. Each platform provides its own implementation:
//   - macOS: ExtensionKit .appex bundles + NSWorkspace icons
//   - Linux/Windows: stub (TODO)
//
// Besides listing panes, an implementation says how to open one
// (`pane_url`) and renders its icon from the platform's own
// icon data (`PlatformPaneIcon`, carried by every pane).
//
// The trait is cfg-dispatched as `PlatformSettingsDiscovery` in
// the parent module, following the same pattern as
// `PlatformAppDiscovery`.
// =========================================================

use super::PlatformPaneIcon;

/// A single discovered system settings pane.
#[derive(Clone)]
pub struct SettingsPane {
    /// Platform-specific identifier used to open this pane.
    /// macOS: `CFBundleIdentifier` (e.g., `"com.apple.Network-Settings.extension"`).
    /// Windows: `ms-settings:` URI. Linux: `.desktop` file path.
    pub id: String,

    /// User-facing display name, localized to the system language.
    pub name: String,

    /// What the platform needs to render this pane's icon. Only the
    /// platform's own implementation looks inside; the gadget hands it
    /// back to `SettingsDiscovery::icon`.
    pub icon_source: PlatformPaneIcon,

    /// Absolute filesystem path to the cached icon file. Populated
    /// by `IconCache::ensure_icon()` during gadget setup; `None`
    /// until then or if icon extraction failed.
    pub icon_path: Option<String>,
}

/// Discovers and manages system settings panes on the current
/// platform.
///
/// Implementations must be `Send + Sync` because discovery runs
/// on a background thread during gadget setup.
pub trait SettingsDiscovery: Send + Sync {
    /// Scan the system for available settings panes.
    ///
    /// Returns the list of panes that can be opened by the user.
    /// Platform-specific filtering (e.g., requiring URL scheme
    /// support on macOS) is handled by the implementation.
    fn discover(&self) -> anyhow::Result<Vec<SettingsPane>>;

    /// Get the icon of a settings pane as a decoded image, from the
    /// `icon_source` that `discover` stored on the pane.
    ///
    /// On macOS this uses `NSWorkspace.iconForFile` on the `.appex`
    /// bundle. Returns `Ok(None)` if no icon is available.
    fn icon(&self, source: &PlatformPaneIcon) -> anyhow::Result<Option<image::DynamicImage>>;

    /// The URL that opens the pane with this id (`SettingsPane::id`),
    /// for example `x-apple.systempreferences:<id>`. The gadget hands
    /// it to its opener capability, which owns the permission checks.
    ///
    /// Fails where the platform cannot open settings panes yet.
    fn pane_url(&self, pane_id: &str) -> anyhow::Result<String>;
}
