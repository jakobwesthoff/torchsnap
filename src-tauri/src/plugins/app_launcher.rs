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
// Icons are extracted via the platform's `IconExtractor` and
// cached on disk as PNGs. The `IconCache` handles mtime-based
// invalidation and orphan cleanup.
//
// Actions:
//   - Open (primary): launch the application
//   - Reveal in Finder (secondary): show in file manager
// =========================================================

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use tauri_plugin_opener::OpenerExt;

use crate::platform::app_discovery::{AppDiscovery, DiscoveredApp};
use crate::icons::IconCache;
use crate::search::types::{Action, ActionId, ActionKeybinding, CatalogEntry, EntryIcon, PostAction};

use super::CatalogPlugin;

/// How long before the cached app list is considered stale and
/// a background refresh is triggered.
const REFRESH_INTERVAL_SECS: i64 = 300; // 5 minutes

pub struct AppLauncherPlugin {
    cache: Arc<RwLock<Vec<DiscoveredApp>>>,
    last_refresh: Arc<AtomicI64>,
    refreshing: Arc<AtomicBool>,
    discovery: Arc<dyn AppDiscovery>,
    icon_cache: Arc<IconCache>,
}

impl AppLauncherPlugin {
    pub fn new(discovery: impl AppDiscovery + 'static, icon_cache: IconCache) -> Self {
        Self {
            cache: Arc::new(RwLock::new(Vec::new())),
            last_refresh: Arc::new(AtomicI64::new(0)),
            refreshing: Arc::new(AtomicBool::new(false)),
            discovery: Arc::new(discovery),
            icon_cache: Arc::new(icon_cache),
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
        let icon_cache = Arc::clone(&self.icon_cache);

        thread::spawn(move || {
            match discovery.discover() {
                Ok(mut apps) => {
                    let valid_keys = extract_icons(&icon_cache, &mut apps);
                    icon_cache.cleanup(&valid_keys);

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

/// Run icon extraction for each discovered app, populating their
/// `icon_path` field and returning the set of valid cache keys
/// for orphan cleanup.
fn extract_icons(icon_cache: &IconCache, apps: &mut [DiscoveredApp]) -> HashSet<String> {
    let mut valid_keys = HashSet::with_capacity(apps.len());

    for app in apps.iter_mut() {
        let key = IconCache::cache_key(app);
        if let Some(path) = icon_cache.ensure_icon(app) {
            app.icon_path = Some(path);
        }
        valid_keys.insert(key);
    }

    valid_keys
}

impl CatalogPlugin for AppLauncherPlugin {
    fn id(&self) -> &str {
        "app-launcher"
    }

    fn setup(&self) {
        // Called on a dedicated background thread by the registry.
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
                let valid_keys = extract_icons(&self.icon_cache, &mut apps);
                self.icon_cache.cleanup(&valid_keys);

                let mut guard = self.cache.write().expect("app cache not poisoned");
                *guard = apps;
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
                icon: Some(
                    app.icon_path
                        .as_ref()
                        .map(|p| EntryIcon::AssetIcon(p.clone()))
                        .unwrap_or_else(|| EntryIcon::HeroIcon("rocket-launch".into())),
                ),
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
    ) -> anyhow::Result<PostAction> {
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

        Ok(PostAction::Dismiss)
    }
}

/// Current unix timestamp in seconds.
fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_secs() as i64
}
