// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Launcher Visibility Handlers
//
// JSON-RPC methods for controlling the launcher window:
// show, hide, toggle, and dismiss.
// =========================================================

use serde_json::Value;
use tauri::Manager;

use crate::control::handler::{ControlError, Handler};
use crate::control::ControlCommand;
use crate::platform::{LauncherPanel as _, PlatformLauncherPanel};

// =========================================================
// show — Make the launcher visible
// =========================================================

pub struct ShowHandler;

impl Handler for ShowHandler {
    fn handle(&self, _params: Value, app: &tauri::AppHandle) -> Result<Value, ControlError> {
        crate::position_launcher_on_cursor_monitor(app);
        PlatformLauncherPanel::show(app)
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
        PlatformLauncherPanel::hide(app).map_err(|e| ControlError::Internal {
            message: format!("{e:#}"),
        })?;

        Ok(serde_json::json!({ "ok": true }))
    }
}

// =========================================================
// toggle — Toggle launcher visibility
// =========================================================

pub struct ToggleHandler;

impl Handler for ToggleHandler {
    fn handle(&self, _params: Value, app: &tauri::AppHandle) -> Result<Value, ControlError> {
        crate::toggle_launcher_window(app);
        Ok(serde_json::json!({ "ok": true }))
    }
}

// =========================================================
// dismiss — Reset frontend state and hide
//
// Pushes a Dismiss command through the control channel so
// the frontend clears its query/selection, then hides the
// window. The channel message is buffered and will be
// processed before the next show — even if the JS event
// loop hasn't drained it by the time hide completes.
// =========================================================

pub struct DismissHandler;

impl Handler for DismissHandler {
    fn handle(&self, _params: Value, app: &tauri::AppHandle) -> Result<Value, ControlError> {
        let channel_state = app.state::<crate::control::ControlChannelState>();
        channel_state.send(ControlCommand::Dismiss);

        PlatformLauncherPanel::hide(app).map_err(|e| ControlError::Internal {
            message: format!("{e:#}"),
        })?;

        Ok(serde_json::json!({ "ok": true }))
    }
}
