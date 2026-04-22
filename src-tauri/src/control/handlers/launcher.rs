// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Launcher Visibility Handlers
//
// JSON-RPC methods for controlling the launcher window:
// show, hide, toggle, and dismiss.
//
// All platform panel operations (show, hide, is_visible) must
// run on the main thread on macOS. Since the control server
// runs in a tokio task, we dispatch through
// `app.run_on_main_thread()` and block on a oneshot channel
// to get the result back.
// =========================================================

use serde_json::Value;
use tauri::Manager;

use crate::LauncherLayoutState;
use crate::control::ControlCommand;
use crate::control::handler::{ControlError, Handler};
use crate::{hide_launcher, request_launcher_dismiss, show_launcher};
use crate::platform::{LauncherPanel as _, PlatformLauncherPanel};

/// Run a closure on the main thread and block until it completes,
/// returning its result. Needed because AppKit/NSPanel calls must
/// happen on the main thread, but control handlers execute in a
/// tokio task.
pub(super) fn on_main_thread<F, R>(app: &tauri::AppHandle, f: F) -> Result<R, ControlError>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(f());
    })
    .map_err(|e| ControlError::Internal {
        message: format!("dispatch to main thread: {e:#}"),
    })?;

    rx.recv().map_err(|_| ControlError::Internal {
        message: "main thread closure dropped without sending result".to_string(),
    })
}

// =========================================================
// show — Make the launcher visible
// =========================================================

pub struct ShowHandler;

impl Handler for ShowHandler {
    fn handle(&self, _params: Value, app: &tauri::AppHandle) -> Result<Value, ControlError> {
        let layout =
            *app.state::<LauncherLayoutState>()
                .get()
                .ok_or_else(|| ControlError::Internal {
                    message: "launcher layout not yet received from frontend".to_string(),
                })?;

        let handle = app.clone();
        on_main_thread(app, move || {
            crate::position_launcher_on_cursor_monitor(&handle, &layout);
            show_launcher(&handle)
        })?
        .map_err(|e| ControlError::Internal {
            message: format!("{e:#}"),
        })?;

        Ok(serde_json::json!({ "ok": true }))
    }
}

// =========================================================
// hide — Hide the launcher without resetting state
// =========================================================

pub struct HideHandler;

impl Handler for HideHandler {
    fn handle(&self, _params: Value, app: &tauri::AppHandle) -> Result<Value, ControlError> {
        let handle = app.clone();
        on_main_thread(app, move || hide_launcher(&handle))?;

        Ok(serde_json::json!({ "ok": true }))
    }
}

// =========================================================
// toggle — Toggle launcher visibility
// =========================================================

pub struct ToggleHandler;

impl Handler for ToggleHandler {
    fn handle(&self, _params: Value, app: &tauri::AppHandle) -> Result<Value, ControlError> {
        let handle = app.clone();
        on_main_thread(app, move || {
            crate::toggle_launcher_window(&handle);
        })?;

        Ok(serde_json::json!({ "ok": true }))
    }
}

// =========================================================
// dismiss — Reset frontend state and hide
//
// Pushes a Dismiss command through the control channel so
// the frontend clears its query/selection, then asks the
// platform dismiss path to hide the window. On Linux the
// hide is driven by the frontend after it blanks its root
// element; on macOS / Windows the hide happens synchronously.
// =========================================================

pub struct DismissHandler;

impl Handler for DismissHandler {
    fn handle(&self, _params: Value, app: &tauri::AppHandle) -> Result<Value, ControlError> {
        let channel_state = app.state::<crate::control::ControlChannelState>();
        channel_state.send(ControlCommand::Dismiss);

        let handle = app.clone();
        on_main_thread(app, move || request_launcher_dismiss(&handle))?;

        Ok(serde_json::json!({ "ok": true }))
    }
}
