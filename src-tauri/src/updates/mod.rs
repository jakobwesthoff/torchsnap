// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Updates
//
// Torchsnap updates itself through `tauri-plugin-updater` (ADR 0053).
// The feed URL and the public key of the update signature come from
// `plugins.updater` in `tauri.conf.json`; the plugin verifies every
// downloaded archive against that key before installing it.
//
// One state machine, `UpdateState`, serves both ways a check starts:
//
// - Manual: the tray's "Check for Updates..." or the Settings button.
//   Opens the update window at once and reports every outcome there,
//   including "up to date" and errors. Ignores a skipped version.
// - Automatic: the scheduler, when `updates.automaticChecks` is on.
//   Opens the window only when an update is found that the user did
//   not skip; failures are only logged.
//
// "Later" closes the window and leaves the found update in memory; the
// tray item then names it until it is installed or skipped. Nothing of
// that survives a restart: the next check finds it again.
//
// The decisions are small pure functions next to the code that uses
// them, so the tests cover them without a running app.
// =========================================================

pub mod location;
pub mod release_notes;
pub mod schedule;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::Context;
use semver::Version;
use serde::Serialize;
use serde_json::Value;
use tauri::menu::MenuItem;
use tauri::{Emitter as _, Manager as _, Url};
use tauri_plugin_store::StoreExt as _;
use tauri_plugin_updater::{Update, UpdaterExt as _};

use location::LocationProblem;
use release_notes::ReleaseNotes;

// =========================================================
// Settings, events and fixed values
// =========================================================

/// `true` or `false` once the user answered in the welcome window or
/// flipped the switch in Settings. Missing means no automatic checks.
pub const AUTOMATIC_CHECKS_KEY: &str = "updates.automaticChecks";
/// Unix seconds of the last successful check, manual or automatic.
const LAST_CHECK_KEY: &str = "updates.lastCheck";
/// Version the user chose "Skip This Version" for.
const SKIPPED_VERSION_KEY: &str = "updates.skippedVersion";

/// Emitted with the new [`Phase`] whenever it changes.
pub const PHASE_CHANGED_EVENT: &str = "update-phase-changed";

/// Where the update window sends users whose copy cannot update itself.
const DOWNLOAD_URL: &str =
    "https://github.com/jakobwesthoff/torchsnap/releases/latest/download/Torchsnap.dmg";

/// Environment variable that points the updater at another feed, for
/// testing an update against a locally served feed. It works in every
/// build; release builds accept only https there, and the signature
/// check against the compiled-in public key still applies.
const FEED_OVERRIDE_VAR: &str = "TORCHSNAP_UPDATE_FEED";

/// Delay of the first automatic check after startup, so the check does
/// not compete with loading gadgets and the launcher.
const STARTUP_DELAY: Duration = Duration::from_secs(10);
/// How often the scheduler asks whether a check is due.
const SCHEDULER_TICK: Duration = Duration::from_secs(15 * 60);

const TRAY_CHECK_TEXT: &str = "Check for Updates...";

/// Marker file that makes the next start show the launcher once, so the
/// user sees Torchsnap came back after installing an update.
const SHOW_LAUNCHER_MARKER: &str = ".show-launcher-after-update";

// =========================================================
// State
// =========================================================

/// What the update window shows. Serialized for the frontend with a
/// `phase` tag.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "phase",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Phase {
    Idle,
    Checking,
    UpToDate {
        installed: String,
    },
    Available(Available),
    Downloading {
        version: String,
        downloaded: u64,
        total: Option<u64>,
    },
    Installing {
        version: String,
    },
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Available {
    pub installed: String,
    pub version: String,
    /// Every release between the installed and the offered version.
    pub releases: Vec<ReleaseNotes>,
    /// Gadget installs and uninstalls the restart will also apply.
    pub pending_gadget_changes: usize,
    /// Set when this copy cannot replace itself; the window then offers
    /// `download_url` instead of installing.
    pub location_problem: Option<LocationProblem>,
    pub download_url: String,
}

pub struct UpdateState {
    inner: Mutex<Inner<Update>>,
    tray_item: OnceLock<MenuItem<tauri::Wry>>,
}

/// The state transitions, free of the app so the tests drive them
/// directly. `U` is the plugin's `Update` handle in the app and a
/// stand-in in the tests.
///
/// The transitions do not change `phase` themselves: they return
/// [`Effects`], and the app applies the new phase through
/// `set_phase`, which also tells the windows and the tray.
struct Inner<U> {
    phase: Phase,
    /// The handle for the update in `Phase::Available`, needed to
    /// download and install it.
    update: Option<U>,
    /// Last failed check in this process, for the retry floor.
    last_failure: Option<i64>,
    check_running: bool,
}

/// How a check against the feed ended.
enum CheckEnd<U> {
    Found { update: U, available: Available },
    NothingNew { installed: String },
    Failed { message: String },
}

