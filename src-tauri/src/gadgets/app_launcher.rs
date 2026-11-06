// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Application Launcher Gadget
//
// Catalog gadget that exposes installed applications as
// searchable entries. Applications are discovered at startup
// via the platform's `AppDiscovery` implementation and cached
// in memory. A background refresh runs every 5 minutes to
// pick up newly installed or removed applications without
// blocking the search path. A refresh that fails keeps the cached
// list and the icons. A search within 30 seconds of a failed
// refresh starts no new one, so a failing discovery costs one
// attempt per 30 seconds of searching, not one per keystroke.
//
// Icons are extracted via `AppDiscovery::icon` and
// cached on disk as WebP. The `IconCache` handles mtime-based
// invalidation and orphan cleanup.
//
// Actions:
//   - Open (primary): launch the application
//   - Reveal (secondary): show in file manager
// =========================================================

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;

use crate::caps::{CapRequest, OpenerPermissions, ProvisionedCaps};
use crate::commands::types::{CatalogEntry, EntryActions, EntryIcon, PostAction};
use crate::icons::IconCache;
use crate::platform::app_discovery::{AppDiscovery, DiscoveredApp};
use crate::storage::StorageKey;

use super::Search;

use super::Gadget;

/// How long before the cached app list is considered stale and
/// a background refresh is triggered.
const REFRESH_INTERVAL_SECS: i64 = 300; // 5 minutes

/// How long after a failed refresh searches start no new one.
const RETRY_INTERVAL_SECS: i64 = 30;

pub struct AppLauncherGadget {
    caps: Arc<ProvisionedCaps>,
    cache: Arc<RwLock<Vec<DiscoveredApp>>>,
    last_refresh: Arc<AtomicI64>,
    retry_not_before: Arc<AtomicI64>,
    refreshing: Arc<AtomicBool>,
    discovery: Arc<dyn AppDiscovery>,
}

impl AppLauncherGadget {
    pub fn cap_requests() -> Vec<CapRequest> {
        vec![
            CapRequest::IconCache,
            CapRequest::Opener {
                permissions: OpenerPermissions {
                    schemes: vec!["*".into()],
                    open_path: true,
                    reveal_path: true,
                },
            },
        ]
    }

    pub fn new(caps: Arc<ProvisionedCaps>, discovery: impl AppDiscovery + 'static) -> Self {
        Self {
            caps,
            cache: Arc::new(RwLock::new(Vec::new())),
            last_refresh: Arc::new(AtomicI64::new(0)),
            retry_not_before: Arc::new(AtomicI64::new(0)),
            refreshing: Arc::new(AtomicBool::new(false)),
            discovery: Arc::new(discovery),
        }
    }

    /// If `refresh_is_due`, spawn a background thread to re-discover
    /// applications. The current `entries()` call returns the existing
    /// (stale) cache immediately; the refresh result is available on
    /// subsequent calls. A failed refresh keeps the cache and sets the
    /// stamp that holds off the next attempt for `RETRY_INTERVAL_SECS`.
    ///
    /// Returns the thread's handle when one was spawned. `entries()`
    /// lets it go; tests join it.
    fn maybe_trigger_background_refresh(&self) -> Option<thread::JoinHandle<()>> {
        // Atomic compare-and-swap prevents multiple concurrent
        // refresh threads if entries() is called rapidly.
        if self
            .refreshing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
            .is_err()
        {
            return None;
        }

        // The due check runs while this search holds the flag, so it
        // sees the stamp a finishing refresh thread wrote before it
        // cleared the flag.
        let due = refresh_is_due(
            unix_now(),
            self.last_refresh.load(Ordering::SeqCst),
            self.retry_not_before.load(Ordering::SeqCst),
        );
        if !due {
            self.refreshing.store(false, Ordering::SeqCst);
            return None;
        }

        let cache = Arc::clone(&self.cache);
        let timestamp = Arc::clone(&self.last_refresh);
        let end = RefreshEnd {
            retry_not_before: Arc::clone(&self.retry_not_before),
            refreshing: Arc::clone(&self.refreshing),
            failed: true,
        };
        let discovery = Arc::clone(&self.discovery);
        let icon_cache = Arc::clone(self.caps.icon_cache());

        Some(thread::spawn(move || {
            // Naming the whole guard moves it into the thread; a
            // closure that only touched `end.failed` would capture
            // that field alone and drop the guard here, at once.
            let mut end = end;
            match discovery.discover() {
                Ok(mut apps) => {
                    let valid_keys = extract_icons(&icon_cache, &*discovery, &mut apps);
                    icon_cache.cleanup("app-launcher", &valid_keys);

                    let mut guard = cache.write().expect("app cache not poisoned");
                    *guard = apps;
                    timestamp.store(unix_now(), Ordering::Relaxed);
                    end.failed = false;
                }
                Err(e) => {
                    // Keep serving the old cache rather than clearing it.
                    eprintln!("background app discovery failed: {e:#}");
                }
            }
        }))
    }
}

