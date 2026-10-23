// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Asking the launcher frontend for its state
//
// The query, results, selection and gadget views live only in
// the launcher's React state. To report them, the backend sends
// `ReportSnapshot { request_id }` over the control channel and
// waits; the frontend answers with the `control_report_snapshot`
// command, which completes the waiting request by its id
// (ADR 69).
// =========================================================

use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::handler::ControlError;
use super::{ControlChannelState, ControlCommand, NOT_CONNECTED, NotConnected};

/// How long a `snapshot` request waits for the frontend's answer.
pub const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(2);

/// What the launcher shows, as the frontend reports it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSnapshot {
    pub query: String,
    pub searching: bool,
    pub results: Vec<SnapshotEntry>,
    pub selected_index: Option<usize>,
    pub inline_view: Option<SnapshotView>,
    pub inline_view_selected: bool,
    pub gadget_view: Option<SnapshotView>,
    pub content_visible: bool,
    pub input_focused: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotEntry {
    pub source: String,
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotView {
    pub gadget_id: String,
    pub view: String,
}

/// A request waiting for the frontend's answer: the channel it was sent
/// on, and where its answer goes. A malformed answer arrives as `Err`
/// with the parse error.
pub(super) struct PendingSnapshot {
    pub(super) channel_id: u32,
    answer: mpsc::SyncSender<Result<LauncherSnapshot, String>>,
}

#[derive(Debug, PartialEq)]
pub enum SnapshotError {
    NotConnected,
    Unanswered,
    Reloaded,
    Malformed(String),
}

impl From<SnapshotError> for ControlError {
    fn from(error: SnapshotError) -> Self {
        match error {
            SnapshotError::NotConnected => ControlError::InvalidState {
                message: NOT_CONNECTED.to_string(),
            },
            SnapshotError::Unanswered => ControlError::Unanswered {
                message: format!(
                    "launcher frontend did not answer within {} ms",
                    SNAPSHOT_TIMEOUT.as_millis()
                ),
            },
            SnapshotError::Reloaded => ControlError::Unanswered {
                message: "launcher frontend reloaded before answering".to_string(),
            },
            SnapshotError::Malformed(parse_error) => ControlError::Internal {
                message: format!("launcher frontend sent a malformed snapshot: {parse_error}"),
            },
        }
    }
}

impl ControlChannelState {
    /// Ask the frontend for a snapshot and wait up to `timeout` for it.
    pub fn request_snapshot(&self, timeout: Duration) -> Result<LauncherSnapshot, SnapshotError> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let (answer, answered) = mpsc::sync_channel(1);

        // Take the channel and register the request under one channel
        // lock, so a `set` either replaces the channel before this (and
        // the request goes to the new page) or drops this request.
        let channel = {
            let stored = self.channel.lock().expect("channel lock not poisoned");
            let channel = stored.clone().ok_or(SnapshotError::NotConnected)?;
            self.pending
                .lock()
                .expect("pending lock not poisoned")
                .insert(
                    request_id,
                    PendingSnapshot {
                        channel_id: channel.id(),
                        answer,
                    },
                );
            channel
        };

        if let Err(NotConnected) =
            self.send_on(&channel, ControlCommand::ReportSnapshot { request_id })
        {
            self.forget(request_id);
            return Err(SnapshotError::NotConnected);
        }

        match answered.recv_timeout(timeout) {
            Ok(Ok(snapshot)) => Ok(snapshot),
            Ok(Err(parse_error)) => Err(SnapshotError::Malformed(parse_error)),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.forget(request_id);
                Err(SnapshotError::Unanswered)
            }
            // `set` dropped the request: its page was replaced.
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(SnapshotError::Reloaded),
        }
    }

    fn forget(&self, request_id: u64) {
        self.pending
            .lock()
            .expect("pending lock not poisoned")
            .remove(&request_id);
    }

    /// Hand the frontend's answer to the request waiting for it. False
    /// when no request waits under `request_id` (it timed out, or its
    /// page was replaced).
    pub fn complete_snapshot(
        &self,
        request_id: u64,
        answer: Result<LauncherSnapshot, String>,
    ) -> bool {
        let waiting = self
            .pending
            .lock()
            .expect("pending lock not poisoned")
            .remove(&request_id);
        // The buffer holds the one answer, so this send does not block;
        // it fails only when the waiter gave up just now.
        waiting.is_some_and(|pending| pending.answer.send(answer).is_ok())
    }
}

