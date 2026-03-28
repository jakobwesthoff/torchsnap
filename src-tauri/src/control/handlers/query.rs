// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Query Handler
//
// Sets the search query text in the frontend via the control
// channel. The frontend's `useSearch` hook then triggers the
// search through its normal path.
//
// Future: return search results directly in the JSON-RPC
// response. This requires solving the double-search problem
// (control handler and frontend both running the search).
// =========================================================

use serde_json::Value;
use tauri::Manager;

use crate::control::ControlCommand;
use crate::control::handler::{ControlError, Handler};

pub struct QueryHandler;

impl Handler for QueryHandler {
    fn handle(&self, params: Value, app: &tauri::AppHandle) -> Result<Value, ControlError> {
        let text = params
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ControlError::InvalidState {
                message: "missing required parameter 'text' (string)".to_string(),
            })?
            .to_string();

        let channel_state = app.state::<crate::control::ControlChannelState>();
        channel_state.send(ControlCommand::SetQuery { text });

        Ok(serde_json::json!({ "ok": true }))
    }
}
