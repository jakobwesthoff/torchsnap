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
pub use super::intake::InstallOrigin;
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

/// What a submission from outside the settings window produced.
#[derive(Debug, Default)]
pub struct Submission {
    /// Requests queued now; they still need `process_in_background`.
    pub queued: Vec<RequestId>,
    /// Archives held until `setup` starts the queue.
    pub buffered: usize,
}

impl Submission {
    /// True when nothing was an archive, so there is nothing to review.
    pub fn is_empty(&self) -> bool {
        self.queued.is_empty() && self.buffered == 0
    }

    fn record(&mut self, submitted: Submitted) {
        match submitted {
            Submitted::Queued(id) => self.queued.push(id),
            Submitted::Buffered => self.buffered += 1,
            Submitted::Ignored => {}
        }
    }
}

/// Queue every `.torchsnap` archive among `inputs`, the one intake all
/// OS entry points share. An input is a path or a `file://` URL;
/// relative paths resolve against `cwd`, and anything else is ignored
/// by the queue with a log line. `origin` is what the review shows as
/// the source of the request.
pub fn submit_inputs<I>(
    queue: &InstallQueue,
    inputs: I,
    cwd: &Path,
    origin: InstallOrigin,
) -> Submission
where
    I: IntoIterator,
    I::Item: AsRef<OsStr>,
{
    let mut submission = Submission::default();
    for input in inputs {
        submission.record(queue.submit(input.as_ref(), cwd, origin));
    }
    submission
}