/// What the app does after a transition, in this order: set the phase,
/// remember the successful check, show the update window.
#[derive(Debug, Default, PartialEq)]
struct Effects {
    phase: Option<Phase>,
    record_last_check: bool,
    show_window: bool,
}

impl<U: Clone> Inner<U> {
    fn new() -> Self {
        Self {
            phase: Phase::Idle,
            update: None,
            last_failure: None,
            check_running: false,
        }
    }

    /// Start a check, or `None` when one runs or an update installs.
    fn begin_check(&mut self, trigger: Trigger) -> Option<Effects> {
        if is_busy(self.check_running, &self.phase) {
            return None;
        }
        self.check_running = true;
        Some(match trigger {
            Trigger::Manual => Effects {
                phase: Some(Phase::Checking),
                show_window: true,
                ..Effects::default()
            },
            Trigger::Automatic => Effects::default(),
        })
    }

    fn finish_check(
        &mut self,
        trigger: Trigger,
        end: CheckEnd<U>,
        skipped: Option<&str>,
        now: i64,
    ) -> Effects {
        self.check_running = false;
        let manual = trigger == Trigger::Manual;
        match end {
            CheckEnd::Found { update, available } => {
                self.last_failure = None;
                if !offers(trigger, &available.version, skipped) {
                    return Effects {
                        record_last_check: true,
                        ..Effects::default()
                    };
                }
                self.update = Some(update);
                Effects {
                    phase: Some(Phase::Available(available)),
                    record_last_check: true,
                    show_window: true,
                }
            }
            CheckEnd::NothingNew { installed } => {
                self.last_failure = None;
                Effects {
                    phase: manual.then_some(Phase::UpToDate { installed }),
                    record_last_check: true,
                    show_window: false,
                }
            }
            CheckEnd::Failed { message } => {
                self.last_failure = Some(now);
                Effects {
                    phase: manual.then_some(Phase::Failed { message }),
                    ..Effects::default()
                }
            }
        }
    }

    /// An update postponed with "Later" waits in the tray.
    fn has_postponed_update(&self) -> bool {
        matches!(self.phase, Phase::Available(_))
    }

    /// The update to install, if the phase allows installing.
    fn install_target(&self) -> Option<U> {
        if installable(&self.phase) {
            self.update.clone()
        } else {
            None
        }
    }

    /// Forget the offered update; returns its version for the skip list.
    fn skip(&mut self) -> Option<String> {
        self.update = None;
        skip_target(&self.phase)
    }

    /// A failed install drops the handle; the next check fetches a new
    /// one.
    fn install_failed(&mut self) {
        self.update = None;
    }
}

impl UpdateState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner::new()),
            tray_item: OnceLock::new(),
        }
    }

    /// Hand over the tray's "Check for Updates..." item, whose text
    /// names a postponed update.
    pub fn set_tray_item(&self, item: MenuItem<tauri::Wry>) {
        let _ = self.tray_item.set(item);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner<Update>> {
        self.inner
            .lock()
            .expect("update state lock is never poisoned")
    }
}

/// The tray item names an update that waits after "Later".
fn tray_text(phase: &Phase) -> String {
    match phase {
        Phase::Available(available) => format!("Update to {}...", available.version),
        _ => TRAY_CHECK_TEXT.to_string(),
    }
}

/// Replace the phase, tell the windows, and keep the tray text in step.
fn set_phase(app: &tauri::AppHandle, phase: Phase) {
    let state = app.state::<UpdateState>();
    state.lock().phase = phase.clone();

    if let Some(item) = state.tray_item.get()
        && let Err(e) = item.set_text(tray_text(&phase))
    {
        eprintln!("failed to update the tray item text: {e:#}");
    }
    if let Err(e) = app.emit(PHASE_CHANGED_EVENT, &phase) {
        eprintln!("failed to emit {PHASE_CHANGED_EVENT}: {e:#}");
    }
}

// =========================================================
// Checking
// =========================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trigger {
    Manual,
    Automatic,
}

/// A second check while one runs, or while an update installs, would
/// only race the first.
fn is_busy(check_running: bool, phase: &Phase) -> bool {
    check_running || matches!(phase, Phase::Downloading { .. } | Phase::Installing { .. })
}

/// Whether a found update is offered. Automatic checks keep quiet about
/// a skipped version; a manual check shows it anyway, so "Check for
/// Updates" never claims "up to date" while a newer version exists.
fn offers(trigger: Trigger, version: &str, skipped: Option<&str>) -> bool {
    trigger == Trigger::Manual || skipped != Some(version)
}

/// Tray item: show a postponed update, otherwise check now.
pub(crate) fn tray_clicked(app: &tauri::AppHandle) {
    let postponed = app.state::<UpdateState>().lock().has_postponed_update();
    if postponed {
        crate::show_update_window(app);
    } else {
        check_now(app);
    }
}

