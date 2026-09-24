// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The install queue: every archive that reaches Torchsnap, from any
//! entry point, waits here for the user's review.
//!
//! A request moves through three states. `Staging` while its archive is
//! copied and parsed, `Ready` with a review to show, or `Failed` with
//! the reason. Confirming a ready request installs it and removes it;
//! dismissing removes it without installing.
//!
//! The queue exists before Tauri's `setup` runs, because macOS can
//! deliver a double-clicked file before the app is ready. Until `start`
//! hands it the install context it only buffers raw inputs.
//!
//! All methods are synchronous. `submit` and `start` return the ids
//! whose archives still need staging; callers run `process` for them on
//! a blocking worker, so the slow copy never holds the queue lock.
//!
//! Lock order: the queue lock, then the `PendingChanges` lock. Only
//! `confirm` takes both; install and uninstall commands take only the
//! second.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::Context;
use serde::Serialize;

use super::decision::{InstallDecision, decide_install};
use super::intake::{InstallOrigin, archive_path};
use super::paths::InstallPaths;
use super::pending::PendingChanges;
use super::provenance::read_provenance;
use super::registered::RegisteredGadgets;
use super::review::{InstallReview, build_review};
use super::staging::{StagedArchive, StagingArea};
use super::{InstalledGadgetInfo, install_staged};

pub type RequestId = String;

/// Everything processing and confirming need, handed over by `setup`.
#[derive(Clone)]
pub struct QueueContext {
    pub paths: InstallPaths,
    pub registered: RegisteredGadgets,
    pub pending: Arc<Mutex<PendingChanges>>,
    pub staging: StagingArea,
    /// Called after every change to the queue, outside its lock.
    pub on_change: Arc<dyn Fn() + Send + Sync>,
}

#[derive(Debug)]
pub enum Submitted {
    /// Added to the queue; its archive still needs `process`.
    Queued(RequestId),
    /// Held until `start`, which queues it.
    Buffered,
    /// Not an archive, or the same file is already queued.
    Ignored,
}

/// What `snapshot` reports for one request.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequestView {
    pub id: RequestId,
    pub origin: InstallOrigin,
    pub source_path: String,
    pub state: RequestStateView,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RequestStateView {
    Staging,
    Ready { review: Box<InstallReview> },
    Failed { message: String },
}

/// The part of a decision the user saw in the review. Confirming
/// re-decides and compares against this, so a review that no longer
/// describes what would happen is never acted on.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ReviewedDecision {
    Fresh,
    Replace { previous_version: String },
    Reject,
}

impl From<&InstallDecision> for ReviewedDecision {
    fn from(decision: &InstallDecision) -> Self {
        match decision {
            InstallDecision::Fresh => ReviewedDecision::Fresh,
            InstallDecision::Replace { previous, .. } => ReviewedDecision::Replace {
                previous_version: previous.gadget.version.clone(),
            },
            InstallDecision::Reject(_) => ReviewedDecision::Reject,
        }
    }
}

struct ReadyRequest {
    staged: StagedArchive,
    review: InstallReview,
    reviewed: ReviewedDecision,
}

enum RequestState {
    Staging,
    Ready(Box<ReadyRequest>),
    Failed(String),
}

struct Request {
    id: RequestId,
    origin: InstallOrigin,
    source_path: PathBuf,
    /// Dedup key: the same file reached through different paths
    /// (symlinks, `..`) is still the same request.
    canonical_path: PathBuf,
    state: RequestState,
}

enum State {
    Buffering(Vec<(PathBuf, InstallOrigin)>),
    Running {
        context: QueueContext,
        requests: Vec<Request>,
    },
}

pub struct InstallQueue {
    state: Mutex<State>,
}

