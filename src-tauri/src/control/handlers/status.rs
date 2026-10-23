// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Status Handler
//
// Reports the launcher state the backend knows without asking
// the frontend: whether the window is visible and has focus,
// and whether the launcher is ready to be driven. Query text and selected
// result require a frontend round-trip and may be added later.
//
// `is_visible` queries the NSPanel state, which must happen
// on the main thread, dispatched via the same helper used
// by the launcher handlers; focus is read in the same call.
// =========================================================

use serde::Serialize;
use serde_json::Value;
use tauri::Manager;

use crate::LauncherLayoutState;
use crate::control::ControlChannelState;
use crate::control::handler::{ControlError, Handler};
use crate::platform::{LauncherPanel as _, PlatformLauncherPanel};

pub struct StatusHandler;

impl Handler for StatusHandler {
    fn handle(&self, _params: Value, app: &tauri::AppHandle) -> Result<Value, ControlError> {
        // Read together, so the pair describes one moment.
        let handle = app.clone();
        let (visible, focused) = super::launcher::on_main_thread(app, move || {
            anyhow::Ok((
                PlatformLauncherPanel::is_visible(&handle)?,
                PlatformLauncherPanel::is_focused(&handle)?,
            ))
        })?
        .map_err(|e| ControlError::Internal {
            message: format!("{e:#}"),
        })?;

        let report = StatusReport::new(
            visible,
            focused,
            app.state::<LauncherLayoutState>().get().is_some(),
            app.state::<ControlChannelState>().is_subscribed(),
        );
        Ok(serde_json::to_value(report).expect("a status report serializes"))
    }
}

/// The `status` answer.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct StatusReport {
    visible: bool,
    focused: bool,
    /// `show` needs the layout and `query`/`dismiss` a subscribed
    /// frontend; with both, the launcher can be driven.
    ready: bool,
}

impl StatusReport {
    fn new(visible: bool, focused: bool, layout_received: bool, frontend_subscribed: bool) -> Self {
        Self {
            visible,
            focused,
            ready: layout_received && frontend_subscribed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_needs_layout_and_frontend() {
        assert!(StatusReport::new(false, false, true, true).ready);
    }

    #[test]
    fn not_ready_without_layout() {
        assert!(!StatusReport::new(false, false, false, true).ready);
    }

    #[test]
    fn not_ready_without_frontend() {
        assert!(!StatusReport::new(false, false, true, false).ready);
    }

    #[test]
    fn status_report_carries_focus() {
        assert!(StatusReport::new(true, true, true, true).focused);
    }

    #[test]
    fn status_report_field_names() {
        assert_eq!(
            serde_json::to_value(StatusReport::new(true, false, true, true)).expect("serializes"),
            serde_json::json!({"visible": true, "focused": false, "ready": true})
        );
    }
}
