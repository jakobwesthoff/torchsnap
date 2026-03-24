// =========================================================
// Platform Abstraction for Launcher Window Behavior
//
// The launcher needs platform-specific window management to
// feel native. On macOS this means an NSPanel that receives
// keyboard input without activating the owning process. On
// Linux and Windows the best we can do (for now) is a regular
// Tauri window — functional but steals focus from the active
// app.
//
// Each platform module implements `LauncherPanel` and is
// re-exported as `PlatformLauncherPanel` via cfg dispatch.
// =========================================================

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacosLauncherPanel as PlatformLauncherPanel;

#[cfg(not(target_os = "macos"))]
mod fallback;
#[cfg(not(target_os = "macos"))]
pub use fallback::FallbackLauncherPanel as PlatformLauncherPanel;

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
