// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Tauri commands of the install queue.
//!
//! Every queue change is announced with `install-queue-changed` (no
//! payload); open windows pull the queue again with
//! `install_queue_snapshot`. Pulling after a notification works whether
//! or not a window existed when a request arrived, which a pushed
//! payload would not. None of these commands opens a window; showing
//! the settings window is up to the OS entry points.

use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;

use super::InstalledGadgetInfo;
use super::intake::InstallOrigin;
use super::queue::{InstallQueue, InstallRequestView, RequestId, Submitted};

pub const QUEUE_CHANGED_EVENT: &str = "install-queue-changed";

/// Stage the archives of `ids` on blocking workers. Each worker updates
/// its request and announces the change itself.
pub fn process_in_background(queue: &Arc<InstallQueue>, ids: Vec<RequestId>) {
    for id in ids {
        let queue = Arc::clone(queue);
        tauri::async_runtime::spawn_blocking(move || queue.process(&id));
    }
}

#[tauri::command]
pub fn install_queue_snapshot(
    queue: tauri::State<'_, Arc<InstallQueue>>,
) -> Vec<InstallRequestView> {
    queue.snapshot()
}

/// Queue archives picked or dropped in the settings window. Paths from
/// the settings window are absolute, so no working directory applies.
#[tauri::command]
pub fn install_queue_submit(
    queue: tauri::State<'_, Arc<InstallQueue>>,
    paths: Vec<String>,
    origin: InstallOrigin,
) {
    let ids = paths
        .iter()
        .filter_map(
            |path| match queue.submit(OsStr::new(path), Path::new("/"), origin) {
                Submitted::Queued(id) => Some(id),
                Submitted::Buffered | Submitted::Ignored => None,
            },
        )
        .collect();
    process_in_background(queue.inner(), ids);
}