/// What ends a refresh thread, on every way out of it including a
/// panic. A refresh that did not succeed holds off the next one for
/// `RETRY_INTERVAL_SECS`, and the refresh flag is cleared after the
/// stamp so a search that takes the flag sees the stamp. Without
/// this, a panic would leave the flag set and stop all refreshes
/// until the app restarts.
struct RefreshEnd {
    retry_not_before: Arc<AtomicI64>,
    refreshing: Arc<AtomicBool>,
    /// Cleared by the thread once the new list is published.
    failed: bool,
}

impl Drop for RefreshEnd {
    fn drop(&mut self) {
        if self.failed {
            self.retry_not_before
                .store(unix_now() + RETRY_INTERVAL_SECS, Ordering::SeqCst);
        }
        self.refreshing.store(false, Ordering::SeqCst);
    }
}

/// Whether a search may start a background refresh now.
///
/// A refresh is due `REFRESH_INTERVAL_SECS` after the last success,
/// unless a failed refresh set a retry stamp that still lies ahead.
/// The stamp only counts when it lies at most `RETRY_INTERVAL_SECS`
/// ahead: a stamp further out means the clock was set back since it
/// was written, and it must not hold a retry for longer than a
/// failure ever does.
fn refresh_is_due(now: i64, last_refresh: i64, retry_not_before: i64) -> bool {
    let retry_wait = retry_not_before - now;
    let in_retry_window = (1..=RETRY_INTERVAL_SECS).contains(&retry_wait);

    now - last_refresh >= REFRESH_INTERVAL_SECS && !in_retry_window
}

/// Run icon extraction for each discovered app, populating their
/// `icon_path` field and returning the set of valid cache keys
/// for orphan cleanup.
fn extract_icons(
    icon_cache: &IconCache,
    discovery: &dyn AppDiscovery,
    apps: &mut [DiscoveredApp],
) -> HashSet<StorageKey> {
    let mut valid_keys = HashSet::with_capacity(apps.len());

    for app in apps.iter_mut() {
        let key = StorageKey::new(&format!(
            "{}:{}",
            app.path.display(),
            app.bundle_id.as_deref().unwrap_or("")
        ));

        let source_mtime = std::fs::metadata(&app.path).and_then(|m| m.modified()).ok();

        if let Some(path) =
            icon_cache.ensure_icon("app-launcher", &key, source_mtime, || discovery.icon(app))
        {
            app.icon_path = Some(path);
        }
        valid_keys.insert(key);
    }

    valid_keys
}

impl Gadget for AppLauncherGadget {
    fn id(&self) -> &str {
        "app-launcher"
    }

