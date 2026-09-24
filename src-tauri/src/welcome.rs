// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Welcome window
//
// Introduces Torchsnap and asks the few questions that matter on the
// first day: the shortcut, launch at login and automatic update
// checks. It opens at startup while the stored revision is lower than
// `WELCOME_REVISION`, and from Settings on request.
//
// The window cannot be closed before its last step. That step finishes
// the welcome in two ways: pressing the launcher shortcut, or the
// "Open the launcher" button for when the shortcut does not fire.
// Finishing stores the update answer and the revision, closes the
// window and shows the launcher. Quitting Torchsnap before that stores
// nothing, so the welcome opens again at the next start.
// =========================================================

use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

/// Bumped when the welcome changes enough to be shown again to
/// everyone who saw an earlier one.
pub const WELCOME_REVISION: u64 = 1;
const SEEN_REVISION_KEY: &str = "welcome.seenRevision";

/// Pause between finishing the welcome and the first automatic update
/// check, so an update window does not jump in right as the launcher
/// opens.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(60);

/// Whether the welcome has to be shown, from the stored revision. A
/// missing or malformed value counts as never seen.
pub fn welcome_due(stored: Option<&Value>) -> bool {
    stored
        .and_then(Value::as_u64)
        .is_none_or(|seen| seen < WELCOME_REVISION)
}

// =========================================================
// State
// =========================================================

/// The window's progress, as far as the backend needs it.
#[derive(Debug, Default)]
struct Inner {
    /// Set while the window shows its last step, with the update
    /// answer from the step before. Only then can it be finished.
    ready: Option<bool>,
    /// Set by finishing, so the close that follows is let through.
    finishing: bool,
}

/// What finishing the welcome does.
#[derive(Debug, PartialEq, Eq)]
struct Finish {
    automatic_checks: bool,
}

impl Inner {
    fn ready_to_finish(&mut self, automatic_checks: bool) {
        self.ready = Some(automatic_checks);
    }

    fn not_ready(&mut self) {
        self.ready = None;
    }

    /// Finish, if the last step is showing. `None` otherwise, which
    /// also makes a shortcut press outside the last step a normal one.
    fn finish(&mut self) -> Option<Finish> {
        let automatic_checks = self.ready.take()?;
        self.finishing = true;
        Some(Finish { automatic_checks })
    }

    fn may_close(&self) -> bool {
        self.finishing
    }

    fn closed(&mut self) {
        *self = Self::default();
    }
}

#[derive(Default)]
pub struct WelcomeState {
    inner: Mutex<Inner>,
}

impl WelcomeState {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .expect("welcome state lock is never poisoned")
    }
}

// =========================================================
// App glue
// =========================================================

use tauri::Manager as _;
use tauri_plugin_store::StoreExt as _;

/// Open the welcome window at startup when it is due.
pub fn show_if_due(app: &tauri::AppHandle) {
    let stored = app
        .store("settings.json")
        .ok()
        .and_then(|store| store.get(SEEN_REVISION_KEY));
    if welcome_due(stored.as_ref()) {
        crate::show_welcome_window(app);
    }
}

/// The launcher shortcut was pressed. On the welcome's last step this
/// finishes the welcome, which shows the launcher itself; true tells
/// the caller not to toggle the launcher on top of that.
pub fn launcher_shortcut_pressed(app: &tauri::AppHandle) -> bool {
    let finish = app
        .try_state::<WelcomeState>()
        .and_then(|state| state.lock().finish());
    match finish {
        Some(finish) => {
            complete(app, finish);
            true
        }
        None => false,
    }
}

/// Whether the welcome window may close now.
pub fn may_close(app: &tauri::AppHandle) -> bool {
    app.state::<WelcomeState>().lock().may_close()
}

/// The welcome window is gone; a reopened one starts over.
pub fn window_closed(app: &tauri::AppHandle) {
    app.state::<WelcomeState>().lock().closed();
}

