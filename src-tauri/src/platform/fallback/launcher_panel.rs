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
    fn init(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
        // The window is already created with transparent + undecorated
        // properties by the caller.
        //
        // Each show draws a new mascot, and WebKitGTK keeps every image
        // it has shown in its memory cache, decoded and as a GPU
        // texture, up to limits of its own. The document viewer cache
        // model turns that cache off. WebKitGTK holds the cache model
        // for the whole process, so the settings, welcome and update
        // windows run without the cache too (ADR 0073).
        #[cfg(target_os = "linux")]
        window
            .with_webview(|webview| {
                use webkit2gtk::{CacheModel, WebContextExt, WebViewExt};
                webview
                    .inner()
                    .context()
                    .expect("WebKitGTK gives every web view a web context")
                    .set_cache_model(CacheModel::DocumentViewer);
            })
            .context("set WebKitGTK's cache model")?;
        #[cfg(not(target_os = "linux"))]
        let _ = window;
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use tauri::test::{mock_context, noop_assets};
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    use webkit2gtk::{CacheModel, WebContext, WebContextExt, WebViewExt};

    use super::*;

    // GTK belongs to the thread that initialised it, and every test runs
    // on a thread of its own, so this stays the module's only GTK test.
    #[test]
    fn launcher_init_turns_off_webkits_cache_for_every_window() {
        let app = tauri::Builder::<tauri::Wry>::default()
            .any_thread()
            .build(mock_context(noop_assets()))
            .expect("a Wry app builds here, given a display (Wayland, X11 or Xvfb)");
        let hidden_window = |label: &str| {
            WebviewWindowBuilder::new(&app, label, WebviewUrl::App("launcher.html".into()))
                .visible(false)
                .build()
                .expect("a hidden window builds")
        };
        let launcher = hidden_window("main");
        assert_eq!(WebContext::new().cache_model(), CacheModel::WebBrowser);

        FallbackLauncherPanel::init(&launcher).expect("init succeeds");

        // WebKitGTK holds the cache model for the whole process: a new
        // context reports it, and so does a window built afterwards on
        // the context tauri shares between its windows.
        assert_eq!(WebContext::new().cache_model(), CacheModel::DocumentViewer);
        let settings = hidden_window("settings");
        let (tx, rx) = std::sync::mpsc::channel();
        settings
            .with_webview(move |webview| {
                let model = webview.inner().context().map(|c| c.cache_model());
                tx.send(model).expect("the test waits for the answer");
            })
            .expect("with_webview reaches the settings window");
        assert_eq!(
            rx.recv().expect("the closure ran"),
            Some(CacheModel::DocumentViewer)
        );
    }
}
