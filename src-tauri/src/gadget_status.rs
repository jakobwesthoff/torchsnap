// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Gadget enable failures
//
// The stored `enabled.<id>` setting is what the user chose; whether
// the gadget runs is a separate, runtime fact. When `enable()` fails
// the host deactivates the gadget but leaves the setting on, so a
// passing failure does not switch the gadget off for good. The error
// is kept here per gadget id, and Settings show it under the gadget's
// switch until the gadget starts or the user turns it off.
// =========================================================

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::gadgets::Gadget;

/// Emitted to every window when a gadget's enable failure is recorded
/// or cleared.
pub const GADGET_ENABLE_FAILURES_CHANGED: &str = "gadget-enable-failures-changed";

/// Why gadgets that are turned on failed to start, keyed by gadget id.
#[derive(Default)]
pub struct EnableFailures {
    failures: Mutex<BTreeMap<String, String>>,
    on_change: OnceLock<Box<dyn Fn() + Send + Sync>>,
}

impl EnableFailures {
    /// Run `on_change` after every change to the failures. Set once,
    /// when the app handle that announces the change exists.
    pub fn set_on_change(&self, on_change: impl Fn() + Send + Sync + 'static) {
        let _ = self.on_change.set(Box::new(on_change));
    }

    pub fn record(&self, gadget_id: &str, reason: String) {
        let previous = self.lock().insert(gadget_id.to_string(), reason.clone());
        let changed = previous.as_ref() != Some(&reason);
        self.announce(changed);
    }

    pub fn clear(&self, gadget_id: &str) {
        let changed = self.lock().remove(gadget_id).is_some();
        self.announce(changed);
    }

    pub fn snapshot(&self) -> BTreeMap<String, String> {
        self.lock().clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, String>> {
        self.failures
            .lock()
            .expect("enable failures lock is never poisoned")
    }

    /// Runs after the lock is released, so `on_change` may read the
    /// failures itself.
    fn announce(&self, changed: bool) {
        if changed && let Some(on_change) = self.on_change.get() {
            on_change();
        }
    }
}

/// Start `gadget` and keep its failure, if any. Returns whether
/// `enable()` failed.
///
/// A retry in flight is not a failure, so the earlier record goes
/// before `enable()` runs. A failure is recorded only while the
/// gadget is still meant to run: when the user turned it off during
/// `enable()`, `active` is already `false` and the gadget is simply
/// off.
pub(crate) fn enable_gadget(
    gadget: &dyn Gadget,
    active: &AtomicBool,
    failures: &EnableFailures,
) -> bool {
    let id = gadget.id();
    failures.clear(id);

    let Err(e) = gadget.enable() else {
        return false;
    };
    eprintln!("gadget {id}: enable failed: {e:#}");
    if active
        .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        failures.record(id, format!("{e:#}"));
    }
    true
}