/// Start a manual check and show the update window for it.
pub(crate) fn check_now(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { run_check(&app, Trigger::Manual).await });
}

async fn run_check(app: &tauri::AppHandle, trigger: Trigger) {
    let state = app.state::<UpdateState>();
    let begun = state.lock().begin_check(trigger);
    let Some(effects) = begun else {
        if trigger == Trigger::Manual {
            crate::show_update_window(app);
        }
        return;
    };
    apply(app, effects);

    let end = match fetch_update(app).await {
        Ok(Some(update)) => CheckEnd::Found {
            available: describe_update(app, &update),
            update,
        },
        Ok(None) => CheckEnd::NothingNew {
            installed: app.package_info().version.to_string(),
        },
        Err(e) => {
            eprintln!("update check failed: {e}");
            CheckEnd::Failed {
                message: e.to_string(),
            }
        }
    };
    let skipped = skipped_version(app);
    let now = unix_now();
    let effects = state
        .lock()
        .finish_check(trigger, end, skipped.as_deref(), now);
    if effects.record_last_check {
        store_value(app, LAST_CHECK_KEY, now.into());
    }
    apply(app, effects);
}

/// Carry out the phase change and the window part of [`Effects`].
fn apply(app: &tauri::AppHandle, effects: Effects) {
    if let Some(phase) = effects.phase {
        set_phase(app, phase);
    }
    if effects.show_window {
        crate::show_update_window(app);
    }
}

/// Ask the feed. The error is already worded for the update window.
async fn fetch_update(app: &tauri::AppHandle) -> Result<Option<Update>, CheckError> {
    let mut builder = app.updater_builder();
    let feed = parse_feed_override(std::env::var(FEED_OVERRIDE_VAR).ok().as_deref())
        .map_err(CheckError::Setup)?;
    if let Some(feed) = feed {
        eprintln!("update feed overridden by {FEED_OVERRIDE_VAR}: {feed}");
        builder = builder.endpoints(vec![feed]).map_err(|e| {
            CheckError::Setup(anyhow::Error::new(e).context("use the overridden update feed"))
        })?;
    }
    let updater = builder
        .build()
        .map_err(|e| CheckError::Setup(anyhow::Error::new(e).context("create the updater")))?;
    updater.check().await.map_err(CheckError::Plugin)
}

fn describe_update(app: &tauri::AppHandle, update: &Update) -> Available {
    let location_problem = std::env::current_exe()
        .ok()
        .and_then(|exe| location::install_location_problem(&exe));
    available_from_feed(
        &update.current_version,
        &update.version,
        &update.raw_json,
        update.body.as_deref(),
        pending_gadget_changes(app),
        location_problem,
    )
}

/// Everything the update window shows about a found update.
fn available_from_feed(
    installed: &str,
    offered: &str,
    feed: &Value,
    offered_notes: Option<&str>,
    pending_gadget_changes: usize,
    location_problem: Option<LocationProblem>,
) -> Available {
    let releases = match (Version::parse(installed), Version::parse(offered)) {
        (Ok(installed), Ok(offered)) => {
            release_notes::notes_between(feed, &installed, &offered, offered_notes)
        }
        _ => vec![ReleaseNotes {
            version: offered.to_string(),
            date: None,
            notes: offered_notes.unwrap_or_default().to_string(),
        }],
    };
    Available {
        installed: installed.to_string(),
        version: offered.to_string(),
        releases,
        pending_gadget_changes,
        location_problem,
        download_url: DOWNLOAD_URL.to_string(),
    }
}

fn pending_gadget_changes(app: &tauri::AppHandle) -> usize {
    app.try_state::<std::sync::Arc<Mutex<crate::gadget_install::PendingChanges>>>()
        .map(|pending| {
            pending
                .lock()
                .expect("pending changes lock is never poisoned")
                .overview()
                .len()
        })
        .unwrap_or(0)
}

// =========================================================
// Errors, worded for the update window
// =========================================================

#[derive(Debug)]
enum CheckError {
    /// Our own setup failed (a malformed override, a bad config).
    Setup(anyhow::Error),
    Plugin(tauri_plugin_updater::Error),
}

impl std::fmt::Display for CheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Setup(e) => write!(f, "Torchsnap could not start the update check: {e:#}"),
            Self::Plugin(e) => f.write_str(&describe_plugin_error(e)),
        }
    }
}

