// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// macOS Platform Implementation
//
// Launcher panel: converts the Tauri window into a custom
// NSPanel that accepts keyboard input without activating the
// owning process. The previously focused app retains its
// active state while the user types into the launcher.
//
// Tray: uses a template (alpha-mask) icon so macOS can tint
// it to match the menu bar appearance. Left-click toggles
// the launcher; right-click (or ctrl-click) opens the
// context menu.
// =========================================================

use anyhow::Context;
use tauri::Manager as _;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri_nspanel::ManagerExt as _;
use tauri_nspanel::WebviewWindowExt as _;
use tauri_nspanel::objc2_app_kit::NSWindowStyleMask;

use super::{LauncherPanel, Tray};

// =========================================================
// Custom NSPanel Subclass
// =========================================================

tauri_nspanel::tauri_panel! {
    panel!(TorchsnapLauncherPanel {
        config: {
            can_become_key_window: true,
            can_become_main_window: false,
            // Intentionally NOT is_floating_panel: floating panels get
            // auto-dimmed when the owning app is inactive, and ours is
            // non-activating so it would always appear dimmed. The
            // floating window *level* (set separately) handles z-order.
            is_floating_panel: false
        }
    })
}

// =========================================================
// LauncherPanel Implementation
// =========================================================

pub struct MacosLauncherPanel;

impl LauncherPanel for MacosLauncherPanel {
    fn init(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
        let panel = window
            .to_panel::<TorchsnapLauncherPanel>()
            .map_err(|e| anyhow::anyhow!("convert launcher window to NSPanel: {e:?}"))?;

        // Keep the panel visible even when the app is not active.
        // Without this, the non-activating panel would auto-hide
        // when the user clicks another app.
        panel.set_hides_on_deactivate(false);

        // Float above normal windows so the launcher is always
        // reachable, even over fullscreen apps.
        panel.set_level(tauri_nspanel::PanelLevel::Floating.into());

        // Add NonactivatingPanel to the existing style mask rather
        // than replacing it, preserving whatever Tauri configured.
        let ns_panel = panel.as_panel();
        let mask = ns_panel.styleMask();
        ns_panel.setStyleMask(mask | NSWindowStyleMask::NonactivatingPanel);

        // Appear on every Space and over fullscreen apps.
        panel.set_collection_behavior(
            tauri_nspanel::CollectionBehavior::new()
                .can_join_all_spaces()
                .full_screen_auxiliary()
                .into(),
        );

        Ok(())
    }

    fn show(app: &tauri::AppHandle) -> anyhow::Result<()> {
        let panel = app
            .get_webview_panel("main")
            .map_err(|e| anyhow::anyhow!("retrieve launcher panel: {e:?}"))?;
        panel.show_and_make_key();
        Ok(())
    }

    fn hide(app: &tauri::AppHandle) -> anyhow::Result<()> {
        let panel = app
            .get_webview_panel("main")
            .map_err(|e| anyhow::anyhow!("retrieve launcher panel: {e:?}"))?;
        panel.hide();
        Ok(())
    }

    fn is_visible(app: &tauri::AppHandle) -> anyhow::Result<bool> {
        let panel = app
            .get_webview_panel("main")
            .map_err(|e| anyhow::anyhow!("retrieve launcher panel: {e:?}"))?;
        Ok(panel.is_visible())
    }
}

// =========================================================
// Tray Implementation
//
// macOS menu bar convention: the tray icon is a template
// image (alpha mask) so the system applies the correct tint
// for light/dark menu bars automatically. Left-click toggles
// the launcher; the context menu appears on right-click.
// =========================================================

pub struct MacosTray;

impl Tray for MacosTray {
    fn build(
        app: &tauri::App,
        on_toggle: fn(&tauri::AppHandle),
        on_settings: fn(&tauri::AppHandle),
    ) -> anyhow::Result<()> {
        let settings_item =
            MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)
                .context("create Settings menu item")?;
        let separator =
            PredefinedMenuItem::separator(app).context("create menu separator")?;
        let quit_item =
            MenuItem::with_id(app, "quit", "Quit Torchsnap", true, Some("CmdOrCtrl+Q"))
                .context("create Quit menu item")?;
        let menu = Menu::with_items(app, &[&settings_item, &separator, &quit_item])
            .context("build tray menu")?;

        // Template icon: macOS tints the alpha mask to match the
        // current menu bar appearance (light or dark).
        let tray_icon = Image::from_bytes(include_bytes!("../../icons/tray-icon-template.png"))
            .context("load tray icon")?;

        TrayIconBuilder::new()
            .icon(tray_icon)
            .icon_as_template(true)
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