/// Apply a change of the stored `enabled.<id>` setting to the gadget:
/// turning it on starts it, turning a running gadget off stops it.
/// A gadget that is off is not failed, whether or not it started.
pub(crate) fn apply_enabled_setting(
    gadget: &dyn Gadget,
    active: &AtomicBool,
    failures: &EnableFailures,
    new_enabled: bool,
) {
    let was_active = active.swap(new_enabled, Ordering::SeqCst);
    if new_enabled && !was_active {
        enable_gadget(gadget, active, failures);
    } else if !new_enabled && was_active {
        gadget.disable();
    }
    if !new_enabled {
        failures.clear(gadget.id());
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc;

    use super::*;
    use crate::commands::types::PostAction;
    use crate::gadgets::ErasedSearch;

    /// A gadget whose `enable()` fails with `enable_error` when set.
    /// With a gate, `enable()` first announces that it started and
    /// then waits until the test releases it.
    struct TestGadget {
        enable_error: Mutex<Option<String>>,
        gate: Option<(mpsc::Sender<()>, Mutex<mpsc::Receiver<()>>)>,
        enables: AtomicUsize,
        disables: AtomicUsize,
    }

    impl TestGadget {
        fn new() -> Self {
            Self {
                enable_error: Mutex::new(None),
                gate: None,
                enables: AtomicUsize::new(0),
                disables: AtomicUsize::new(0),
            }
        }

        fn failing(reason: &str) -> Self {
            let gadget = Self::new();
            gadget.fail_with(Some(reason));
            gadget
        }

        fn fail_with(&self, reason: Option<&str>) {
            *self.enable_error.lock().expect("not poisoned") = reason.map(str::to_string);
        }

        /// Returns the receiver that reports "enable started" and the
        /// sender that lets `enable()` finish.
        fn gated(mut self) -> (Self, mpsc::Receiver<()>, mpsc::Sender<()>) {
            let (started_tx, started_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            self.gate = Some((started_tx, Mutex::new(release_rx)));
            (self, started_rx, release_tx)
        }
    }

    impl ErasedSearch for TestGadget {
        fn execute(&self, _command: &str) -> anyhow::Result<PostAction> {
            Ok(PostAction::Nothing)
        }
    }

    impl Gadget for TestGadget {
        fn id(&self) -> &str {
            "broken"
        }

        fn enable(&self) -> anyhow::Result<()> {
            self.enables.fetch_add(1, Ordering::SeqCst);
            if let Some((started, release)) = &self.gate {
                started.send(()).expect("test waits for the start");
                release
                    .lock()
                    .expect("not poisoned")
                    .recv()
                    .expect("test releases enable");
            }
            match self.enable_error.lock().expect("not poisoned").clone() {
                Some(reason) => Err(anyhow::anyhow!(reason).context("start the gadget")),
                None => Ok(()),
            }
        }

        fn disable(&self) {
            self.disables.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn failure_of(failures: &EnableFailures) -> Option<String> {
        failures.snapshot().get("broken").cloned()
    }

    // -----------------------------------------------------
    // enable_gadget
    // -----------------------------------------------------

    #[test]
    fn a_failed_enable_deactivates_the_gadget_and_records_the_reason() {
        let gadget = TestGadget::failing("no clipboard");
        let active = AtomicBool::new(true);
        let failures = EnableFailures::default();

        let failed = enable_gadget(&gadget, &active, &failures);

        assert!(failed);
        assert!(!active.load(Ordering::SeqCst));
        assert_eq!(
            failures.snapshot(),
            BTreeMap::from([(
                "broken".to_string(),
                "start the gadget: no clipboard".to_string()
            )])
        );
    }

    #[test]
    fn a_successful_enable_clears_an_earlier_failure() {
        let gadget = TestGadget::new();
        let active = AtomicBool::new(true);
        let failures = EnableFailures::default();
        failures.record("broken", "earlier".into());

        let failed = enable_gadget(&gadget, &active, &failures);

        assert!(!failed);
        assert!(active.load(Ordering::SeqCst));
        assert_eq!(failure_of(&failures), None);
    }

    #[test]
    fn a_retry_clears_the_failure_while_enable_runs() {
        let (gadget, started, release) = TestGadget::failing("no clipboard").gated();
        let gadget = Arc::new(gadget);
        let active = Arc::new(AtomicBool::new(true));
        let failures = Arc::new(EnableFailures::default());
        failures.record("broken", "earlier".into());

        let worker = {
            let (gadget, active, failures) = (
                Arc::clone(&gadget),
                Arc::clone(&active),
                Arc::clone(&failures),
            );
            std::thread::spawn(move || enable_gadget(&*gadget, &active, &failures))
        };
        started.recv().expect("enable starts");
        let while_running = failure_of(&failures);
        release.send(()).expect("enable waits");
        worker.join().expect("enable finishes");

        assert_eq!(while_running, None);
        assert_eq!(
            failure_of(&failures).as_deref(),
            Some("start the gadget: no clipboard")
        );
    }

    #[test]
    fn turning_a_gadget_off_while_its_enable_fails_records_nothing() {
        let (gadget, started, release) = TestGadget::failing("no clipboard").gated();
        let gadget = Arc::new(gadget);
        let active = Arc::new(AtomicBool::new(true));
        let failures = Arc::new(EnableFailures::default());

        let worker = {
            let (gadget, active, failures) = (
                Arc::clone(&gadget),
                Arc::clone(&active),
                Arc::clone(&failures),
            );
            std::thread::spawn(move || enable_gadget(&*gadget, &active, &failures))
        };
        started.recv().expect("enable starts");
        apply_enabled_setting(&*gadget, &active, &failures, false);
        release.send(()).expect("enable waits");
        let failed = worker.join().expect("enable finishes");

        assert!(failed);
        assert!(!active.load(Ordering::SeqCst));
        assert_eq!(failure_of(&failures), None);
    }

    // -----------------------------------------------------
    // apply_enabled_setting
    // -----------------------------------------------------

    #[test]
    fn turning_a_failed_gadget_off_clears_the_failure_without_disable() {
        let gadget = TestGadget::failing("no clipboard");
        let active = AtomicBool::new(true);
        let failures = EnableFailures::default();
        enable_gadget(&gadget, &active, &failures);

        apply_enabled_setting(&gadget, &active, &failures, false);

        assert_eq!(failure_of(&failures), None);
        assert_eq!(gadget.disables.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn turning_a_failed_gadget_on_again_retries_enable() {
        let gadget = TestGadget::failing("no clipboard");
        let active = AtomicBool::new(true);
        let failures = EnableFailures::default();
        enable_gadget(&gadget, &active, &failures);
        gadget.fail_with(None);

        apply_enabled_setting(&gadget, &active, &failures, false);
        apply_enabled_setting(&gadget, &active, &failures, true);

        assert_eq!(gadget.enables.load(Ordering::SeqCst), 2);
        assert!(active.load(Ordering::SeqCst));
        assert_eq!(failure_of(&failures), None);
    }

    #[test]
    fn turning_a_failed_gadget_on_again_records_a_new_failure() {
        let gadget = TestGadget::failing("no clipboard");
        let active = AtomicBool::new(false);
        let failures = EnableFailures::default();

        apply_enabled_setting(&gadget, &active, &failures, true);

        assert!(!active.load(Ordering::SeqCst));
        assert_eq!(
            failure_of(&failures).as_deref(),
            Some("start the gadget: no clipboard")
        );
    }

    #[test]
    fn turning_a_running_gadget_off_calls_disable() {
        let gadget = TestGadget::new();
        let active = AtomicBool::new(true);
        let failures = EnableFailures::default();

        apply_enabled_setting(&gadget, &active, &failures, false);

        assert_eq!(gadget.disables.load(Ordering::SeqCst), 1);
        assert!(!active.load(Ordering::SeqCst));
    }

    #[test]
    fn turning_on_an_already_active_gadget_does_nothing() {
        let gadget = TestGadget::new();
        let active = AtomicBool::new(true);
        let failures = EnableFailures::default();

        apply_enabled_setting(&gadget, &active, &failures, true);

        assert_eq!(gadget.enables.load(Ordering::SeqCst), 0);
        assert_eq!(gadget.disables.load(Ordering::SeqCst), 0);
    }

    // -----------------------------------------------------
    // EnableFailures
    // -----------------------------------------------------

    #[test]
    fn on_change_runs_only_when_the_failures_change() {
        let failures = EnableFailures::default();
        let changes = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&changes);
        failures.set_on_change(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });

        failures.clear("broken");
        failures.record("broken", "no clipboard".into());
        failures.record("broken", "no clipboard".into());
        failures.record("broken", "no display".into());
        failures.clear("broken");
        failures.clear("broken");

        assert_eq!(changes.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn on_change_may_read_the_failures() {
        let failures = Arc::new(EnableFailures::default());
        let seen = Arc::new(Mutex::new(Vec::new()));
        {
            let reader = Arc::downgrade(&failures);
            let seen = Arc::clone(&seen);
            failures.set_on_change(move || {
                let failures = reader.upgrade().expect("failures outlive the callback");
                seen.lock().expect("not poisoned").push(failures.snapshot());
            });
        }

        failures.record("broken", "no clipboard".into());

        assert_eq!(
            *seen.lock().expect("not poisoned"),
            [BTreeMap::from([(
                "broken".to_string(),
                "no clipboard".to_string()
            )])]
        );
    }
}