/// The plugin's messages name its internals ("Could not fetch a valid
/// release JSON from the remote" for a 404), so the user-facing text
/// is chosen here by kind.
fn describe_plugin_error(error: &tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error;
    match error {
        Error::Reqwest(_) | Error::Network(_) | Error::ReleaseNotFound => {
            "Torchsnap could not get update information from torchsnap.app. \
             Check the internet connection and try again later."
                .to_string()
        }
        Error::TargetNotFound(_) | Error::TargetsNotFound(_) | Error::Serialization(_) => {
            "The update information on torchsnap.app is incomplete. Try again later.".to_string()
        }
        Error::Minisign(_)
        | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion => {
            "The downloaded update could not be verified, so it was not installed.".to_string()
        }
        Error::AuthenticationFailed => {
            "Installing the update needs an administrator password, and none was given.".to_string()
        }
        Error::Io(e) => format!("Torchsnap could not replace itself: {e}"),
        other => format!("Updating failed: {other}"),
    }
}

// =========================================================
// Window actions
// =========================================================

/// The phase for a window that opens or reloads.
#[tauri::command]
pub fn update_phase(state: tauri::State<'_, UpdateState>) -> Phase {
    state.lock().phase.clone()
}

#[tauri::command]
pub fn update_check(app: tauri::AppHandle) {
    check_now(&app);
}

/// Only an offered update in a place that can be updated installs.
fn installable(phase: &Phase) -> bool {
    matches!(phase, Phase::Available(available) if available.location_problem.is_none())
}

/// Download progress is reported about every percent, or every 256 KiB
/// when the server sends no length; the plugin reports every network
/// chunk.
fn progress_step(total: Option<u64>) -> u64 {
    total.map_or(256 * 1024, |total| (total / 100).max(1))
}

/// Download and install the found update, then restart.
#[tauri::command]
pub fn update_install(app: tauri::AppHandle) -> Result<(), String> {
    let update = app
        .state::<UpdateState>()
        .lock()
        .install_target()
        .ok_or("There is no update to install.")?;

    tauri::async_runtime::spawn(async move {
        let version = update.version.clone();
        set_phase(
            &app,
            Phase::Downloading {
                version: version.clone(),
                downloaded: 0,
                total: None,
            },
        );

        let mut downloaded: u64 = 0;
        let mut reported: u64 = 0;
        let result = update
            .download_and_install(
                |chunk, total| {
                    downloaded += chunk as u64;
                    if downloaded - reported >= progress_step(total) {
                        reported = downloaded;
                        set_phase(
                            &app,
                            Phase::Downloading {
                                version: version.clone(),
                                downloaded,
                                total,
                            },
                        );
                    }
                },
                || {
                    set_phase(
                        &app,
                        Phase::Installing {
                            version: version.clone(),
                        },
                    )
                },
            )
            .await;

        match result {
            Ok(()) => {
                if let Err(e) = write_show_launcher_marker(&app) {
                    eprintln!("failed to write the show-launcher marker: {e:#}");
                }
                app.restart();
            }
            Err(e) => {
                eprintln!("installing the update failed: {e:#}");
                app.state::<UpdateState>().lock().install_failed();
                set_phase(
                    &app,
                    Phase::Failed {
                        message: describe_plugin_error(&e),
                    },
                );
            }
        }
    });
    Ok(())
}

/// The version "Skip This Version" applies to.
fn skip_target(phase: &Phase) -> Option<String> {
    match phase {
        Phase::Available(available) => Some(available.version.clone()),
        _ => None,
    }
}

/// Never offer this version again in automatic checks.
#[tauri::command]
pub fn update_skip(app: tauri::AppHandle) {
    let version = app.state::<UpdateState>().lock().skip();
    if let Some(version) = version {
        store_value(&app, SKIPPED_VERSION_KEY, version.into());
    }
    set_phase(&app, Phase::Idle);
    crate::close_update_window(&app);
}

/// Closing the window after "up to date" or an error returns to idle;
/// an offered update keeps its phase, which is what "Later" means, and
/// so does one that is installing.
fn settles_on_close(phase: &Phase) -> bool {
    !matches!(
        phase,
        Phase::Available(_) | Phase::Downloading { .. } | Phase::Installing { .. }
    )
}

/// The update window was closed, by any of its controls.
pub fn update_window_closed(app: &tauri::AppHandle) {
    if settles_on_close(&app.state::<UpdateState>().lock().phase) {
        set_phase(app, Phase::Idle);
    }
}

// =========================================================
// Scheduler
// =========================================================

/// The check interval, honoring `TORCHSNAP_UPDATE_INTERVAL`. A bad
/// value is reported and the default used.
fn interval_from_env(value: Option<&str>) -> Duration {
    match schedule::parse_interval_override(value) {
        Ok(Some(interval)) => {
            eprintln!(
                "update interval overridden by {}: {}s",
                schedule::INTERVAL_OVERRIDE_VAR,
                interval.as_secs()
            );
            interval
        }
        Ok(None) => schedule::CHECK_INTERVAL,
        Err(e) => {
            eprintln!("{e:#}; using the default update interval");
            schedule::CHECK_INTERVAL
        }
    }
}