fn complete(app: &tauri::AppHandle, finish: Finish) {
    match app.store("settings.json") {
        Ok(store) => {
            store.set(
                crate::updates::AUTOMATIC_CHECKS_KEY,
                finish.automatic_checks,
            );
            store.set(SEEN_REVISION_KEY, WELCOME_REVISION);
            if let Err(e) = store.save() {
                eprintln!("failed to save the welcome answers: {e:#}");
            }
        }
        Err(e) => eprintln!("failed to open the settings store for the welcome: {e:#}"),
    }
    crate::close_welcome_window(app);
    crate::show_launcher_window(app);
    if finish.automatic_checks {
        crate::updates::check_automatically_after(app, FIRST_CHECK_DELAY);
    }
}

/// The last step is showing, with the answer to automatic updates.
#[tauri::command]
pub fn welcome_ready_to_finish(state: tauri::State<'_, WelcomeState>, automatic_checks: bool) {
    state.lock().ready_to_finish(automatic_checks);
}

/// The user went back from the last step.
#[tauri::command]
pub fn welcome_not_ready(state: tauri::State<'_, WelcomeState>) {
    state.lock().not_ready();
}

/// "Open the launcher" on the last step.
#[tauri::command]
pub fn welcome_finish(app: tauri::AppHandle) -> Result<(), String> {
    let finish = app
        .state::<WelcomeState>()
        .lock()
        .finish()
        .ok_or("The welcome is not on its last step.")?;
    complete(&app, finish);
    Ok(())
}

/// "Show Welcome" in Settings.
#[tauri::command]
pub fn welcome_show(app: tauri::AppHandle) {
    crate::show_welcome_window(&app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn never_seen_is_due() {
        assert!(welcome_due(None));
    }

    #[test]
    fn an_older_revision_is_due() {
        assert!(welcome_due(Some(&json!(WELCOME_REVISION - 1))));
    }

    #[test]
    fn the_current_revision_is_not_due() {
        assert!(!welcome_due(Some(&json!(WELCOME_REVISION))));
    }

    #[test]
    fn a_newer_revision_is_not_due() {
        assert!(!welcome_due(Some(&json!(WELCOME_REVISION + 1))));
    }

    #[test]
    fn a_malformed_revision_is_due() {
        assert!(welcome_due(Some(&json!("1"))));
        assert!(welcome_due(Some(&json!(-1))));
    }

    #[test]
    fn cannot_finish_before_the_last_step() {
        let mut inner = Inner::default();
        assert_eq!(inner.finish(), None);
        assert!(!inner.may_close());
    }

    #[test]
    fn finishing_on_the_last_step_carries_the_update_answer() {
        let mut inner = Inner::default();
        inner.ready_to_finish(false);
        assert_eq!(
            inner.finish(),
            Some(Finish {
                automatic_checks: false
            })
        );
        assert!(inner.may_close());
    }

    #[test]
    fn going_back_from_the_last_step_cannot_finish() {
        let mut inner = Inner::default();
        inner.ready_to_finish(true);
        inner.not_ready();
        assert_eq!(inner.finish(), None);
    }

    #[test]
    fn the_latest_answer_counts() {
        let mut inner = Inner::default();
        inner.ready_to_finish(true);
        inner.ready_to_finish(false);
        assert_eq!(
            inner.finish(),
            Some(Finish {
                automatic_checks: false
            })
        );
    }

    #[test]
    fn finishing_happens_once() {
        let mut inner = Inner::default();
        inner.ready_to_finish(true);
        assert!(inner.finish().is_some());
        assert_eq!(inner.finish(), None);
    }

    #[test]
    fn a_closed_window_starts_over() {
        let mut inner = Inner::default();
        inner.ready_to_finish(true);
        inner.finish();
        inner.closed();
        assert!(!inner.may_close());
        assert_eq!(inner.finish(), None);
    }
}
