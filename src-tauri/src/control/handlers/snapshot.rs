// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Snapshot Handler
//
// Reports what the launcher shows (query, results, selection,
// gadget views, content visibility, input focus) by asking the
// launcher frontend. Unlike `status`, the answer comes from the
// frontend and can take up to `SNAPSHOT_TIMEOUT`.
// =========================================================

use std::time::Duration;

use serde_json::Value;
use tauri::{Manager, Runtime};

use crate::control::handler::{ControlError, Handler};
use crate::control::{ControlChannelState, SNAPSHOT_TIMEOUT};

pub struct SnapshotHandler;

impl Handler for SnapshotHandler {
    fn handle(&self, _params: Value, app: &tauri::AppHandle) -> Result<Value, ControlError> {
        snapshot(app, SNAPSHOT_TIMEOUT)
    }
}

fn snapshot<R: Runtime>(
    app: &tauri::AppHandle<R>,
    timeout: Duration,
) -> Result<Value, ControlError> {
    let snapshot = app
        .state::<ControlChannelState>()
        .request_snapshot(timeout)?;
    Ok(serde_json::to_value(snapshot).expect("a launcher snapshot serializes"))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tauri::ipc::Channel;
    use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

    use super::*;
    use crate::control::test_channels::body_json;

    fn app() -> tauri::App<MockRuntime> {
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("mock app builds");
        app.manage(ControlChannelState::new());
        app
    }

    #[test]
    fn snapshot_without_a_frontend_is_invalid_state() {
        let app = app();
        let error = snapshot(app.handle(), Duration::from_secs(30)).expect_err("no frontend");
        assert_eq!(error.code(), -1);
    }

    #[test]
    fn snapshot_answers_with_the_frontend_report() {
        let app = app();
        let handle = app.handle().clone();
        let reported = Arc::new(serde_json::json!({
            "query": "fire",
            "searching": true,
            "results": [],
            "selectedIndex": null,
            "inlineView": null,
            "inlineViewSelected": false,
            "gadgetView": {"gadgetId": "calculator", "view": "Result"},
            "contentVisible": true,
            "inputFocused": false,
        }));
        let answer = Arc::clone(&reported);
        app.state::<ControlChannelState>()
            .set(Channel::new(move |body| {
                let message: Value = serde_json::from_str(&body_json(body)).expect("JSON");
                let id = message["requestId"].as_u64().expect("request id");
                let parsed = serde_json::from_value((*answer).clone()).expect("valid snapshot");
                handle
                    .state::<ControlChannelState>()
                    .complete_snapshot(id, Ok(parsed));
                Ok(())
            }));

        assert_eq!(
            snapshot(app.handle(), Duration::from_secs(30)).expect("answered"),
            *reported
        );
    }
}
