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
use tauri::{Manager, Runtime};

use crate::control::handler::{ControlError, Handler};
use crate::control::{ControlChannelState, ControlCommand};

pub struct QueryHandler;

impl Handler for QueryHandler {
    fn handle(&self, params: Value, app: &tauri::AppHandle) -> Result<Value, ControlError> {
        query(params, app)
    }
}

fn query<R: Runtime>(params: Value, app: &tauri::AppHandle<R>) -> Result<Value, ControlError> {
    let text = params
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ControlError::InvalidState {
            message: "missing required parameter 'text' (string)".to_string(),
        })?
        .to_string();

    app.state::<ControlChannelState>()
        .deliver(ControlCommand::SetQuery { text })?;

    Ok(serde_json::json!({ "ok": true }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tauri::test::{mock_builder, mock_context, noop_assets};

    use super::*;
    use crate::control::test_channels::recording;

    fn app() -> tauri::App<tauri::test::MockRuntime> {
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("mock app builds");
        app.manage(ControlChannelState::new());
        app
    }

    #[test]
    fn query_without_a_frontend_is_invalid_state() {
        let app = app();
        let error =
            query(json!({"text": "firefox"}), app.handle()).expect_err("no frontend is subscribed");
        assert_eq!(error.code(), -1);
        assert_eq!(error.message(), "launcher frontend is not connected");
    }

    #[test]
    fn query_hands_the_text_to_the_frontend() {
        let app = app();
        let (channel, received) = recording();
        app.state::<ControlChannelState>().set(channel);

        let result = query(json!({"text": "firefox"}), app.handle()).expect("query succeeds");

        assert_eq!(result, json!({"ok": true}));
        assert_eq!(
            *received.lock().expect("test lock"),
            vec![r#"{"type":"setQuery","text":"firefox"}"#.to_string()]
        );
    }

    #[test]
    fn query_without_text_sends_nothing() {
        let app = app();
        let (channel, received) = recording();
        app.state::<ControlChannelState>().set(channel);

        let error = query(json!({}), app.handle()).expect_err("text is required");

        assert_eq!(error.code(), -1);
        assert_eq!(
            error.message(),
            "missing required parameter 'text' (string)"
        );
        assert!(received.lock().expect("test lock").is_empty());
    }
}
