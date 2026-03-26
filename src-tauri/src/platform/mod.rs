// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Platform Abstraction
//
// Platform-specific behavior is expressed through traits that
// each platform module implements. The traits are re-exported
// as `Platform*` type aliases via cfg dispatch so the rest of
// the codebase stays platform-agnostic.
//
// Currently abstracted:
//   - LauncherPanel: window management (NSPanel vs. regular)
//   - Tray: system tray icon, menu, and click behavior
//   - AppDiscovery: installed application scanning
//   - IconExtractor: application icon extraction
// =========================================================

pub mod app_discovery;
pub mod icon_extraction;
pub mod settings_discovery;

#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacosIconExtractor as PlatformIconExtractor;
#[cfg(target_os = "macos")]
pub use macos::MacosLauncherPanel as PlatformLauncherPanel;
#[cfg(target_os = "macos")]
pub use macos::MacosSettingsDiscovery as PlatformSettingsDiscovery;
#[cfg(target_os = "macos")]
pub use macos::MacosTray as PlatformTray;
#[cfg(target_os = "macos")]
pub use macos::MdfindDiscovery as PlatformAppDiscovery;

#[cfg(not(target_os = "macos"))]
mod fallback;
#[cfg(not(target_os = "macos"))]
pub use fallback::FallbackDiscovery as PlatformAppDiscovery;
#[cfg(not(target_os = "macos"))]
pub use fallback::FallbackIconExtractor as PlatformIconExtractor;
#[cfg(not(target_os = "macos"))]
pub use fallback::FallbackLauncherPanel as PlatformLauncherPanel;
#[cfg(not(target_os = "macos"))]
pub use fallback::FallbackSettingsDiscovery as PlatformSettingsDiscovery;
#[cfg(not(target_os = "macos"))]
pub use fallback::FallbackTray as PlatformTray;

/// Abstraction over platform-specific launcher window behavior.
///
/// Each method takes an `AppHandle` rather than `&self` because the
/// underlying platform objects (NSPanel, etc.) are retrieved from the
/// app state on each call — there is no persistent Rust-side instance.
pub trait LauncherPanel {
    /// Perform platform-specific initialization on the launcher window.
    /// Called once during `setup()` after the window is created.
    fn init(window: &tauri::WebviewWindow) -> anyhow::Result<()>
    where
        Self: Sized;

    /// Show the launcher panel and make it accept keyboard input.
    fn show(app: &tauri::AppHandle) -> anyhow::Result<()>;

    /// Hide the launcher panel.
    fn hide(app: &tauri::AppHandle) -> anyhow::Result<()>;

    /// Whether the launcher panel is currently visible.
    fn is_visible(app: &tauri::AppHandle) -> anyhow::Result<bool>;
}

/// Abstraction over platform-specific system tray setup.
///
/// macOS uses a template (alpha-mask) icon and distinguishes left-click
/// (toggle launcher) from right-click (context menu). Other platforms
/// may use full-color icons or different click conventions.
pub trait Tray {
    /// Build and attach the system tray icon during `setup()`.
    ///
    /// The two callbacks let the caller wire up app-level actions
    /// (toggle launcher, show settings, quit) without the platform
    /// module knowing about those concepts.
    fn build(
        app: &tauri::App,
        on_toggle: fn(&tauri::AppHandle),
        on_settings: fn(&tauri::AppHandle),
    ) -> anyhow::Result<()>;
}
