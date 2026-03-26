// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Fallback Platform Implementation (Linux / Windows)
//
// Launcher panel: uses a regular Tauri window. This works
// but lacks the non-activating behavior of macOS NSPanel —
// showing the launcher steals focus from the active app.
//
// TODO: Investigate platform-specific alternatives:
//   - Linux/Wayland: layer-shell protocol for overlay windows
//   - Linux/X11: override-redirect or _NET_WM_STATE hints
//   - Windows: WS_EX_NOACTIVATE extended window style
//
// Tray: uses a full-color icon. Left-click toggles the
// launcher, matching macOS behavior for now.
//
// TODO: Evaluate platform conventions:
//   - Windows: left-click typically opens a menu, not a toggle
//   - Linux: tray protocol varies (StatusNotifierItem vs. XEmbed),
//     some DEs prefer SVG icons or specific sizes
// =========================================================

use anyhow::Context;
use tauri::Manager;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use super::app_discovery::{AppDiscovery, DiscoveredApp};
use super::icon_extraction::IconExtractor;
use super::{LauncherPanel, Tray};

// =========================================================
// LauncherPanel Implementation
// =========================================================

pub struct FallbackLauncherPanel;

impl LauncherPanel for FallbackLauncherPanel {
    fn init(_window: &tauri::WebviewWindow) -> anyhow::Result<()> {
        // No platform-specific initialization needed for the fallback.
        // The window is already created with transparent + undecorated
        // properties by the caller.
        Ok(())
    }

    fn show(app: &tauri::AppHandle) -> anyhow::Result<()> {
        let win = app
            .get_webview_window("main")
            .context("retrieve launcher window")?;
        win.show().context("show launcher window")?;
        win.set_focus().context("focus launcher window")?;
        Ok(())
    }

    fn hide(app: &tauri::AppHandle) -> anyhow::Result<()> {
        let win = app
            .get_webview_window("main")
            .context("retrieve launcher window")?;
        win.hide().context("hide launcher window")?;
        Ok(())
    }

    fn is_visible(app: &tauri::AppHandle) -> anyhow::Result<bool> {
        let win = app
            .get_webview_window("main")
            .context("retrieve launcher window")?;
        win.is_visible().context("check launcher visibility")
    }
}

// =========================================================
// Tray Implementation
//
// Uses the same full-color icon on both Linux and Windows.
// Left-click toggles the launcher for now — this may need
// to diverge per platform once we test on real desktops.
// =========================================================

pub struct FallbackTray;

impl Tray for FallbackTray {
    fn build(
        app: &tauri::App,
        on_toggle: fn(&tauri::AppHandle),
        on_settings: fn(&tauri::AppHandle),
    ) -> anyhow::Result<()> {
        let settings_item = MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)
            .context("create Settings menu item")?;
        let separator = PredefinedMenuItem::separator(app).context("create menu separator")?;
        let quit_item = MenuItem::with_id(app, "quit", "Quit Torchsnap", true, None::<&str>)
            .context("create Quit menu item")?;
        let menu = Menu::with_items(app, &[&settings_item, &separator, &quit_item])
            .context("build tray menu")?;

        // Full-color icon — no template tinting on non-macOS.
        // TODO: Use a dedicated tray icon optimized for small sizes
        // and dark/light system themes on Windows and Linux.
        let tray_icon =
            Image::from_bytes(include_bytes!("../../icons/32x32.png")).context("load tray icon")?;

        TrayIconBuilder::new()
            .icon(tray_icon)
            .tooltip("Torchsnap")
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_menu_event(move |app, event| match event.id.as_ref() {
                "settings" => on_settings(app),
                "quit" => app.exit(0),
                _ => {}
            })
            .on_tray_icon_event(move |tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    on_toggle(tray.app_handle());
                }
            })
            .build(app)
            .context("build tray icon")?;

        Ok(())
    }
}

// =========================================================
// Application Discovery
//
// Stub implementation. Returns an empty list until platform-
// specific discovery is implemented for Linux and Windows.
// =========================================================

pub struct FallbackDiscovery;

impl AppDiscovery for FallbackDiscovery {
    fn discover(&self) -> anyhow::Result<Vec<DiscoveredApp>> {
        // TODO: Linux — scan .desktop files from XDG data dirs
        // TODO: Windows — enumerate Start Menu shortcuts / shell:AppsFolder
        Ok(Vec::new())
    }
}

// =========================================================
// Icon Extraction
//
// Stub implementation. Returns None until platform-specific
// icon extraction is implemented for Linux and Windows.
// =========================================================

pub struct FallbackIconExtractor;

impl IconExtractor for FallbackIconExtractor {
    fn extract(&self, _app_path: &std::path::Path) -> anyhow::Result<Option<Vec<u8>>> {
        // TODO: Linux — extract from icon theme based on .desktop Icon= field
        // TODO: Windows — extract from PE resources or shortcut targets
        Ok(None)
    }
}
