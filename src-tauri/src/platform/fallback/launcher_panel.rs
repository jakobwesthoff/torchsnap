// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Launcher Panel (Fallback)
//
// Uses a regular Tauri window. This works but lacks the
// non-activating behavior of macOS NSPanel — showing the
// launcher steals focus from the active app.
//
// TODO: Investigate platform-specific alternatives:
//   - Linux/Wayland: layer-shell protocol for overlay windows
//   - Linux/X11: override-redirect or _NET_WM_STATE hints
//   - Windows: WS_EX_NOACTIVATE extended window style
// =========================================================

use anyhow::Context;
use tauri::Manager;

use crate::platform::LauncherPanel;

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
