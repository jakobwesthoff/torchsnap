// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// System Tray (Fallback)
//
// Uses a full-color icon. Left-click toggles the launcher,
// matching macOS behavior for now.
//
// TODO: Evaluate platform conventions:
//   - Windows: left-click typically opens a menu, not a toggle
//   - Linux: tray protocol varies (StatusNotifierItem vs. XEmbed),
//     some DEs prefer SVG icons or specific sizes
// =========================================================

use anyhow::Context;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::platform::Tray;

pub struct FallbackTray;

impl Tray for FallbackTray {
    fn build(
        app: &tauri::App,
        on_toggle: fn(&tauri::AppHandle),
        on_settings: fn(&tauri::AppHandle),
        on_devtools: fn(&tauri::AppHandle),
        on_check_updates: fn(&tauri::AppHandle),
    ) -> anyhow::Result<()> {
        // Opening this menu blurs the launcher, which dismisses it, so
        // by the time the item is clicked the toggle always shows it.
        let launcher_item = MenuItem::with_id(app, "launcher", "Open Launcher", true, None::<&str>)
            .context("create Open Launcher menu item")?;
        let launcher_separator =
            PredefinedMenuItem::separator(app).context("create menu separator")?;
        let updates_item = MenuItem::with_id(
            app,
            "check-updates",
            "Check for Updates...",
            true,
            None::<&str>,
        )
        .context("create Check for Updates menu item")?;
        let settings_item = MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)
            .context("create Settings menu item")?;
        let devtools_item =
            MenuItem::with_id(app, "devtools", "Developer Tools...", true, None::<&str>)
                .context("create Developer Tools menu item")?;
        let separator = PredefinedMenuItem::separator(app).context("create menu separator")?;
        let quit_item = MenuItem::with_id(app, "quit", "Quit Torchsnap", true, None::<&str>)
            .context("create Quit menu item")?;
        let menu = Menu::with_items(
            app,
            &[
                &launcher_item,
                &launcher_separator,
                &updates_item,
                &settings_item,
                &devtools_item,
                &separator,
                &quit_item,
            ],
        )
        .context("build tray menu")?;

        // Full-color icon — no template tinting on non-macOS.
        // TODO: Use a dedicated tray icon optimized for small sizes
        // and dark/light system themes on Windows and Linux.
        let tray_icon = Image::from_bytes(include_bytes!("../../../icons/32x32.png"))
            .context("load tray icon")?;

        TrayIconBuilder::new()
            .icon(tray_icon)
            .tooltip("Torchsnap")
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_menu_event(move |app, event| match event.id.as_ref() {
                "launcher" => on_toggle(app),
                "check-updates" => on_check_updates(app),
                "settings" => on_settings(app),
                "devtools" => on_devtools(app),
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