#[tauri::command]
pub async fn install_queue_confirm(
    queue: tauri::State<'_, Arc<InstallQueue>>,
    request_id: String,
) -> Result<InstalledGadgetInfo, String> {
    let queue = Arc::clone(queue.inner());
    tauri::async_runtime::spawn_blocking(move || queue.confirm(&request_id))
        .await
        .map_err(|e| format!("install task panicked: {e}"))?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn install_queue_dismiss(
    queue: tauri::State<'_, Arc<InstallQueue>>,
    request_id: String,
) -> Result<(), String> {
    queue.dismiss(&request_id).map_err(|e| format!("{e:#}"))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use serde_json::{Value, json};
    use tauri::test::{
        INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
    };
    use tauri::{Emitter as _, Listener as _, Manager as _};

    use super::*;
    use crate::gadget_install::paths::InstallPaths;
    use crate::gadget_install::pending::PendingChanges;
    use crate::gadget_install::queue::QueueContext;
    use crate::gadget_install::registered::RegisteredGadgets;
    use crate::gadget_install::staging::StagingArea;
    use crate::wasm::manifest::test_helpers::archive_with_manifest;

    struct TestApp {
        _root: tempfile::TempDir,
        app: tauri::App<MockRuntime>,
        webview: tauri::WebviewWindow<MockRuntime>,
        paths: InstallPaths,
        events: Arc<AtomicUsize>,
    }

    impl TestApp {
        fn new() -> Self {
            let root = tempfile::tempdir().expect("create temp dir");
            let app = mock_builder()
                .invoke_handler(tauri::generate_handler![
                    install_queue_snapshot,
                    install_queue_submit,
                    install_queue_confirm,
                    install_queue_dismiss,
                ])
                .build(mock_context(noop_assets()))
                .expect("mock app builds");

            let paths = InstallPaths::new(&root.path().join("data"));
            let queue = Arc::new(InstallQueue::new());
            let handle = app.handle().clone();
            queue.start(QueueContext {
                paths: paths.clone(),
                registered: RegisteredGadgets::default(),
                pending: Arc::new(Mutex::new(PendingChanges::default())),
                staging: StagingArea::new(root.path().join("staging")),
                on_change: Arc::new(move || {
                    let _ = handle.emit(QUEUE_CHANGED_EVENT, ());
                }),
            });
            app.manage(queue);

            let events = Arc::new(AtomicUsize::new(0));
            let counter = Arc::clone(&events);
            app.listen_any(QUEUE_CHANGED_EVENT, move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
            });

            let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
                .build()
                .expect("mock webview builds");

            Self {
                _root: root,
                app,
                webview,
                paths,
                events,
            }
        }

        fn invoke(&self, cmd: &str, body: Value) -> Result<Value, Value> {
            get_ipc_response(
                &self.webview,
                tauri::webview::InvokeRequest {
                    cmd: cmd.into(),
                    callback: tauri::ipc::CallbackFn(0),
                    error: tauri::ipc::CallbackFn(1),
                    // The origin Tauri treats as bundled content; any
                    // other URL counts as remote and is refused.
                    url: "tauri://localhost".parse().expect("valid url"),
                    body: tauri::ipc::InvokeBody::Json(body),
                    headers: Default::default(),
                    invoke_key: INVOKE_KEY.to_string(),
                },
            )
            .map(|response| response.deserialize::<Value>().expect("response is JSON"))
        }

        fn snapshot(&self) -> Vec<Value> {
            self.invoke("install_queue_snapshot", json!({}))
                .expect("snapshot succeeds")
                .as_array()
                .expect("snapshot is an array")
                .clone()
        }

        /// Staging runs on a blocking worker; wait until every request
        /// has left the `staging` state.
        fn wait_until_processed(&self) -> Vec<Value> {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let snapshot = self.snapshot();
                if snapshot.iter().all(|r| r["state"]["kind"] != "staging") {
                    return snapshot;
                }
                assert!(Instant::now() < deadline, "staging did not finish");
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn events(&self) -> usize {
            self.events.load(Ordering::SeqCst)
        }

        fn submit_ready(&self, archive: &Path) -> String {
            self.invoke(
                "install_queue_submit",
                json!({ "paths": [archive], "origin": "settingsPicker" }),
            )
            .expect("submit succeeds");
            let snapshot = self.wait_until_processed();
            assert_eq!(snapshot.len(), 1);
            assert_eq!(snapshot[0]["state"]["kind"], "ready");
            snapshot[0]["id"]
                .as_str()
                .expect("id is a string")
                .to_string()
        }
    }

    #[test]
    fn a_submitted_archive_appears_in_the_snapshot_with_its_review() {
        let test = TestApp::new();
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");

        test.submit_ready(&archive);

        let snapshot = test.snapshot();
        assert_eq!(snapshot[0]["origin"], "settingsPicker");
        assert_eq!(snapshot[0]["state"]["review"]["gadget"]["id"], "weather");
        assert_eq!(snapshot[0]["state"]["review"]["action"]["kind"], "install");
    }

    #[test]
    fn confirming_installs_the_archive_and_returns_the_outcome() {
        let test = TestApp::new();
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");
        let id = test.submit_ready(&archive);

        let outcome = test
            .invoke("install_queue_confirm", json!({ "requestId": id }))
            .expect("confirm succeeds");

        assert_eq!(outcome["id"], "weather");
        assert_eq!(outcome["requiresRestart"], true);
        assert!(test.paths.archive("weather").exists());
        assert!(test.snapshot().is_empty());
    }

    #[test]
    fn confirming_an_unknown_request_is_an_error() {
        let test = TestApp::new();

        let error = test
            .invoke("install_queue_confirm", json!({ "requestId": "01unknown" }))
            .expect_err("unknown request");

        assert!(error.to_string().contains("unknown install request"));
    }

    #[test]
    fn dismissing_removes_the_request() {
        let test = TestApp::new();
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");
        let id = test.submit_ready(&archive);

        test.invoke("install_queue_dismiss", json!({ "requestId": id }))
            .expect("dismiss succeeds");

        assert!(test.snapshot().is_empty());
        assert!(!test.paths.archive("weather").exists());
    }

    #[test]
    fn every_mutation_announces_a_queue_change() {
        let test = TestApp::new();
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");

        let id = test.submit_ready(&archive);
        // Queued, then ready.
        assert_eq!(test.events(), 2);

        test.invoke("install_queue_confirm", json!({ "requestId": id }))
            .expect("confirm succeeds");
        assert_eq!(test.events(), 3);

        let id = test.submit_ready(&archive);
        test.invoke("install_queue_dismiss", json!({ "requestId": id }))
            .expect("dismiss succeeds");
        assert_eq!(test.events(), 6);
    }

    #[test]
    fn no_command_opens_a_window() {
        let test = TestApp::new();
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");

        let id = test.submit_ready(&archive);
        test.invoke("install_queue_confirm", json!({ "requestId": id }))
            .expect("confirm succeeds");

        assert_eq!(test.app.webview_windows().len(), 1);
    }
}
