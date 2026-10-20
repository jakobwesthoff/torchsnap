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

    fn warm_up(app: &tauri::AppHandle) -> anyhow::Result<()> {
        let panel = app
            .get_webview_panel("main")
            .map_err(|e| anyhow::anyhow!("retrieve launcher panel: {e:?}"))?;

        let ns_panel = panel.as_panel();

        // Make the panel invisible to the user but "visible" to
        // WebKit's compositor so it renders the first frame.
        ns_panel.setAlphaValue(0.0);
        panel.show_and_make_key();
        panel.hide();
        ns_panel.setAlphaValue(1.0);

        Ok(())
    }

    /// Atomic position + size via `NSWindow.setFrame(_:display:)`.
    ///
    /// Avoids the race between separate `set_position` and
    /// `set_size` calls that can cause a visible flash when the
    /// window is resized from 1×1 on first show.
    ///
    /// Coordinates are in logical pixels with a top-left origin
    /// (matching Tauri's convention). The flip to macOS bottom-left
    /// coordinates uses the primary screen as reference, since
    /// global display coordinates are relative to its bottom-left
    /// corner.
    ///
    /// Dispatches to the main thread if not already there, since
    /// AppKit calls require it.
    fn set_frame(
        app: &tauri::AppHandle,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> anyhow::Result<()> {
        use anyhow::Context;

        let handle = app.clone();
        app.run_on_main_thread(move || {
            use objc2_app_kit::NSScreen;
            use objc2_foundation::NSRect;
            use tauri_nspanel::objc2::MainThreadMarker;

            let Ok(panel) = handle.get_webview_panel("main") else {
                eprintln!("set_frame: failed to retrieve launcher panel");
                return;
            };

            // SAFETY: This closure runs inside `run_on_main_thread`,
            // which guarantees main-thread execution.
            let mtm = unsafe { MainThreadMarker::new_unchecked() };

            // macOS global coordinates use bottom-left origin relative
            // to the primary screen. Flip the top-left y coordinate.
            // AppKit lists the primary screen first in `screens`.
            let primary_height = NSScreen::screens(mtm)
                .firstObject()
                .map(|s| s.frame().size.height);
            let Some(flipped_y) = appkit_frame_y(primary_height, y, height) else {
                eprintln!("set_frame: no primary screen to place the launcher on");
                return;
            };

            let frame = NSRect::new(
                objc2_foundation::NSPoint::new(x, flipped_y),
                objc2_foundation::NSSize::new(width, height),
            );
            panel.as_panel().setFrame_display(frame, true);
        })
        .context("dispatch set_frame to main thread")?;

        Ok(())
    }
}

/// AppKit y of a frame's bottom edge, from the frame's `top` edge in
/// top-left-origin global points. AppKit's global origin is the
/// bottom-left corner of the primary screen, so the flip needs that
/// screen's height; without a primary screen there is no frame.
fn appkit_frame_y(primary_screen_height: Option<f64>, top: f64, height: f64) -> Option<f64> {
    primary_screen_height.map(|primary| primary - top - height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appkit_frame_y_on_the_primary_screen() {
        assert_eq!(appkit_frame_y(Some(982.0), 100.0, 830.0), Some(52.0));
    }

    #[test]
    fn appkit_frame_y_for_a_top_below_the_primary() {
        assert_eq!(appkit_frame_y(Some(982.0), 1200.0, 830.0), Some(-1048.0));
    }

    #[test]
    fn appkit_frame_y_for_a_negative_top() {
        assert_eq!(appkit_frame_y(Some(982.0), -1300.0, 830.0), Some(1452.0));
    }

    #[test]
    fn appkit_frame_y_is_none_without_a_primary_screen() {
        assert_eq!(appkit_frame_y(None, 100.0, 830.0), None);
    }
}
