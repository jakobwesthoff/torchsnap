// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Application Launcher Plugin
//
// Catalog plugin that exposes installed applications as
// searchable entries. Applications are discovered at startup
// via the platform's `AppDiscovery` implementation and cached
// in memory. A background refresh runs every 5 minutes to
// pick up newly installed or removed applications without
// blocking the search path.
//
// Actions:
//   - Open (primary): launch the application
//   - Reveal in Finder (secondary): show in file manager
// =========================================================

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use tauri_plugin_opener::OpenerExt;

use crate::platform::app_discovery::{AppDiscovery, DiscoveredApp};
use crate::search::types::{Action, ActionId, ActionKeybinding, CatalogEntry, EntryIcon};

use super::CatalogPlugin;

/// How long before the cached app list is considered stale and
/// a background refresh is triggered.
const REFRESH_INTERVAL_SECS: i64 = 300; // 5 minutes

pub struct AppLauncherPlugin {
    cache: Arc<RwLock<Vec<DiscoveredApp>>>,
    last_refresh: Arc<AtomicI64>,
    refreshing: Arc<AtomicBool>,
    discovery: Arc<dyn AppDiscovery>,
}

impl AppLauncherPlugin {
    pub fn new(discovery: impl AppDiscovery + 'static) -> Self {
        Self {
            cache: Arc::new(RwLock::new(Vec::new())),
            last_refresh: Arc::new(AtomicI64::new(0)),
            refreshing: Arc::new(AtomicBool::new(false)),
            discovery: Arc::new(discovery),
        }
    }

    /// If the cache is older than `REFRESH_INTERVAL_SECS`, spawn a
    /// background thread to re-discover applications. The current
    /// `entries()` call returns the existing (stale) cache immediately
    /// — the refresh result will be available on subsequent calls.
    fn maybe_trigger_background_refresh(&self) {
        let now = unix_now();
        let last = self.last_refresh.load(Ordering::Relaxed);

        if now - last < REFRESH_INTERVAL_SECS {
            return;
        }

        // Atomic compare-and-swap prevents multiple concurrent
        // refresh threads if entries() is called rapidly.
        if self
            .refreshing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        let cache = Arc::clone(&self.cache);
        let timestamp = Arc::clone(&self.last_refresh);
        let refreshing = Arc::clone(&self.refreshing);
        let discovery = Arc::clone(&self.discovery);

        thread::spawn(move || {
            match discovery.discover() {
                Ok(apps) => {
                    let mut guard = cache.write().expect("app cache not poisoned");
                    *guard = apps;
                    timestamp.store(unix_now(), Ordering::Relaxed);
                }
                Err(e) => {
                    // Keep serving the old cache rather than clearing it.
                    eprintln!("background app discovery failed: {e:#}");
                }
            }
            refreshing.store(false, Ordering::SeqCst);
        });
    }
}

impl CatalogPlugin for AppLauncherPlugin {
    fn id(&self) -> &str {
        "app-launcher"
    }

    fn setup(&self) {
        // Called on a dedicated background thread by the registry.
        // We can block here — the launcher opens immediately with
        // an empty result set and populates once discovery finishes.
        match self.discovery.discover() {
            Ok(apps) => {
                let mut guard = self.cache.write().expect("app cache not poisoned");
                *guard = apps;
                self.last_refresh.store(unix_now(), Ordering::Relaxed);
            }
            Err(e) => {
                eprintln!("initial app discovery failed: {e:#}");
            }
        }
    }

    fn entries(&self) -> Vec<CatalogEntry> {
        self.maybe_trigger_background_refresh();

        let cache = self.cache.read().expect("app cache not poisoned");

        cache
            .iter()
            .map(|app| CatalogEntry {
                id: app.id.clone(),
                title: app.name.clone(),
                subtitle: Some(app.path.to_string_lossy().into_owned()),
                icon: Some(EntryIcon::HeroIcon("rocket-launch".into())),
                keywords: vec![],
                actions: vec![
                    Action {
                        id: ActionId::Open,
                        label: "Open".into(),
                        keybinding: None,
                    },
                    Action {
                        id: ActionId::Reveal,
                        label: "Reveal in Finder".into(),
                        keybinding: Some(ActionKeybinding {
                            modifiers: vec!["Meta".into()],
                            key: "Enter".into(),
                        }),
                    },
                ],
            })
            .collect()
    }

    fn execute(
        &self,
        entry_id: &str,
        action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<()> {
        match action_id {
            ActionId::Open => {
                app.opener()
                    .open_path(entry_id, None::<&str>)
                    .context("open application")?;
            }
            ActionId::Reveal => {
                app.opener()
                    .reveal_item_in_dir(entry_id)
                    .context("reveal application in Finder")?;
            }
            other => {
                anyhow::bail!("unsupported action {other:?} for app-launcher entry {entry_id}");
            }
        }

        Ok(())
    }
}

/// Current unix timestamp in seconds.
fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_secs() as i64
}
