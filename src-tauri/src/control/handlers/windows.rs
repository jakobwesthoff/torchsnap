// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Windows Handler
//
// Lists the app's open windows (the launcher, settings,
// developer tools, update and welcome windows) with their
// visibility and focus. Tauri's window getters go through the
// event loop and are safe to call from the handler's thread.
// =========================================================

use serde::Serialize;
use serde_json::Value;
use tauri::{Manager, Runtime};

use crate::control::handler::{ControlError, Handler};

pub struct WindowsHandler;

impl Handler for WindowsHandler {
    fn handle(&self, _params: Value, app: &tauri::AppHandle) -> Result<Value, ControlError> {
        Ok(serde_json::json!({ "windows": list_windows(app) }))
    }
}

#[derive(Debug, PartialEq, Serialize)]
struct WindowInfo {
    label: String,
    title: String,
    visible: bool,
    focused: bool,
}

/// Every open webview window, sorted by label.
fn list_windows<R: Runtime>(app: &tauri::AppHandle<R>) -> Vec<WindowInfo> {
    let mut windows: Vec<WindowInfo> = app
        .webview_windows()
        .into_iter()
        .filter_map(|(label, window)| {
            window_info(
                label,
                window.title(),
                window.is_visible(),
                window.is_focused(),
            )
        })
        .collect();
    windows.sort_by(|a, b| a.label.cmp(&b.label));
    windows
}

/// A window whose getters fail was destroyed after it was listed; it is
/// left out.
fn window_info(
    label: String,
    title: tauri::Result<String>,
    visible: tauri::Result<bool>,
    focused: tauri::Result<bool>,
) -> Option<WindowInfo> {
    Some(WindowInfo {
        label,
        title: title.ok()?,
        visible: visible.ok()?,
        focused: focused.ok()?,
    })
}

#[cfg(test)]
mod tests {
    use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

    use super::*;

    fn app_with_windows(labels: &[&str]) -> tauri::App<MockRuntime> {
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("mock app builds");
        for label in labels {
            tauri::WebviewWindowBuilder::new(&app, *label, Default::default())
                .build()
                .expect("mock window builds");
        }
        app
    }

    // The mock runtime reports every window as visible, unfocused and
    // untitled.
    fn mock_window(label: &str) -> WindowInfo {
        WindowInfo {
            label: label.into(),
            title: String::new(),
            visible: true,
            focused: false,
        }
    }

    #[test]
    fn lists_every_window_sorted_by_label() {
        let app = app_with_windows(&["settings", "main"]);
        assert_eq!(
            list_windows(app.handle()),
            vec![mock_window("main"), mock_window("settings")]
        );
    }

    #[test]
    fn no_windows_gives_an_empty_list() {
        let app = app_with_windows(&[]);
        assert_eq!(list_windows(app.handle()), Vec::new());
    }

    #[test]
    fn window_info_skips_a_window_whose_getter_fails() {
        assert_eq!(
            window_info(
                "settings".into(),
                Ok("Torchsnap Settings".into()),
                Err(tauri::Error::WebviewNotFound),
                Ok(false),
            ),
            None
        );
    }

    #[test]
    fn window_info_keeps_a_readable_window() {
        assert_eq!(
            window_info(
                "settings".into(),
                Ok("Torchsnap Settings".into()),
                Ok(true),
                Ok(true),
            ),
            Some(WindowInfo {
                label: "settings".into(),
                title: "Torchsnap Settings".into(),
                visible: true,
                focused: true,
            })
        );
    }

    #[test]
    fn window_info_serializes_with_these_names() {
        assert_eq!(
            serde_json::to_value(mock_window("main")).expect("serializes"),
            serde_json::json!({"label": "main", "title": "", "visible": true, "focused": false})
        );
    }
}