/// Run automatic checks for the rest of the session.
pub fn start_scheduler(app: tauri::AppHandle) {
    let interval = interval_from_env(
        std::env::var(schedule::INTERVAL_OVERRIDE_VAR)
            .ok()
            .as_deref(),
    );
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;
        loop {
            check_if_due(&app, interval).await;
            tokio::time::sleep(SCHEDULER_TICK.min(interval)).await;
        }
    });
}

async fn check_if_due(app: &tauri::AppHandle, interval: Duration) {
    if !automatic_checks_enabled(read_value(app, AUTOMATIC_CHECKS_KEY).as_ref()) {
        return;
    }
    let last_check = read_value(app, LAST_CHECK_KEY).and_then(|v| v.as_i64());
    let last_failure = app.state::<UpdateState>().lock().last_failure;
    if schedule::check_is_due(unix_now(), last_check, last_failure, interval) {
        run_check(app, Trigger::Automatic).await;
    }
}

/// Only an explicit `true` turns automatic checks on; a missing or
/// malformed value means the user has not agreed.
fn automatic_checks_enabled(value: Option<&Value>) -> bool {
    value.and_then(Value::as_bool).unwrap_or(false)
}

fn skipped_version(app: &tauri::AppHandle) -> Option<String> {
    read_value(app, SKIPPED_VERSION_KEY).and_then(|v| v.as_str().map(str::to_string))
}

// =========================================================
// Helpers
// =========================================================

fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn read_value(app: &tauri::AppHandle, key: &str) -> Option<Value> {
    app.store("settings.json").ok()?.get(key)
}

fn store_value(app: &tauri::AppHandle, key: &str, value: Value) {
    match app.store("settings.json") {
        Ok(store) => {
            store.set(key, value);
            if let Err(e) = store.save() {
                eprintln!("failed to save {key}: {e:#}");
            }
        }
        Err(e) => eprintln!("failed to open the settings store for {key}: {e:#}"),
    }
}

/// Parse the feed override. An unset or empty variable means no
/// override; anything else has to be a URL.
fn parse_feed_override(value: Option<&str>) -> anyhow::Result<Option<Url>> {
    match value.map(str::trim) {
        None | Some("") => Ok(None),
        Some(raw) => raw
            .parse::<Url>()
            .map(Some)
            .with_context(|| format!("parse {FEED_OVERRIDE_VAR} value '{raw}' as a URL")),
    }
}

fn show_launcher_marker(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(SHOW_LAUNCHER_MARKER)
}

fn write_show_launcher_marker(app: &tauri::AppHandle) -> anyhow::Result<()> {
    let dir = app.path().app_data_dir().context("resolve app data dir")?;
    write_show_launcher_marker_in(&dir)
}

fn write_show_launcher_marker_in(app_data_dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(app_data_dir).context("create the app data directory")?;
    std::fs::write(show_launcher_marker(app_data_dir), b"")
        .context("write the show-launcher marker")
}

