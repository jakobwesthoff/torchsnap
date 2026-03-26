// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Launcher Panel (macOS)
//
// Converts the Tauri window into a custom NSPanel that accepts
// keyboard input without activating the owning process. The
// previously focused app retains its active state while the
// user types into the launcher.
// =========================================================

use tauri::Manager as _;
use tauri_nspanel::ManagerExt as _;
use tauri_nspanel::WebviewWindowExt as _;
use tauri_nspanel::objc2_app_kit::NSWindowStyleMask;

use crate::platform::LauncherPanel;

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