/// The frontend's answer to `ReportSnapshot`. The snapshot arrives as
/// plain JSON and is parsed here, so a malformed answer still reaches
/// the waiting request (as an error) instead of being rejected before
/// this runs.
#[tauri::command]
pub fn control_report_snapshot(
    state: tauri::State<'_, ControlChannelState>,
    request_id: u64,
    snapshot: Value,
) -> Result<(), String> {
    match serde_json::from_value::<LauncherSnapshot>(snapshot) {
        Ok(snapshot) => {
            state.complete_snapshot(request_id, Ok(snapshot));
            Ok(())
        }
        Err(e) => {
            let message = e.to_string();
            state.complete_snapshot(request_id, Err(message.clone()));
            Err(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier, Mutex};
    use std::thread;

    use tauri::ipc::Channel;

    use super::*;
    use crate::control::test_channels::{body_json, failing_once};

    /// A test's safety net against a request that never returns: the
    /// expectations below never come near it.
    const HANG: Duration = Duration::from_secs(30);

    fn sample(query: &str) -> LauncherSnapshot {
        LauncherSnapshot {
            query: query.into(),
            searching: false,
            results: vec![SnapshotEntry {
                source: "app-launcher".into(),
                id: "/Applications/Firefox.app".into(),
                title: "Firefox".into(),
                subtitle: None,
            }],
            selected_index: Some(0),
            inline_view: None,
            inline_view_selected: false,
            gadget_view: None,
            content_visible: true,
            input_focused: true,
        }
    }

    /// The request id of a `ReportSnapshot` the backend sent.
    fn request_id(body: tauri::ipc::InvokeResponseBody) -> u64 {
        let message: Value = serde_json::from_str(&body_json(body)).expect("command is JSON");
        assert_eq!(message["type"], "reportSnapshot");
        message["requestId"].as_u64().expect("request id")
    }

    fn pending_count(state: &ControlChannelState) -> usize {
        state.pending.lock().expect("test lock").len()
    }

    /// A frontend that answers every request at once, from within the
    /// send, with `answer`.
    fn answering(
        state: &Arc<ControlChannelState>,
        answer: Result<LauncherSnapshot, String>,
    ) -> Channel<ControlCommand> {
        let state = Arc::downgrade(state);
        Channel::new(move |body| {
            let id = request_id(body);
            if let Some(state) = state.upgrade() {
                state.complete_snapshot(id, answer.clone());
            }
            Ok(())
        })
    }

    /// Runs `request` on its own thread and returns its result, failing
    /// the test instead of hanging when it never returns.
    fn within_deadline<T: Send + 'static>(request: impl FnOnce() -> T + Send + 'static) -> T {
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = done_tx.send(request());
        });
        done_rx
            .recv_timeout(HANG)
            .expect("the request returned instead of waiting")
    }

    #[test]
    fn snapshot_round_trip() {
        let state = Arc::new(ControlChannelState::new());
        state.set(answering(&state, Ok(sample("fire"))));
        assert_eq!(state.request_snapshot(HANG), Ok(sample("fire")));
        assert_eq!(pending_count(&state), 0);
    }

    #[test]
    fn snapshot_without_a_frontend_fails_at_once() {
        let state = ControlChannelState::new();
        assert_eq!(
            state.request_snapshot(HANG),
            Err(SnapshotError::NotConnected)
        );
        assert_eq!(pending_count(&state), 0);
    }

    #[test]
    fn snapshot_whose_send_fails_removes_its_entry() {
        let state = ControlChannelState::new();
        state.set(failing_once());
        assert_eq!(
            state.request_snapshot(HANG),
            Err(SnapshotError::NotConnected)
        );
        assert_eq!(pending_count(&state), 0);
        assert!(!state.is_subscribed());
    }

    #[test]
    fn snapshot_times_out() {
        let state = ControlChannelState::new();
        state.set(Channel::new(|_| Ok(())));
        assert_eq!(
            state.request_snapshot(Duration::ZERO),
            Err(SnapshotError::Unanswered)
        );
        assert_eq!(pending_count(&state), 0);
    }

    #[test]
    fn resubscribe_fails_a_request_on_the_old_channel() {
        let state = Arc::new(ControlChannelState::new());
        // The page reloads while the request is on its way: the new page
        // subscribes before the old one could answer.
        let resubscriber = Arc::downgrade(&state);
        state.set(Channel::new(move |_| {
            if let Some(state) = resubscriber.upgrade() {
                state.set(Channel::new(|_| Ok(())));
            }
            Ok(())
        }));

        let waiting = Arc::clone(&state);
        let result = within_deadline(move || waiting.request_snapshot(HANG));

        assert_eq!(result, Err(SnapshotError::Reloaded));
        assert_eq!(pending_count(&state), 0);
    }

    #[test]
    fn a_request_on_the_current_channel_survives_an_earlier_set() {
        let state = Arc::new(ControlChannelState::new());
        state.set(Channel::new(|_| Ok(())));
        state.set(answering(&state, Ok(sample("after reload"))));
        assert_eq!(state.request_snapshot(HANG), Ok(sample("after reload")));
    }

    #[test]
    fn late_reply_is_dropped() {
        let state = ControlChannelState::new();
        assert!(!state.complete_snapshot(99, Ok(sample("late"))));
        assert_eq!(pending_count(&state), 0);
    }

    #[test]
    fn malformed_reply_answers_at_once() {
        let state = Arc::new(ControlChannelState::new());
        state.set(answering(&state, Err("missing field `query`".into())));
        let waiting = Arc::clone(&state);
        let result = within_deadline(move || waiting.request_snapshot(HANG));
        assert_eq!(
            result,
            Err(SnapshotError::Malformed("missing field `query`".into()))
        );
    }

    #[test]
    fn concurrent_requests_get_their_own_answers() {
        for _ in 0..200 {
            let state = Arc::new(ControlChannelState::new());
            // The channel's callback runs on the requesting thread, so
            // it records which thread got which request id.
            let (sent_tx, sent_rx) = mpsc::channel();
            let sent_tx = Mutex::new(sent_tx);
            state.set(Channel::new(move |body| {
                let sent = (thread::current().id(), request_id(body));
                sent_tx
                    .lock()
                    .expect("test lock")
                    .send(sent)
                    .expect("test listens");
                Ok(())
            }));

            let start = Arc::new(Barrier::new(2));
            let requesters: Vec<_> = (0..2)
                .map(|_| {
                    let state = Arc::clone(&state);
                    let start = Arc::clone(&start);
                    thread::spawn(move || {
                        start.wait();
                        (thread::current().id(), state.request_snapshot(HANG))
                    })
                })
                .collect();

            let first = sent_rx.recv_timeout(HANG).expect("first request sent");
            let second = sent_rx.recv_timeout(HANG).expect("second request sent");
            // Answer in reverse order of sending.
            for (_, id) in [second, first] {
                assert!(state.complete_snapshot(id, Ok(sample(&format!("answer {id}")))));
            }

            let sent = [first, second];
            for requester in requesters {
                let (thread, result) = requester.join().expect("requester finished");
                let (_, id) = sent
                    .iter()
                    .find(|(sender, _)| *sender == thread)
                    .expect("every requester sent a request");
                assert_eq!(result, Ok(sample(&format!("answer {id}"))));
            }
            assert_eq!(pending_count(&state), 0);
        }
    }

    #[test]
    fn report_snapshot_wire_format() {
        assert_eq!(
            serde_json::to_value(ControlCommand::ReportSnapshot { request_id: 7 })
                .expect("serializes"),
            serde_json::json!({"type": "reportSnapshot", "requestId": 7})
        );
    }

    #[test]
    fn snapshot_reads_the_frontend_shape() {
        let snapshot: LauncherSnapshot = serde_json::from_value(serde_json::json!({
            "query": "fire",
            "searching": false,
            "results": [{
                "source": "app-launcher",
                "id": "/Applications/Firefox.app",
                "title": "Firefox",
                "subtitle": null,
            }],
            "selectedIndex": 0,
            "inlineView": null,
            "inlineViewSelected": false,
            "gadgetView": null,
            "contentVisible": true,
            "inputFocused": true,
        }))
        .expect("parses");
        assert_eq!(snapshot, sample("fire"));
    }

    #[test]
    fn snapshot_errors_map_to_control_errors() {
        let codes: Vec<(i64, String)> = [
            SnapshotError::NotConnected,
            SnapshotError::Unanswered,
            SnapshotError::Reloaded,
            SnapshotError::Malformed("missing field `query`".into()),
        ]
        .into_iter()
        .map(|e| {
            let e = ControlError::from(e);
            (e.code(), e.message().to_string())
        })
        .collect();
        assert_eq!(
            codes,
            vec![
                (-1, "launcher frontend is not connected".to_string()),
                (
                    -4,
                    "launcher frontend did not answer within 2000 ms".to_string()
                ),
                (
                    -4,
                    "launcher frontend reloaded before answering".to_string()
                ),
                (
                    -3,
                    "launcher frontend sent a malformed snapshot: missing field `query`"
                        .to_string()
                ),
            ]
        );
    }

    // =========================================================
    // The Tauri command, through the IPC layer
    // =========================================================

    mod ipc {
        use serde_json::json;
        use tauri::Manager as _;
        use tauri::test::{
            INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
        };

        use super::*;

        struct TestApp {
            _app: tauri::App<MockRuntime>,
            webview: tauri::WebviewWindow<MockRuntime>,
        }

        impl TestApp {
            /// A launcher whose frontend reports the request ids it gets
            /// on `ids` instead of answering.
            fn new(ids: mpsc::Sender<u64>) -> Self {
                let app = mock_builder()
                    .invoke_handler(tauri::generate_handler![control_report_snapshot])
                    .build(mock_context(noop_assets()))
                    .expect("mock app builds");
                app.manage(ControlChannelState::new());
                let ids = Mutex::new(ids);
                app.state::<ControlChannelState>()
                    .set(Channel::new(move |body| {
                        let _ = ids.lock().expect("test lock").send(request_id(body));
                        Ok(())
                    }));
                let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
                    .build()
                    .expect("mock webview builds");
                Self { _app: app, webview }
            }

            fn report(&self, request_id: u64, snapshot: Value) -> Result<Value, Value> {
                get_ipc_response(
                    &self.webview,
                    tauri::webview::InvokeRequest {
                        cmd: "control_report_snapshot".into(),
                        callback: tauri::ipc::CallbackFn(0),
                        error: tauri::ipc::CallbackFn(1),
                        url: "tauri://localhost".parse().expect("valid url"),
                        body: tauri::ipc::InvokeBody::Json(
                            json!({"requestId": request_id, "snapshot": snapshot}),
                        ),
                        headers: Default::default(),
                        invoke_key: INVOKE_KEY.to_string(),
                    },
                )
                .map(|response| response.deserialize::<Value>().expect("response is JSON"))
            }
        }

        /// Starts a request on the app's managed state and returns the
        /// waiting thread and the request id the frontend got.
        fn start_request(
            test: &TestApp,
            ids: &mpsc::Receiver<u64>,
        ) -> (
            thread::JoinHandle<Result<LauncherSnapshot, SnapshotError>>,
            u64,
        ) {
            let app = test.webview.app_handle().clone();
            let waiting =
                thread::spawn(move || app.state::<ControlChannelState>().request_snapshot(HANG));
            let id = ids.recv_timeout(HANG).expect("request sent");
            (waiting, id)
        }

        #[test]
        fn a_reported_snapshot_reaches_the_waiting_request() {
            let (ids_tx, ids) = mpsc::channel();
            let test = TestApp::new(ids_tx);
            let (waiting, id) = start_request(&test, &ids);

            let reply = test.report(id, serde_json::to_value(sample("fire")).expect("json"));

            assert_eq!(reply, Ok(Value::Null));
            assert_eq!(
                waiting.join().expect("request finished"),
                Ok(sample("fire"))
            );
        }

        #[test]
        fn a_malformed_snapshot_fails_the_command_and_the_request() {
            let (ids_tx, ids) = mpsc::channel();
            let test = TestApp::new(ids_tx);
            let (waiting, id) = start_request(&test, &ids);

            let reply = test.report(id, json!({"query": "fire"}));

            assert!(reply.is_err(), "{reply:?}");
            assert!(matches!(
                waiting.join().expect("request finished"),
                Err(SnapshotError::Malformed(message)) if message.contains("searching")
            ));
        }
    }
}