/// Consume the marker an update left before restarting. True when it
/// was there.
pub fn take_show_launcher_marker(app_data_dir: &Path) -> anyhow::Result<bool> {
    match std::fs::remove_file(show_launcher_marker(app_data_dir)) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).context("remove the show-launcher marker"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn available(version: &str, location_problem: Option<LocationProblem>) -> Phase {
        Phase::Available(Available {
            installed: "0.11.1".into(),
            version: version.into(),
            releases: vec![],
            pending_gadget_changes: 0,
            location_problem,
            download_url: DOWNLOAD_URL.into(),
        })
    }

    fn downloading() -> Phase {
        Phase::Downloading {
            version: "0.12.0".into(),
            downloaded: 0,
            total: None,
        }
    }

    fn installing() -> Phase {
        Phase::Installing {
            version: "0.12.0".into(),
        }
    }

    // ---- feed override ----

    #[test]
    fn unset_or_blank_override_means_the_configured_feed() {
        assert!(parse_feed_override(None).unwrap().is_none());
        assert!(parse_feed_override(Some("  ")).unwrap().is_none());
    }

    #[test]
    fn override_is_parsed_as_url() {
        let url = parse_feed_override(Some("https://example.org/feed.json"))
            .unwrap()
            .expect("a set variable is an override");
        assert_eq!(url.as_str(), "https://example.org/feed.json");
    }

    #[test]
    fn malformed_override_is_an_error() {
        assert!(parse_feed_override(Some("not a url")).is_err());
    }

    // ---- interval override ----

    #[test]
    fn interval_defaults_and_overrides() {
        assert_eq!(interval_from_env(None), schedule::CHECK_INTERVAL);
        assert_eq!(interval_from_env(Some("90")), Duration::from_secs(90));
    }

    #[test]
    fn bad_interval_falls_back_to_the_default() {
        assert_eq!(interval_from_env(Some("soon")), schedule::CHECK_INTERVAL);
    }

    // ---- decisions ----

    #[test]
    fn automatic_checks_need_an_explicit_true() {
        assert!(!automatic_checks_enabled(None));
        assert!(!automatic_checks_enabled(Some(&json!(false))));
        assert!(!automatic_checks_enabled(Some(&json!("true"))));
        assert!(automatic_checks_enabled(Some(&json!(true))));
    }

    #[test]
    fn automatic_checks_keep_quiet_about_a_skipped_version() {
        assert!(!offers(Trigger::Automatic, "0.12.0", Some("0.12.0")));
        assert!(offers(Trigger::Automatic, "0.12.1", Some("0.12.0")));
        assert!(offers(Trigger::Automatic, "0.12.0", None));
    }

    #[test]
    fn manual_checks_offer_a_skipped_version() {
        assert!(offers(Trigger::Manual, "0.12.0", Some("0.12.0")));
    }

    #[test]
    fn busy_while_checking_downloading_or_installing() {
        assert!(is_busy(true, &Phase::Idle));
        assert!(is_busy(false, &downloading()));
        assert!(is_busy(false, &installing()));
        assert!(!is_busy(false, &Phase::Idle));
        assert!(!is_busy(false, &available("0.12.0", None)));
        assert!(!is_busy(
            false,
            &Phase::Failed {
                message: "x".into()
            }
        ));
    }

    #[test]
    fn only_an_offered_update_in_an_updatable_place_installs() {
        assert!(installable(&available("0.12.0", None)));
        assert!(!installable(&available(
            "0.12.0",
            Some(LocationProblem::DiskImage)
        )));
        assert!(!installable(&Phase::Idle));
        assert!(!installable(&downloading()));
    }

    #[test]
    fn skip_applies_to_the_offered_version_only() {
        assert_eq!(
            skip_target(&available("0.12.0", None)).as_deref(),
            Some("0.12.0")
        );
        assert_eq!(skip_target(&Phase::Idle), None);
        assert_eq!(skip_target(&Phase::Checking), None);
    }

    #[test]
    fn closing_settles_finished_phases_only() {
        assert!(settles_on_close(&Phase::Checking));
        assert!(settles_on_close(&Phase::UpToDate {
            installed: "0.12.0".into()
        }));
        assert!(settles_on_close(&Phase::Failed {
            message: "x".into()
        }));
        assert!(!settles_on_close(&available("0.12.0", None)));
        assert!(!settles_on_close(&downloading()));
        assert!(!settles_on_close(&installing()));
    }

    #[test]
    fn tray_names_a_postponed_update() {
        assert_eq!(tray_text(&available("0.12.0", None)), "Update to 0.12.0...");
        assert_eq!(tray_text(&Phase::Idle), TRAY_CHECK_TEXT);
        assert_eq!(tray_text(&downloading()), TRAY_CHECK_TEXT);
    }

    #[test]
    fn progress_is_reported_about_every_percent() {
        assert_eq!(progress_step(Some(28_000_000)), 280_000);
        assert_eq!(progress_step(Some(50)), 1);
        assert_eq!(progress_step(None), 256 * 1024);
    }

    // ---- transitions ----

    const NOW: i64 = 1_790_000_000;

    fn found(version: &str) -> CheckEnd<u32> {
        let Phase::Available(available) = available(version, None) else {
            unreachable!("available() builds Phase::Available")
        };
        CheckEnd::Found {
            update: 7,
            available,
        }
    }

    #[test]
    fn manual_check_shows_the_window_while_checking() {
        let mut inner = Inner::<u32>::new();
        let effects = inner
            .begin_check(Trigger::Manual)
            .expect("idle state starts a check");
        assert_eq!(
            effects,
            Effects {
                phase: Some(Phase::Checking),
                record_last_check: false,
                show_window: true
            }
        );
        assert!(inner.check_running);
    }

    #[test]
    fn automatic_check_starts_silently() {
        let mut inner = Inner::<u32>::new();
        assert_eq!(
            inner.begin_check(Trigger::Automatic),
            Some(Effects::default())
        );
    }

    #[test]
    fn no_second_check_while_one_runs() {
        let mut inner = Inner::<u32>::new();
        inner
            .begin_check(Trigger::Automatic)
            .expect("first check starts");
        assert_eq!(inner.begin_check(Trigger::Manual), None);
    }

    #[test]
    fn no_check_while_an_update_installs() {
        let mut inner = Inner::<u32>::new();
        inner.phase = downloading();
        assert_eq!(inner.begin_check(Trigger::Manual), None);
        inner.phase = installing();
        assert_eq!(inner.begin_check(Trigger::Automatic), None);
    }

    #[test]
    fn found_update_is_offered_and_kept() {
        let mut inner = Inner::<u32>::new();
        inner.begin_check(Trigger::Automatic);
        inner.last_failure = Some(NOW - 10);
        let effects = inner.finish_check(Trigger::Automatic, found("0.12.0"), None, NOW);
        assert_eq!(effects.phase, Some(available("0.12.0", None)));
        assert!(effects.record_last_check);
        assert!(effects.show_window);
        assert_eq!(inner.update, Some(7));
        assert_eq!(inner.last_failure, None);
        assert!(!inner.check_running);
    }

    #[test]
    fn automatic_check_keeps_a_skipped_update_to_itself() {
        let mut inner = Inner::<u32>::new();
        inner.begin_check(Trigger::Automatic);
        let effects = inner.finish_check(Trigger::Automatic, found("0.12.0"), Some("0.12.0"), NOW);
        assert_eq!(
            effects,
            Effects {
                phase: None,
                record_last_check: true,
                show_window: false
            }
        );
        assert_eq!(inner.update, None);
    }

    #[test]
    fn manual_check_offers_a_skipped_update() {
        let mut inner = Inner::<u32>::new();
        inner.begin_check(Trigger::Manual);
        let effects = inner.finish_check(Trigger::Manual, found("0.12.0"), Some("0.12.0"), NOW);
        assert_eq!(effects.phase, Some(available("0.12.0", None)));
        assert_eq!(inner.update, Some(7));
    }

    #[test]
    fn manual_check_reports_up_to_date() {
        let mut inner = Inner::<u32>::new();
        inner.begin_check(Trigger::Manual);
        let effects = inner.finish_check(
            Trigger::Manual,
            CheckEnd::NothingNew {
                installed: "0.12.0".into(),
            },
            None,
            NOW,
        );
        assert_eq!(
            effects,
            Effects {
                phase: Some(Phase::UpToDate {
                    installed: "0.12.0".into()
                }),
                record_last_check: true,
                show_window: false
            }
        );
    }

    #[test]
    fn automatic_check_without_news_changes_nothing_visible() {
        let mut inner = Inner::<u32>::new();
        inner.begin_check(Trigger::Automatic);
        let effects = inner.finish_check(
            Trigger::Automatic,
            CheckEnd::NothingNew {
                installed: "0.12.0".into(),
            },
            None,
            NOW,
        );
        assert_eq!(effects.phase, None);
        assert!(effects.record_last_check);
    }

    #[test]
    fn manual_failure_is_shown_and_remembered() {
        let mut inner = Inner::<u32>::new();
        inner.begin_check(Trigger::Manual);
        let effects = inner.finish_check(
            Trigger::Manual,
            CheckEnd::Failed {
                message: "offline".into(),
            },
            None,
            NOW,
        );
        assert_eq!(
            effects,
            Effects {
                phase: Some(Phase::Failed {
                    message: "offline".into()
                }),
                record_last_check: false,
                show_window: false
            }
        );
        assert_eq!(inner.last_failure, Some(NOW));
        assert!(!inner.check_running);
    }

    #[test]
    fn automatic_failure_is_only_remembered() {
        let mut inner = Inner::<u32>::new();
        inner.begin_check(Trigger::Automatic);
        let effects = inner.finish_check(
            Trigger::Automatic,
            CheckEnd::Failed {
                message: "offline".into(),
            },
            None,
            NOW,
        );
        assert_eq!(effects, Effects::default());
        assert_eq!(inner.last_failure, Some(NOW));
    }

    #[test]
    fn an_offered_update_is_postponed_until_installed_or_skipped() {
        let mut inner = Inner::<u32>::new();
        assert!(!inner.has_postponed_update());
        inner.phase = available("0.12.0", None);
        assert!(inner.has_postponed_update());
    }

    #[test]
    fn install_target_needs_an_installable_phase() {
        let mut inner = Inner::<u32>::new();
        inner.update = Some(7);
        assert_eq!(inner.install_target(), None);
        inner.phase = available("0.12.0", None);
        assert_eq!(inner.install_target(), Some(7));
        inner.phase = available("0.12.0", Some(LocationProblem::DiskImage));
        assert_eq!(inner.install_target(), None);
    }

    #[test]
    fn skip_forgets_the_update_and_names_its_version() {
        let mut inner = Inner::<u32>::new();
        inner.phase = available("0.12.0", None);
        inner.update = Some(7);
        assert_eq!(inner.skip().as_deref(), Some("0.12.0"));
        assert_eq!(inner.update, None);
    }

    #[test]
    fn a_failed_install_drops_the_handle() {
        let mut inner = Inner::<u32>::new();
        inner.update = Some(7);
        inner.install_failed();
        assert_eq!(inner.update, None);
    }

    // ---- what the window shows ----

    #[test]
    fn available_lists_the_releases_in_between() {
        let feed = json!({ "releases": [
            { "version": "0.12.1", "date": "2026-10-10", "notes": "b" },
            { "version": "0.12.0", "date": "2026-10-02", "notes": "a" },
            { "version": "0.11.1", "date": "2026-09-24", "notes": "old" },
        ]});
        let available = available_from_feed("0.11.1", "0.12.1", &feed, Some("b"), 3, None);
        let versions: Vec<_> = available
            .releases
            .iter()
            .map(|r| r.version.as_str())
            .collect();
        assert_eq!(versions, ["0.12.1", "0.12.0"]);
        assert_eq!(available.pending_gadget_changes, 3);
        assert_eq!(available.download_url, DOWNLOAD_URL);
    }

    #[test]
    fn unparsable_versions_fall_back_to_the_offered_notes() {
        let available = available_from_feed(
            "not semver",
            "0.12.0",
            &json!({}),
            Some("notes"),
            0,
            Some(LocationProblem::Translocated),
        );
        assert_eq!(
            available.releases,
            [ReleaseNotes {
                version: "0.12.0".into(),
                date: None,
                notes: "notes".into()
            }]
        );
        assert_eq!(
            available.location_problem,
            Some(LocationProblem::Translocated)
        );
    }

    // ---- error wording ----

    #[test]
    fn unreachable_feed_is_worded_for_people() {
        let message = describe_plugin_error(&tauri_plugin_updater::Error::ReleaseNotFound);
        assert!(
            message.contains("could not get update information"),
            "{message}"
        );
    }

    #[test]
    fn incomplete_feed_is_named() {
        let message = describe_plugin_error(&tauri_plugin_updater::Error::TargetNotFound(
            "darwin-aarch64".into(),
        ));
        assert!(message.contains("incomplete"), "{message}");
    }

    #[test]
    fn unverifiable_update_says_it_was_not_installed() {
        let message = describe_plugin_error(&tauri_plugin_updater::Error::MissingSignedVersion);
        assert!(message.contains("not installed"), "{message}");
    }

    #[test]
    fn refused_password_is_named() {
        let message = describe_plugin_error(&tauri_plugin_updater::Error::AuthenticationFailed);
        assert!(message.contains("administrator password"), "{message}");
    }

    #[test]
    fn io_errors_keep_their_detail() {
        let error = tauri_plugin_updater::Error::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "read-only volume",
        ));
        let message = describe_plugin_error(&error);
        assert!(message.contains("could not replace itself"), "{message}");
        assert!(message.contains("read-only volume"), "{message}");
    }

    #[test]
    fn other_errors_are_passed_through() {
        let message = describe_plugin_error(&tauri_plugin_updater::Error::EmptyEndpoints);
        assert!(message.starts_with("Updating failed:"), "{message}");
    }

    #[test]
    fn setup_errors_say_the_check_could_not_start() {
        let error = CheckError::Setup(anyhow::anyhow!("bad override"));
        assert!(
            error
                .to_string()
                .contains("could not start the update check")
        );
    }

    // ---- serialization for the frontend ----

    #[test]
    fn phase_is_tagged_for_the_frontend() {
        let json = serde_json::to_value(Phase::Downloading {
            version: "0.12.0".into(),
            downloaded: 5,
            total: Some(10),
        })
        .unwrap();
        assert_eq!(
            json,
            json!({
                "phase": "downloading",
                "version": "0.12.0",
                "downloaded": 5,
                "total": 10
            })
        );
        assert_eq!(
            serde_json::to_value(Phase::UpToDate {
                installed: "0.12.0".into()
            })
            .unwrap(),
            json!({ "phase": "upToDate", "installed": "0.12.0" })
        );
    }

    #[test]
    fn available_fields_are_camel_case() {
        let mut phase = available("0.12.0", Some(LocationProblem::Translocated));
        if let Phase::Available(available) = &mut phase {
            available.pending_gadget_changes = 2;
        }
        let json = serde_json::to_value(phase).unwrap();
        assert_eq!(json["phase"], "available");
        assert_eq!(json["pendingGadgetChanges"], 2);
        assert_eq!(json["locationProblem"], "translocated");
        assert_eq!(json["downloadUrl"], DOWNLOAD_URL);
    }

    // ---- show-launcher marker ----

    #[test]
    fn show_launcher_marker_is_consumed_once() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(!take_show_launcher_marker(dir.path()).unwrap());
        write_show_launcher_marker_in(dir.path()).unwrap();
        assert!(take_show_launcher_marker(dir.path()).unwrap());
        assert!(!take_show_launcher_marker(dir.path()).unwrap());
    }

    #[test]
    fn show_launcher_marker_creates_the_data_directory() {
        let dir = tempfile::tempdir().expect("temp dir");
        let nested = dir.path().join("not-yet-there");
        write_show_launcher_marker_in(&nested).unwrap();
        assert!(take_show_launcher_marker(&nested).unwrap());
    }
}