    fn enable(&self) -> anyhow::Result<()> {
        let icon_cache = self.caps.icon_cache();

        // Called on a dedicated background thread by the host.
        //
        // Phase 1: Discover apps and publish immediately so search
        // results appear without waiting for icon extraction.
        // Phase 2: Extract icons in the background and update the
        // cache — icons fill in on subsequent searches.
        match self.discovery.discover() {
            Ok(mut apps) => {
                // Publish the app list right away with fallback icons.
                {
                    let mut guard = self.cache.write().expect("app cache not poisoned");
                    *guard = apps.clone();
                }
                self.last_refresh.store(unix_now(), Ordering::Relaxed);

                // Now extract icons (the slow part). Once done,
                // swap the cache with icon-enriched entries.
                let valid_keys = extract_icons(icon_cache, &*self.discovery, &mut apps);
                icon_cache.cleanup("app-launcher", &valid_keys);

                let mut guard = self.cache.write().expect("app cache not poisoned");
                *guard = apps;
            }
            Err(e) => {
                eprintln!("initial app discovery failed: {e:#}");
            }
        }
        Ok(())
    }
}

/// What an app entry's actions do. Both carry the app's id, the
/// path the opener works on (on macOS the `.app` bundle).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Command {
    Open(String),
    Reveal(String),
}

/// Enter opens the app; Cmd+Enter reveals it in the file manager.
fn app_actions(id: &str) -> EntryActions<Command> {
    EntryActions::new()
        .primary("Open", Command::Open(id.to_string()))
        .secondary("Reveal in Finder", Command::Reveal(id.to_string()))
}

impl Search for AppLauncherGadget {
    type Command = Command;

    fn entries(&self) -> Vec<CatalogEntry<Command>> {
        let _ = self.maybe_trigger_background_refresh();

        let cache = self.cache.read().expect("app cache not poisoned");

        cache
            .iter()
            .map(|app| CatalogEntry {
                id: app.id.clone(),
                title: app.name.clone(),
                subtitle: Some(app.path.to_string_lossy().into_owned()),
                icon: Some(
                    app.icon_path
                        .as_ref()
                        .map(|p| EntryIcon::AssetIcon(p.clone()))
                        .unwrap_or_else(|| EntryIcon::HeroIcon("rocket-launch".into())),
                ),
                keywords: vec![],
                actions: app_actions(&app.id),
            })
            .collect()
    }

    fn execute(&self, command: Command) -> anyhow::Result<PostAction> {
        let opener = self.caps.opener();

        match command {
            Command::Open(id) => {
                opener
                    .open_path(&id)
                    .map_err(|e| anyhow::anyhow!(e))
                    .context("open application")?;
            }
            Command::Reveal(id) => {
                opener
                    .reveal_path(&id)
                    .map_err(|e| anyhow::anyhow!(e))
                    .context("reveal application in file manager")?;
            }
        }

        Ok(PostAction::Dismiss)
    }
}