impl Default for InstallQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl InstallQueue {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State::Buffering(Vec::new())),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .expect("install queue lock is never poisoned")
    }

    /// Accept one raw input from an entry point. Relative paths resolve
    /// against `cwd`.
    pub fn submit(&self, input: &OsStr, cwd: &Path, origin: InstallOrigin) -> Submitted {
        let Some(path) = archive_path(input, cwd) else {
            eprintln!(
                "ignoring install input `{}`: not a .torchsnap file",
                input.to_string_lossy()
            );
            return Submitted::Ignored;
        };

        let mut state = self.lock();
        match &mut *state {
            State::Buffering(buffered) => {
                buffered.push((path, origin));
                Submitted::Buffered
            }
            State::Running { context, requests } => {
                let Some(id) = add_request(requests, path, origin) else {
                    return Submitted::Ignored;
                };
                let on_change = Arc::clone(&context.on_change);
                drop(state);
                on_change();
                Submitted::Queued(id)
            }
        }
    }

    /// Hand over the install context and queue everything buffered so
    /// far. Returns the ids that need `process`.
    pub fn start(&self, context: QueueContext) -> Vec<RequestId> {
        let mut state = self.lock();
        let buffered = match &mut *state {
            State::Buffering(buffered) => std::mem::take(buffered),
            State::Running { .. } => return Vec::new(),
        };
        let mut requests = Vec::new();
        let ids: Vec<RequestId> = buffered
            .into_iter()
            .filter_map(|(path, origin)| add_request(&mut requests, path, origin))
            .collect();
        let on_change = Arc::clone(&context.on_change);
        *state = State::Running { context, requests };
        drop(state);
        if !ids.is_empty() {
            on_change();
        }
        ids
    }

    /// Stage the archive of request `id` and build its review. A request
    /// that is gone by the time staging finishes (dismissed meanwhile)
    /// has its copy discarded.
    pub fn process(&self, id: &str) {
        let (context, source_path) = {
            let state = self.lock();
            let State::Running { context, requests } = &*state else {
                return;
            };
            match requests.iter().find(|request| request.id == id) {
                Some(request) if matches!(request.state, RequestState::Staging) => {
                    (context.clone(), request.source_path.clone())
                }
                _ => return,
            }
        };

        let provenance = read_provenance(&source_path);
        let outcome = context.staging.stage(&source_path).map(|staged| {
            let decision = {
                let pending = context
                    .pending
                    .lock()
                    .expect("pending changes lock is never poisoned");
                let gadget_id = staged.manifest().gadget.id.as_str();
                decide_install(
                    context.registered.get(gadget_id),
                    pending.get(gadget_id),
                    staged.manifest(),
                )
            };
            let review = build_review(staged.manifest(), &source_path, provenance, &decision);
            (staged, review, ReviewedDecision::from(&decision))
        });

        let mut state = self.lock();
        let State::Running { requests, .. } = &mut *state else {
            return;
        };
        let Some(request) = requests.iter_mut().find(|request| request.id == id) else {
            if let Ok((staged, _, _)) = outcome {
                staged.discard();
            }
            return;
        };
        request.state = match outcome {
            Ok((staged, review, reviewed)) => RequestState::Ready(Box::new(ReadyRequest {
                staged,
                review,
                reviewed,
            })),
            Err(e) => RequestState::Failed(format!("{e:#}")),
        };
        drop(state);
        (context.on_change)();
    }

    /// Install the archive of a ready request.
    pub fn confirm(&self, id: &str) -> anyhow::Result<InstalledGadgetInfo> {
        let mut state = self.lock();
        let State::Running { context, requests } = &mut *state else {
            anyhow::bail!("unknown install request `{id}`");
        };
        let on_change = Arc::clone(&context.on_change);
        let index = requests
            .iter()
            .position(|request| request.id == id)
            .with_context(|| format!("unknown install request `{id}`"))?;
        let RequestState::Ready(ready) = &requests[index].state else {
            anyhow::bail!("install request `{id}` is not ready to be installed");
        };
        let (staged, reviewed) = (&ready.staged, &ready.reviewed);
        if *reviewed == ReviewedDecision::Reject {
            anyhow::bail!("this gadget cannot be installed; see the review for the reason");
        }

        let mut pending = context
            .pending
            .lock()
            .expect("pending changes lock is never poisoned");
        let gadget_id = staged.manifest().gadget.id.as_str();
        let current = ReviewedDecision::from(&decide_install(
            context.registered.get(gadget_id),
            pending.get(gadget_id),
            staged.manifest(),
        ));

        let result = if current != *reviewed {
            Err(anyhow::anyhow!(
                "the installed state of `{gadget_id}` changed since this review; open the file again to review it anew"
            ))
        } else {
            install_staged(&context.paths, &context.registered, &mut pending, staged)
        };
        drop(pending);

        match &result {
            Ok(_) => {
                let request = requests.remove(index);
                if let RequestState::Ready(ready) = request.state {
                    ready.staged.discard();
                }
            }
            Err(e) => {
                let failed = std::mem::replace(
                    &mut requests[index].state,
                    RequestState::Failed(format!("{e:#}")),
                );
                if let RequestState::Ready(ready) = failed {
                    ready.staged.discard();
                }
            }
        }
        drop(state);
        on_change();
        result
    }

    /// Drop a request without installing it.
    pub fn dismiss(&self, id: &str) -> anyhow::Result<()> {
        let mut state = self.lock();
        let State::Running { context, requests } = &mut *state else {
            anyhow::bail!("unknown install request `{id}`");
        };
        let index = requests
            .iter()
            .position(|request| request.id == id)
            .with_context(|| format!("unknown install request `{id}`"))?;
        let request = requests.remove(index);
        if let RequestState::Ready(ready) = request.state {
            ready.staged.discard();
        }
        let on_change = Arc::clone(&context.on_change);
        drop(state);
        on_change();
        Ok(())
    }

    /// Every request, in arrival order. Empty until `start`.
    pub fn snapshot(&self) -> Vec<InstallRequestView> {
        let state = self.lock();
        let State::Running { requests, .. } = &*state else {
            return Vec::new();
        };
        requests
            .iter()
            .map(|request| InstallRequestView {
                id: request.id.clone(),
                origin: request.origin,
                source_path: request.source_path.display().to_string(),
                state: match &request.state {
                    RequestState::Staging => RequestStateView::Staging,
                    RequestState::Ready(ready) => RequestStateView::Ready {
                        review: Box::new(ready.review.clone()),
                    },
                    RequestState::Failed(message) => RequestStateView::Failed {
                        message: message.clone(),
                    },
                },
            })
            .collect()
    }
}

