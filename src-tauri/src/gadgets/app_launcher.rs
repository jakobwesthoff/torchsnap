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
// blocking the search path.
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

pub struct AppLauncherGadget {
    caps: Arc<ProvisionedCaps>,
    cache: Arc<RwLock<Vec<DiscoveredApp>>>,
    last_refresh: Arc<AtomicI64>,
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
        let icon_cache = Arc::clone(self.caps.icon_cache());

        thread::spawn(move || {
            match discovery.discover() {
                Ok(mut apps) => {
                    let valid_keys = extract_icons(&icon_cache, &*discovery, &mut apps);
                    icon_cache.cleanup("app-launcher", &valid_keys);

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
}
