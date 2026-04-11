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
//   - AppDiscovery: installed application scanning, icon
//     extraction, and launching
//   - SettingsDiscovery: system settings pane scanning, icon
//     rendering, and opening
//   - WindowChrome: hiding the native title bar controls so
//     the frontend can render its own (see ADR 0034)
// =========================================================

pub mod app_discovery;
pub mod clipboard;
pub mod settings_discovery;

#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacosClipboard as PlatformClipboard;
#[cfg(target_os = "macos")]
pub use macos::MacosLauncherPanel as PlatformLauncherPanel;
#[cfg(target_os = "macos")]
pub use macos::MacosSettingsDiscovery as PlatformSettingsDiscovery;
#[cfg(target_os = "macos")]
pub use macos::MacosTray as PlatformTray;
#[cfg(target_os = "macos")]
pub use macos::MacosWindowChrome as PlatformWindowChrome;
#[cfg(target_os = "macos")]
pub use macos::MdfindDiscovery as PlatformAppDiscovery;

#[cfg(not(target_os = "macos"))]
mod fallback;
#[cfg(not(target_os = "macos"))]
pub use fallback::FallbackClipboard as PlatformClipboard;
#[cfg(not(target_os = "macos"))]
pub use fallback::FallbackDiscovery as PlatformAppDiscovery;
#[cfg(not(target_os = "macos"))]
pub use fallback::FallbackLauncherPanel as PlatformLauncherPanel;
#[cfg(not(target_os = "macos"))]
pub use fallback::FallbackSettingsDiscovery as PlatformSettingsDiscovery;
#[cfg(not(target_os = "macos"))]
pub use fallback::FallbackTray as PlatformTray;
#[cfg(not(target_os = "macos"))]
pub use fallback::FallbackWindowChrome as PlatformWindowChrome;

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

    /// Force the webview to composite its first frame while the
    /// panel remains visually hidden.
    ///
    /// WebKit may defer full compositor initialization until the
    /// window is shown for the first time, causing a brief flash
    /// of empty content on the first real show. This method makes
    /// the window briefly "visible" to the compositor without the
    /// user seeing it, so that subsequent shows are flicker-free.
    ///
    /// Should be called once after the layout dimensions are known
    /// and the frame has been set, but before the first real show.
    /// The default implementation is a no-op.
    fn warm_up(_app: &tauri::AppHandle) -> anyhow::Result<()> {
        Ok(())
    }

    /// Set the launcher window's position and size in one step.
    ///
    /// The default implementation uses separate Tauri `set_position`
    /// and `set_size` calls which may race. Platforms that support
    /// atomic frame updates (e.g. macOS `setFrame:display:`) should
    /// override this.
    fn set_frame(
        app: &tauri::AppHandle,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> anyhow::Result<()> {
        use anyhow::Context;
        use tauri::Manager;
        let win = app
            .get_webview_window("main")
            .context("get launcher window")?;
        win.set_position(tauri::LogicalPosition::new(x, y))
            .context("set launcher position")?;
        win.set_size(tauri::LogicalSize::new(width, height))
            .context("set launcher size")?;
        Ok(())
    }
}

/// Abstraction over platform-specific system tray setup.
///
/// macOS uses a template (alpha-mask) icon and distinguishes left-click
/// (toggle launcher) from right-click (context menu). Other platforms
/// may use full-color icons or different click conventions.
pub trait Tray {
    /// Build and attach the system tray icon during `setup()`.
    ///
    /// The callbacks let the caller wire up app-level actions
    /// (toggle launcher, show settings, show devtools, quit)
    /// without the platform module knowing about those concepts.
    fn build(
        app: &tauri::App,
        on_toggle: fn(&tauri::AppHandle),
        on_settings: fn(&tauri::AppHandle),
        on_devtools: fn(&tauri::AppHandle),
    ) -> anyhow::Result<()>;
}

/// Abstraction over hiding a window's native title-bar controls so
/// the frontend can render its own title bar.
///
/// On macOS this hides the three `standardWindowButton`s (close,
/// miniaturize, zoom) via the public `NSWindow` API while leaving the
/// rest of the native window (shadow, rounded corners, edge-drag
/// resize) fully intact. The alternative — `decorations(false)` —
/// strips too much on macOS and has no clean path back to native
/// rounded corners without pulling in a community plugin. See
/// [ADR 0034](../../../docs/adr/0034-custom-title-bar-with-native-controls-hidden.md).
///
/// Other platforms will implement this as needed when we add
/// cross-platform support; the fallback is a no-op so the rest of
/// the codebase can call `PlatformWindowChrome::hide_controls`
/// unconditionally.
pub trait WindowChrome {
    /// Hide the native close / minimize / zoom (or platform
    /// equivalent) buttons on the given window.
    fn hide_controls(window: &tauri::WebviewWindow) -> anyhow::Result<()>;
}