/// Queue `path` unless the same file is already waiting.
fn add_request(
    requests: &mut Vec<Request>,
    path: PathBuf,
    origin: InstallOrigin,
) -> Option<RequestId> {
    let canonical_path = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    if requests
        .iter()
        .any(|request| request.canonical_path == canonical_path)
    {
        return None;
    }
    let id = ulid::Ulid::generate().to_string().to_lowercase();
    requests.push(Request {
        id: id.clone(),
        origin,
        source_path: path,
        canonical_path,
        state: RequestState::Staging,
    });
    Some(id)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::gadget_install::intake::InstallOrigin;
    use crate::gadget_install::paths::InstallPaths;
    use crate::gadget_install::pending::PendingChanges;
    use crate::gadget_install::registered::{RegisteredGadgets, Registration};
    use crate::gadget_install::staging::StagingArea;
    use crate::wasm::manifest::test_helpers::{
        archive_with_manifest, write_archive_without_manifest,
    };
    use crate::wasm::source::{ArchiveSource, GadgetSource};

    struct Fixture {
        root: tempfile::TempDir,
        queue: InstallQueue,
        context: QueueContext,
        changes: Arc<AtomicUsize>,
    }

    impl Fixture {
        fn new(registered: RegisteredGadgets) -> Self {
            let root = tempfile::tempdir().expect("create temp dir");
            let changes = Arc::new(AtomicUsize::new(0));
            let counter = Arc::clone(&changes);
            let context = QueueContext {
                paths: InstallPaths::new(&root.path().join("data")),
                registered,
                pending: Arc::new(Mutex::new(PendingChanges::default())),
                staging: StagingArea::new(root.path().join("staging")),
                on_change: Arc::new(move || {
                    counter.fetch_add(1, Ordering::SeqCst);
                }),
            };
            Self {
                root,
                queue: InstallQueue::new(),
                context,
                changes,
            }
        }

        fn started(registered: RegisteredGadgets) -> Self {
            let fixture = Self::new(registered);
            let buffered = fixture.queue.start(fixture.context.clone());
            assert!(buffered.is_empty());
            fixture
        }

        fn changes(&self) -> usize {
            self.changes.load(Ordering::SeqCst)
        }

        fn staged_files(&self) -> Vec<PathBuf> {
            std::fs::read_dir(self.root.path().join("staging"))
                .map(|entries| entries.map(|e| e.expect("dir entry").path()).collect())
                .unwrap_or_default()
        }

        fn submit(&self, path: &Path) -> RequestId {
            match self.queue.submit(
                path.as_os_str(),
                Path::new("/"),
                InstallOrigin::SettingsPicker,
            ) {
                Submitted::Queued(id) => id,
                other => panic!("expected a queued request, got {other:?}"),
            }
        }

        fn submit_and_process(&self, path: &Path) -> RequestId {
            let id = self.submit(path);
            self.queue.process(&id);
            id
        }
    }

    fn state_of(queue: &InstallQueue, id: &str) -> RequestStateView {
        queue
            .snapshot()
            .into_iter()
            .find(|request| request.id == id)
            .unwrap_or_else(|| panic!("request {id} not in the queue"))
            .state
    }

    fn registered_user(id: &str, version: &str) -> RegisteredGadgets {
        let (_dir, archive) = archive_with_manifest(id, version, "");
        let manifest = ArchiveSource::open(&archive)
            .expect("fixture opens")
            .manifest()
            .clone();
        RegisteredGadgets::from_entries([(
            id.to_string(),
            Registration::User {
                manifest: Box::new(manifest),
                is_directory: false,
            },
        )])
    }

    // =========================================================
    // Submitting
    // =========================================================

    #[test]
    fn requests_before_start_are_buffered_until_start() {
        let fixture = Fixture::new(RegisteredGadgets::default());
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");

        let submitted = fixture.queue.submit(
            archive.as_os_str(),
            Path::new("/"),
            InstallOrigin::OsOpenFile,
        );

        assert!(matches!(submitted, Submitted::Buffered));
        assert!(fixture.queue.snapshot().is_empty());

        let started = fixture.queue.start(fixture.context.clone());

        assert_eq!(started.len(), 1);
        let snapshot = fixture.queue.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].origin, InstallOrigin::OsOpenFile);
        assert_eq!(snapshot[0].state, RequestStateView::Staging);
    }

    #[test]
    fn inputs_that_are_not_archives_are_ignored() {
        let fixture = Fixture::started(RegisteredGadgets::default());

        let submitted = fixture.queue.submit(
            OsStr::new("/tmp/notes.txt"),
            Path::new("/"),
            InstallOrigin::SettingsDrop,
        );

        assert!(matches!(submitted, Submitted::Ignored));
        assert!(fixture.queue.snapshot().is_empty());
        assert_eq!(fixture.changes(), 0);
    }

    #[test]
    fn the_same_file_is_queued_once_while_pending() {
        let fixture = Fixture::started(RegisteredGadgets::default());
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");
        let first = fixture.submit(&archive);

        let again = fixture.queue.submit(
            archive.as_os_str(),
            Path::new("/"),
            InstallOrigin::OsOpenFile,
        );

        assert!(matches!(again, Submitted::Ignored));
        fixture.queue.dismiss(&first).expect("dismiss");
        assert!(matches!(
            fixture.queue.submit(
                archive.as_os_str(),
                Path::new("/"),
                InstallOrigin::OsOpenFile
            ),
            Submitted::Queued(_)
        ));
    }

    #[test]
    fn requests_keep_their_arrival_order() {
        let fixture = Fixture::started(RegisteredGadgets::default());
        let (_a, first) = archive_with_manifest("weather", "1.0.0", "");
        let (_b, second) = archive_with_manifest("calendar", "1.0.0", "");

        let first_id = fixture.submit(&first);
        let second_id = fixture.submit(&second);

        let ids: Vec<_> = fixture.queue.snapshot().into_iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![first_id, second_id]);
    }

    // =========================================================
    // Processing
    // =========================================================

    #[test]
    fn processing_stages_the_archive_and_builds_the_review() {
        let fixture = Fixture::started(RegisteredGadgets::default());
        let (_src, archive) =
            archive_with_manifest("weather", "1.0.0", "[permissions]\nclipboard = true\n");

        let id = fixture.submit_and_process(&archive);

        let RequestStateView::Ready { review } = state_of(&fixture.queue, &id) else {
            panic!("expected a ready request");
        };
        assert_eq!(review.gadget.id, "weather");
        assert_eq!(review.permissions.len(), 1);
        assert_eq!(fixture.staged_files().len(), 1);
    }

    #[test]
    fn a_broken_archive_fails_with_its_error() {
        let fixture = Fixture::started(RegisteredGadgets::default());
        let (_src, archive) = write_archive_without_manifest(&[("README.md", b"no manifest")]);
        let broken = archive.with_file_name("broken.torchsnap");
        std::fs::rename(&archive, &broken).expect("rename fixture");

        let id = fixture.submit_and_process(&broken);

        let RequestStateView::Failed { message } = state_of(&fixture.queue, &id) else {
            panic!("expected a failed request");
        };
        assert!(message.contains("staged gadget archive"), "{message}");
        assert!(fixture.staged_files().is_empty());
    }

    #[test]
    fn every_state_change_is_announced() {
        let fixture = Fixture::started(RegisteredGadgets::default());
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");

        let id = fixture.submit(&archive);
        assert_eq!(fixture.changes(), 1);
        fixture.queue.process(&id);
        assert_eq!(fixture.changes(), 2);
        fixture.queue.confirm(&id).expect("confirm");
        assert_eq!(fixture.changes(), 3);

        let other = fixture.submit_and_process(&archive);
        let before_dismiss = fixture.changes();
        fixture.queue.dismiss(&other).expect("dismiss");
        assert_eq!(fixture.changes(), before_dismiss + 1);
    }

    // =========================================================
    // Dismissing
    // =========================================================

    #[test]
    fn dismissing_removes_the_request_and_its_staged_copy() {
        let fixture = Fixture::started(RegisteredGadgets::default());
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");
        let id = fixture.submit_and_process(&archive);

        fixture.queue.dismiss(&id).expect("dismiss should succeed");

        assert!(fixture.queue.snapshot().is_empty());
        assert!(fixture.staged_files().is_empty());
    }

    /// Staging runs outside the queue lock, so a dismiss can land while
    /// it is still copying. The copy must not outlive the request.
    #[test]
    fn a_request_dismissed_during_staging_leaves_no_copy() {
        let fixture = Fixture::started(RegisteredGadgets::default());
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");
        let id = fixture.submit(&archive);

        fixture.queue.dismiss(&id).expect("dismiss should succeed");
        fixture.queue.process(&id);

        assert!(fixture.queue.snapshot().is_empty());
        assert!(fixture.staged_files().is_empty());
    }

    #[test]
    fn dismissing_an_unknown_request_is_an_error() {
        let fixture = Fixture::started(RegisteredGadgets::default());

        assert!(fixture.queue.dismiss("01unknown").is_err());
    }

    // =========================================================
    // Confirming
    // =========================================================

    #[test]
    fn confirming_installs_and_removes_the_request() {
        let fixture = Fixture::started(RegisteredGadgets::default());
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");
        let id = fixture.submit_and_process(&archive);

        let info = fixture.queue.confirm(&id).expect("confirm should succeed");

        assert_eq!(info.id, "weather");
        assert!(fixture.context.paths.archive("weather").exists());
        assert!(fixture.queue.snapshot().is_empty());
        assert!(fixture.staged_files().is_empty());
    }

    #[test]
    fn a_second_confirm_of_the_same_request_is_rejected() {
        let fixture = Fixture::started(RegisteredGadgets::default());
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");
        let id = fixture.submit_and_process(&archive);
        fixture.queue.confirm(&id).expect("first confirm");

        assert!(fixture.queue.confirm(&id).is_err());
    }

    #[test]
    fn confirming_a_request_that_is_not_ready_is_rejected() {
        let fixture = Fixture::started(RegisteredGadgets::default());
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");
        let staging = fixture.submit(&archive);

        let error = fixture.queue.confirm(&staging).expect_err("still staging");
        assert!(format!("{error:#}").contains("not ready"));

        let (_b, broken) = write_archive_without_manifest(&[("README.md", b"x")]);
        let failed = fixture.submit_and_process(&broken);
        assert!(fixture.queue.confirm(&failed).is_err());
    }

    #[test]
    fn confirming_a_rejected_review_is_refused() {
        let fixture = Fixture::started(RegisteredGadgets::from_entries([(
            "weather".to_string(),
            Registration::Builtin,
        )]));
        let (_src, archive) = archive_with_manifest("weather", "1.0.0", "");
        let id = fixture.submit_and_process(&archive);

        assert!(fixture.queue.confirm(&id).is_err());
        assert!(!fixture.context.paths.archive("weather").exists());
    }

    /// Two queued files for the same id: after the first one installs,
    /// the second one's review ("fresh install") no longer describes
    /// what confirming it would do.
    #[test]
    fn a_decision_that_changed_since_the_review_fails_the_request() {
        let fixture = Fixture::started(RegisteredGadgets::default());
        let (_a, v1) = archive_with_manifest("weather", "1.0.0", "");
        let (_b, v2) = archive_with_manifest("weather", "2.0.0", "");
        let first = fixture.submit_and_process(&v1);
        let second = fixture.submit_and_process(&v2);
        fixture.queue.confirm(&first).expect("first install");

        let error = fixture.queue.confirm(&second).expect_err("stale review");

        assert!(format!("{error:#}").contains("changed since"));
        assert!(matches!(
            state_of(&fixture.queue, &second),
            RequestStateView::Failed { .. }
        ));
    }

    #[test]
    fn an_uninstall_after_the_review_fails_the_request() {
        let fixture = Fixture::started(registered_user("weather", "1.0.0"));
        let (_src, v2) = archive_with_manifest("weather", "2.0.0", "");
        let id = fixture.submit_and_process(&v2);
        fixture
            .context
            .pending
            .lock()
            .expect("pending lock")
            .record_uninstall("weather", true);

        let error = fixture.queue.confirm(&id).expect_err("stale review");

        assert!(format!("{error:#}").contains("changed since"));
    }
}