/// Current unix timestamp in seconds.
///
/// Returns 0 on clock skew (system clock before the Unix epoch), which
/// causes the affected entries to sort as if last-used in 1970 rather
/// than panicking.
fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::types::Action;
    use std::collections::VecDeque;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc::{Receiver, Sender, channel};

    #[test]
    fn app_opens_on_enter_and_reveals_on_cmd_enter() {
        let actions = app_actions("/Applications/Safari.app");
        assert_eq!(
            actions.primary,
            Some(Action::labeled(
                "Open",
                Command::Open("/Applications/Safari.app".into())
            ))
        );
        assert_eq!(
            actions.secondary,
            Some(Action::labeled(
                "Reveal in Finder",
                Command::Reveal("/Applications/Safari.app".into())
            ))
        );
        assert_eq!(actions.iter().count(), 2);
    }

    // =========================================================
    // Refresh scheduling
    //
    // The gadget runs against a scripted `AppDiscovery` and a real
    // `IconCache` on a temporary directory. Tests that need the
    // refresh thread's end call `maybe_trigger_background_refresh`
    // and join the handle it returns, so nothing waits on a timer.
    // =========================================================

    /// How long a test waits for the refresh thread to announce a
    /// call before it fails, so a refresh that never starts fails the
    /// test and does not hang the suite.
    const WAIT: Duration = Duration::from_secs(10);

    /// Answers `discover` from a script, one entry per call. Once
    /// the script is empty every call fails, so a refresh the test
    /// did not expect shows up in the call count and not as a panic
    /// in the refresh thread.
    struct ScriptedDiscovery {
        script: Mutex<VecDeque<anyhow::Result<Vec<DiscoveredApp>>>>,
        calls: Arc<AtomicUsize>,
        started: Option<Mutex<Sender<()>>>,
        release: Option<Mutex<Receiver<()>>>,
    }

    impl ScriptedDiscovery {
        fn new(script: Vec<anyhow::Result<Vec<DiscoveredApp>>>) -> (Self, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            let discovery = Self {
                script: Mutex::new(script.into()),
                calls: Arc::clone(&calls),
                started: None,
                release: None,
            };
            (discovery, calls)
        }

        /// Send a message on every call, as soon as it begins.
        fn announcing_calls(mut self) -> (Self, Receiver<()>) {
            let (tx, rx) = channel();
            self.started = Some(Mutex::new(tx));
            (self, rx)
        }

        /// Hold every call until the returned sender sends.
        fn held_until_released(mut self) -> (Self, Sender<()>) {
            let (tx, rx) = channel();
            self.release = Some(Mutex::new(rx));
            (self, tx)
        }
    }

    impl AppDiscovery for ScriptedDiscovery {
        fn discover(&self) -> anyhow::Result<Vec<DiscoveredApp>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(started) = &self.started {
                let _ = started.lock().expect("sender lock").send(());
            }
            if let Some(release) = &self.release {
                release
                    .lock()
                    .expect("receiver lock")
                    .recv()
                    .expect("the test releases the call");
            }
            self.script
                .lock()
                .expect("script lock")
                .pop_front()
                .unwrap_or_else(|| Err(anyhow::anyhow!("script exhausted")))
        }

        fn icon(&self, _app: &DiscoveredApp) -> anyhow::Result<Option<image::DynamicImage>> {
            Ok(Some(image::DynamicImage::new_rgba8(16, 16)))
        }
    }

    fn app(name: &str) -> DiscoveredApp {
        DiscoveredApp {
            id: format!("/Applications/{name}.app"),
            name: name.to_string(),
            path: PathBuf::from(format!("/Applications/{name}.app")),
            bundle_id: Some(format!("test.{name}")),
            icon_path: None,
        }
    }

    fn failure() -> anyhow::Result<Vec<DiscoveredApp>> {
        Err(anyhow::anyhow!("discovery failed"))
    }

    fn caps_with_icons_in(icon_dir: &Path) -> Arc<ProvisionedCaps> {
        Arc::new(ProvisionedCaps {
            opener: None,
            http: None,
            filesystem: None,
            command: None,
            clipboard: None,
            sql_storage: None,
            website_metadata: None,
            icon_cache: Some(Arc::new(IconCache::new(icon_dir.to_path_buf()))),
            settings: None,
            frecency: None,
            path_resolver: None,
        })
    }

    fn gadget_with(icon_dir: &Path, discovery: ScriptedDiscovery) -> AppLauncherGadget {
        AppLauncherGadget::new(caps_with_icons_in(icon_dir), discovery)
    }

    /// The ids of the cached apps, read without triggering a refresh.
    fn cached_ids(gadget: &AppLauncherGadget) -> Vec<String> {
        let cache = gadget.cache.read().expect("app cache not poisoned");
        cache.iter().map(|app| app.id.clone()).collect()
    }

    fn cached_icon_paths(gadget: &AppLauncherGadget) -> Vec<String> {
        let cache = gadget.cache.read().expect("app cache not poisoned");
        cache
            .iter()
            .filter_map(|app| app.icon_path.clone())
            .collect()
    }

    /// Every file below `dir`, recursively.
    fn files_below(dir: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return found;
        };
        for entry in entries {
            let path = entry.expect("directory entry is readable").path();
            if path.is_dir() {
                found.extend(files_below(&path));
            } else {
                found.push(path);
            }
        }
        found
    }

    fn trigger_and_join(gadget: &AppLauncherGadget) {
        gadget
            .maybe_trigger_background_refresh()
            .expect("a refresh was due and started")
            .join()
            .expect("refresh thread does not panic");
    }

    /// Clear both gates so the next trigger is due.
    fn make_due(gadget: &AppLauncherGadget) {
        gadget.last_refresh.store(0, Ordering::SeqCst);
        gadget.retry_not_before.store(0, Ordering::SeqCst);
    }

    #[test]
    fn a_failed_refresh_is_not_retried_by_the_next_search() {
        let dir = tempfile::tempdir().expect("temporary directory can be created");
        let (discovery, calls) = ScriptedDiscovery::new(vec![failure()]);
        let gadget = gadget_with(dir.path(), discovery);

        trigger_and_join(&gadget);
        let second = gadget.maybe_trigger_background_refresh();

        assert!(
            second.is_none(),
            "a search right after a failure spawned a refresh"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_failed_refresh_sets_the_retry_stamp_thirty_seconds_ahead() {
        let dir = tempfile::tempdir().expect("temporary directory can be created");
        let (discovery, _) = ScriptedDiscovery::new(vec![failure()]);
        let gadget = gadget_with(dir.path(), discovery);

        let before = unix_now();
        trigger_and_join(&gadget);
        let after = unix_now();

        let stamp = gadget.retry_not_before.load(Ordering::SeqCst);
        assert!(
            (before + RETRY_INTERVAL_SECS..=after + RETRY_INTERVAL_SECS).contains(&stamp),
            "stamp {stamp} outside [{before}+30, {after}+30]"
        );
    }

    #[test]
    fn a_failed_refresh_leaves_the_last_success_time_alone() {
        let dir = tempfile::tempdir().expect("temporary directory can be created");
        let (discovery, _) = ScriptedDiscovery::new(vec![failure()]);
        let gadget = gadget_with(dir.path(), discovery);
        gadget.last_refresh.store(1, Ordering::SeqCst);

        trigger_and_join(&gadget);

        assert_eq!(gadget.last_refresh.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_failed_refresh_is_retried_once_the_stamp_has_passed() {
        let dir = tempfile::tempdir().expect("temporary directory can be created");
        let (discovery, calls) = ScriptedDiscovery::new(vec![failure(), failure()]);
        let gadget = gadget_with(dir.path(), discovery);
        trigger_and_join(&gadget);
        gadget
            .retry_not_before
            .store(unix_now() - 1, Ordering::SeqCst);

        trigger_and_join(&gadget);

        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_failed_retry_after_a_failed_startup_backs_off() {
        let dir = tempfile::tempdir().expect("temporary directory can be created");
        let (discovery, calls) = ScriptedDiscovery::new(vec![failure(), failure()]);
        let gadget = gadget_with(dir.path(), discovery);
        gadget
            .enable()
            .expect("a failed discovery is logged, not raised");

        trigger_and_join(&gadget);
        let third = gadget.maybe_trigger_background_refresh();

        assert!(
            third.is_none(),
            "a search after the failed retry spawned a refresh"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn the_first_search_retries_a_failed_startup_discovery() {
        let dir = tempfile::tempdir().expect("temporary directory can be created");
        let (discovery, calls) = ScriptedDiscovery::new(vec![failure(), failure()]);
        let (discovery, started) = discovery.announcing_calls();
        let gadget = gadget_with(dir.path(), discovery);
        gadget
            .enable()
            .expect("a failed discovery is logged, not raised");
        started
            .recv_timeout(WAIT)
            .expect("enable() called discover");

        gadget.entries();

        started
            .recv_timeout(WAIT)
            .expect("the search started a second discovery");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_search_during_a_running_refresh_spawns_nothing() {
        let dir = tempfile::tempdir().expect("temporary directory can be created");
        let (discovery, calls) = ScriptedDiscovery::new(vec![failure()]);
        let (discovery, started) = discovery.announcing_calls();
        let (discovery, release) = discovery.held_until_released();
        let gadget = gadget_with(dir.path(), discovery);

        let running = gadget
            .maybe_trigger_background_refresh()
            .expect("the first search starts a refresh");
        started
            .recv_timeout(WAIT)
            .expect("the refresh reached discover");
        assert!(
            gadget.refreshing.load(Ordering::SeqCst),
            "the flag is held while the refresh runs"
        );
        assert_eq!(gadget.retry_not_before.load(Ordering::SeqCst), 0);
        let second = gadget.maybe_trigger_background_refresh();
        release.send(()).expect("the refresh is waiting");
        running.join().expect("refresh thread does not panic");

        assert!(
            second.is_none(),
            "a search during a refresh spawned another"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    /// A discovery that panics, as a bug in a platform backend would.
    struct PanickingDiscovery;

    impl AppDiscovery for PanickingDiscovery {
        fn discover(&self) -> anyhow::Result<Vec<DiscoveredApp>> {
            panic!("discovery panics on purpose");
        }

        fn icon(&self, _app: &DiscoveredApp) -> anyhow::Result<Option<image::DynamicImage>> {
            Ok(None)
        }
    }

    /// Start a refresh that panics and wait for its thread to end.
    fn panicking_refresh() -> (AppLauncherGadget, i64, i64) {
        let dir = tempfile::tempdir().expect("temporary directory can be created");
        let gadget = AppLauncherGadget::new(caps_with_icons_in(dir.path()), PanickingDiscovery);

        let before = unix_now();
        let handle = gadget
            .maybe_trigger_background_refresh()
            .expect("the first search starts a refresh");
        assert!(handle.join().is_err(), "the refresh thread did not panic");

        (gadget, before, unix_now())
    }

    #[test]
    fn a_panicking_refresh_releases_the_refresh_flag() {
        let (gadget, _, _) = panicking_refresh();

        assert!(
            !gadget.refreshing.load(Ordering::SeqCst),
            "the refresh flag stays set after a panic"
        );
    }

    #[test]
    fn a_panicking_refresh_sets_the_retry_stamp() {
        let (gadget, before, after) = panicking_refresh();

        let stamp = gadget.retry_not_before.load(Ordering::SeqCst);
        assert!(
            (before + RETRY_INTERVAL_SECS..=after + RETRY_INTERVAL_SECS).contains(&stamp),
            "stamp {stamp} outside [{before}+30, {after}+30]"
        );
        assert!(
            gadget.maybe_trigger_background_refresh().is_none(),
            "a search right after a panic spawned a refresh"
        );
    }

    #[test]
    fn a_search_inside_the_retry_window_releases_the_refresh_flag() {
        let dir = tempfile::tempdir().expect("temporary directory can be created");
        let (discovery, _) = ScriptedDiscovery::new(vec![failure(), failure()]);
        let gadget = gadget_with(dir.path(), discovery);
        trigger_and_join(&gadget);

        let blocked = gadget.maybe_trigger_background_refresh();
        assert!(
            blocked.is_none(),
            "a search inside the window spawned a refresh"
        );
        gadget
            .retry_not_before
            .store(unix_now() - 1, Ordering::SeqCst);

        trigger_and_join(&gadget);
    }

    #[test]
    fn a_search_inside_the_success_interval_releases_the_refresh_flag() {
        let dir = tempfile::tempdir().expect("temporary directory can be created");
        let (discovery, _) = ScriptedDiscovery::new(vec![Ok(vec![app("One")]), failure()]);
        let gadget = gadget_with(dir.path(), discovery);
        gadget.enable().expect("discovery succeeds");

        let blocked = gadget.maybe_trigger_background_refresh();
        assert!(
            blocked.is_none(),
            "a search inside the interval spawned a refresh"
        );
        make_due(&gadget);

        trigger_and_join(&gadget);
    }

    #[test]
    fn a_failed_refresh_keeps_the_list_and_the_icon_files() {
        let dir = tempfile::tempdir().expect("temporary directory can be created");
        let (discovery, calls) =
            ScriptedDiscovery::new(vec![Ok(vec![app("One"), app("Two")]), failure()]);
        let gadget = gadget_with(dir.path(), discovery);
        gadget.enable().expect("discovery succeeds");
        let ids = cached_ids(&gadget);
        let icons = cached_icon_paths(&gadget);
        assert_eq!(ids.len(), 2);
        assert_eq!(icons.len(), 2);
        make_due(&gadget);

        trigger_and_join(&gadget);

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(cached_ids(&gadget), ids);
        assert_eq!(cached_icon_paths(&gadget), icons);
        assert_eq!(files_below(dir.path()).len(), 2);
        assert!(icons.iter().all(|path| Path::new(path).exists()));
    }

    #[test]
    fn a_failed_startup_discovery_keeps_the_icons_of_an_earlier_run() {
        let dir = tempfile::tempdir().expect("temporary directory can be created");
        let (earlier, _) = ScriptedDiscovery::new(vec![Ok(vec![app("One"), app("Two")])]);
        gadget_with(dir.path(), earlier)
            .enable()
            .expect("discovery succeeds");
        assert_eq!(files_below(dir.path()).len(), 2);

        let (later, _) = ScriptedDiscovery::new(vec![failure()]);
        let gadget = gadget_with(dir.path(), later);
        gadget
            .enable()
            .expect("a failed discovery is logged, not raised");

        assert!(cached_ids(&gadget).is_empty());
        assert_eq!(files_below(dir.path()).len(), 2);
    }

    #[test]
    fn the_next_successful_refresh_replaces_the_list_and_drops_old_icons() {
        let dir = tempfile::tempdir().expect("temporary directory can be created");
        let (discovery, _) = ScriptedDiscovery::new(vec![
            Ok(vec![app("One"), app("Two")]),
            failure(),
            Ok(vec![app("Three")]),
        ]);
        let gadget = gadget_with(dir.path(), discovery);
        gadget.enable().expect("discovery succeeds");
        make_due(&gadget);
        trigger_and_join(&gadget);
        make_due(&gadget);

        trigger_and_join(&gadget);

        assert_eq!(cached_ids(&gadget), ["/Applications/Three.app"]);
        assert_eq!(files_below(dir.path()).len(), 1);
    }

    #[test]
    fn a_successful_refresh_after_a_failure_waits_the_long_interval() {
        let dir = tempfile::tempdir().expect("temporary directory can be created");
        let (discovery, _) = ScriptedDiscovery::new(vec![failure(), Ok(vec![app("One")])]);
        let gadget = gadget_with(dir.path(), discovery);
        trigger_and_join(&gadget);
        make_due(&gadget);

        let before = unix_now();
        trigger_and_join(&gadget);
        let after = unix_now();

        let last = gadget.last_refresh.load(Ordering::SeqCst);
        assert!(
            (before..=after).contains(&last),
            "last_refresh {last} outside [{before}, {after}]"
        );
        assert_eq!(gadget.retry_not_before.load(Ordering::SeqCst), 0);
        assert!(gadget.maybe_trigger_background_refresh().is_none());
    }

    #[test]
    fn entries_lists_the_cached_apps_and_leaves_the_refresh_flag_clear() {
        let dir = tempfile::tempdir().expect("temporary directory can be created");
        let (discovery, _) = ScriptedDiscovery::new(vec![Ok(vec![app("One")])]);
        let gadget = gadget_with(dir.path(), discovery);
        gadget.enable().expect("discovery succeeds");

        let entries = gadget.entries();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "/Applications/One.app");
        assert!(!gadget.refreshing.load(Ordering::SeqCst));
    }

    // =========================================================
    // The due check
    // =========================================================

    #[test]
    fn a_refresh_is_due_after_the_success_interval() {
        assert!(!refresh_is_due(1299, 1000, 0));
        assert!(refresh_is_due(1300, 1000, 0));
    }

    #[test]
    fn a_refresh_waits_while_the_retry_stamp_lies_ahead() {
        let now = 10_000;
        assert!(refresh_is_due(now, 0, now - 1));
        assert!(refresh_is_due(now, 0, now));
        assert!(!refresh_is_due(now, 0, now + 1));
        assert!(!refresh_is_due(now, 0, now + RETRY_INTERVAL_SECS));
    }

    #[test]
    fn a_retry_stamp_further_ahead_than_the_retry_interval_is_ignored() {
        let now = 10_000;
        assert!(refresh_is_due(now, 0, now + RETRY_INTERVAL_SECS + 1));
        assert!(refresh_is_due(now, 0, now + 10_000));
    }
}
