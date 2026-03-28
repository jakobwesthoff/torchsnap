// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Status Handler
//
// Reports the current launcher state. Currently limited to
// visibility; query text and selected result require a
// frontend round-trip and may be added later.
// =========================================================

use serde_json::Value;

use crate::control::handler::{ControlError, Handler};
use crate::platform::{LauncherPanel as _, PlatformLauncherPanel};

pub struct StatusHandler;

impl Handler for StatusHandler {
    fn handle(&self, _params: Value, app: &tauri::AppHandle) -> Result<Value, ControlError> {
        let visible = PlatformLauncherPanel::is_visible(app).map_err(|e| {
            ControlError::Internal {
                message: format!("{e:#}"),
            }
        })?;

        Ok(serde_json::json!({
            "visible": visible,
        }))
    }
}