/// Queue the archives named on a command line. `args` starts with the
/// program name; arguments starting with `-` are flags, not files.
/// Relative paths resolve against `cwd`, the directory the command was
/// run in. On Linux and Windows this is how a file association hands
/// over the opened file.
pub fn submit_command_line(
    queue: &InstallQueue,
    args: impl IntoIterator<Item = std::ffi::OsString>,
    cwd: &Path,
) -> Submission {
    let files = args
        .into_iter()
        .skip(1)
        .filter(|arg| !arg.as_encoded_bytes().starts_with(b"-"));
    submit_inputs(queue, files, cwd, InstallOrigin::CommandLine)
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
                platform: None,
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

    // =========================================================
    // Files opened by the OS
    // =========================================================

    #[test]
    fn opened_archive_urls_are_queued() {
        let test = TestApp::new();
        let queue = Arc::clone(test.app.state::<Arc<InstallQueue>>().inner());
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");
        let file_url = url::Url::from_file_path(&archive).expect("absolute path");

        let submitted = submit_inputs(
            &queue,
            [file_url.as_str()],
            Path::new("/"),
            InstallOrigin::OsOpenFile,
        );

        assert_eq!(submitted.queued.len(), 1);
        assert_eq!(submitted.buffered, 0);
        assert_eq!(queue.snapshot()[0].origin, InstallOrigin::OsOpenFile);
    }

    #[test]
    fn opened_urls_that_are_not_archives_are_ignored() {
        let queue = Arc::new(InstallQueue::new());

        let submitted = submit_inputs(
            &queue,
            [
                "https://example.com/weather.torchsnap",
                "file:///tmp/notes.txt",
            ],
            Path::new("/"),
            InstallOrigin::OsOpenFile,
        );

        assert!(submitted.is_empty());
    }

    /// Before `setup` starts the queue, opened files are held back;
    /// `setup` then queues them and opens the settings window itself.
    #[test]
    fn opened_files_before_start_are_buffered() {
        let queue = Arc::new(InstallQueue::new());

        let submitted = submit_inputs(
            &queue,
            ["file:///tmp/weather.torchsnap"],
            Path::new("/"),
            InstallOrigin::OsOpenFile,
        );

        assert!(submitted.queued.is_empty());
        assert_eq!(submitted.buffered, 1);
    }

    // =========================================================
    // Shared intake
    // =========================================================

    #[test]
    fn inputs_keep_the_origin_of_their_channel() {
        let test = TestApp::new();
        let queue = Arc::clone(test.app.state::<Arc<InstallQueue>>().inner());
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");

        submit_inputs(
            &queue,
            [archive.as_os_str()],
            Path::new("/"),
            InstallOrigin::CommandLine,
        );

        assert_eq!(queue.snapshot()[0].origin, InstallOrigin::CommandLine);
    }

    #[test]
    fn the_first_input_is_an_input_not_a_program_name() {
        let test = TestApp::new();
        let queue = Arc::clone(test.app.state::<Arc<InstallQueue>>().inner());
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");

        let submitted = submit_inputs(
            &queue,
            [archive.as_os_str()],
            Path::new("/"),
            InstallOrigin::OsOpenFile,
        );

        assert_eq!(submitted.queued.len(), 1);
    }

    /// Flags are the command line's business; the intake itself takes a
    /// file name that starts with a dash as the file it is.
    #[test]
    fn a_dash_prefixed_archive_name_is_queued() {
        let test = TestApp::new();
        let queue = Arc::clone(test.app.state::<Arc<InstallQueue>>().inner());
        let (src, _archive) = archive_with_manifest("weather", "1.0.0", "");
        let dashed = src.path().join("-weather.torchsnap");
        std::fs::write(&dashed, b"not read until processing").expect("write archive stand-in");

        let submitted = submit_inputs(
            &queue,
            [dashed.as_os_str()],
            Path::new("/"),
            InstallOrigin::CommandLine,
        );

        assert_eq!(submitted.queued.len(), 1);
    }

    #[test]
    fn the_same_archive_twice_is_queued_once() {
        let test = TestApp::new();
        let queue = Arc::clone(test.app.state::<Arc<InstallQueue>>().inner());
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");

        let submitted = submit_inputs(
            &queue,
            [archive.as_os_str(), archive.as_os_str()],
            Path::new("/"),
            InstallOrigin::OsOpenFile,
        );

        assert_eq!(submitted.queued.len(), 1);
        assert_eq!(queue.snapshot().len(), 1);
    }

    #[test]
    fn inputs_queue_in_order_and_skip_what_is_not_an_archive() {
        let test = TestApp::new();
        let queue = Arc::clone(test.app.state::<Arc<InstallQueue>>().inner());
        let (_src_a, first) = archive_with_manifest("weather", "1.0.0", "");
        let (_src_b, second) = archive_with_manifest("clock", "1.0.0", "");
        let first_url = url::Url::from_file_path(&first).expect("absolute path");
        let second_url = url::Url::from_file_path(&second).expect("absolute path");

        let submitted = submit_inputs(
            &queue,
            [
                first_url.as_str(),
                "file:///tmp/notes.txt",
                second_url.as_str(),
            ],
            Path::new("/"),
            InstallOrigin::OsOpenFile,
        );

        assert_eq!(submitted.queued.len(), 2);
        let snapshot = queue.snapshot();
        assert_eq!(snapshot[0].source_path, first.display().to_string());
        assert_eq!(snapshot[1].source_path, second.display().to_string());
    }

    #[test]
    fn no_inputs_submit_nothing() {
        let queue = Arc::new(InstallQueue::new());

        let submitted = submit_inputs(
            &queue,
            Vec::<&OsStr>::new(),
            Path::new("/"),
            InstallOrigin::OsOpenFile,
        );

        assert!(submitted.is_empty());
    }

    // =========================================================
    // Command line
    // =========================================================

    fn args(values: &[&str]) -> Vec<std::ffi::OsString> {
        values.iter().map(std::ffi::OsString::from).collect()
    }

    #[test]
    fn command_line_skips_the_program_name_and_flags() {
        let test = TestApp::new();
        let queue = Arc::clone(test.app.state::<Arc<InstallQueue>>().inner());
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");

        let submitted = submit_command_line(
            &queue,
            vec![
                std::ffi::OsString::from("/Applications/Torchsnap.app/Contents/MacOS/torchsnap"),
                std::ffi::OsString::from("--flag"),
                archive.clone().into_os_string(),
            ],
            Path::new("/"),
        );

        assert_eq!(submitted.queued.len(), 1);
        let snapshot = queue.snapshot();
        assert_eq!(snapshot[0].origin, InstallOrigin::CommandLine);
        assert_eq!(snapshot[0].source_path, archive.display().to_string());
    }

    #[test]
    fn command_line_paths_resolve_against_the_working_directory() {
        let test = TestApp::new();
        let queue = Arc::clone(test.app.state::<Arc<InstallQueue>>().inner());
        let (src, archive) = archive_with_manifest("weather", "1.0.0", "");
        let file_name = archive.file_name().expect("archive has a file name");

        submit_command_line(
            &queue,
            vec![
                std::ffi::OsString::from("torchsnap"),
                file_name.to_os_string(),
            ],
            src.path(),
        );

        assert_eq!(
            queue.snapshot()[0].source_path,
            archive.display().to_string()
        );
    }

    #[test]
    fn a_launch_without_files_submits_nothing() {
        let queue = Arc::new(InstallQueue::new());

        let submitted = submit_command_line(&queue, args(&["torchsnap"]), Path::new("/"));

        assert!(submitted.is_empty());
    }

    #[test]
    fn command_line_files_before_start_are_buffered() {
        let queue = Arc::new(InstallQueue::new());

        let submitted = submit_command_line(
            &queue,
            args(&["torchsnap", "/tmp/weather.torchsnap"]),
            Path::new("/"),
        );

        assert_eq!(submitted.buffered, 1);
        assert!(!submitted.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_command_line_paths_are_kept() {
        use std::os::unix::ffi::OsStrExt as _;

        let queue = Arc::new(InstallQueue::new());
        let raw = OsStr::from_bytes(b"/tmp/caf\xe9.torchsnap").to_os_string();

        let submitted = submit_command_line(
            &queue,
            vec![std::ffi::OsString::from("torchsnap"), raw],
            Path::new("/"),
        );

        assert_eq!(submitted.buffered, 1);
    }
}
