// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// System Preferences Gadget
//
// Catalog gadget that exposes system settings panes as
// searchable entries. Settings panes are discovered at startup
// via the platform's `SettingsDiscovery` implementation.
//
// Icons are rendered via `SettingsDiscovery::render_icon` and
// cached on disk as WebP. Opening a pane delegates to
// `SettingsDiscovery::open`. All platform-specific behavior is
// encapsulated in the discovery trait so this gadget stays
// fully platform-agnostic.
//
// No background refresh is needed — the set of settings panes
// only changes on OS updates.
//
// Actions:
//   - Open (primary): open the settings pane
// =========================================================

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use anyhow::Context;

use crate::commands::types::{Action, ActionId, CatalogEntry, EntryIcon, PostAction, ScoredEntry};
use crate::icons::IconCache;
use crate::platform::settings_discovery::{SettingsDiscovery, SettingsPane};
use crate::storage::StorageKey;

use super::{Gadget, ProvisioningContext};

pub struct SystemPreferencesCaps {
    pub icon_cache: Arc<IconCache>,
}

pub struct SystemPreferencesGadget {
    cache: Arc<RwLock<Vec<SettingsPane>>>,
    discovery: Arc<dyn SettingsDiscovery>,
}

impl SystemPreferencesGadget {
    pub fn new(discovery: impl SettingsDiscovery + 'static) -> Self {
        Self {
            cache: Arc::new(RwLock::new(Vec::new())),
            discovery: Arc::new(discovery),
        }
    }
}

/// Render and cache icons for each settings pane, populating
/// their `icon_path` field. Returns the set of valid cache keys
/// for orphan cleanup.
fn cache_pane_icons(
    icon_cache: &IconCache,
    discovery: &dyn SettingsDiscovery,
    panes: &mut [SettingsPane],
) -> HashSet<StorageKey> {
    let mut valid_keys = HashSet::with_capacity(panes.len());

    for pane in panes.iter_mut() {
        let key = StorageKey::new(&pane.id);

        if let Some(path) = icon_cache.ensure_icon(
            "system-preferences",
            &key,
            None, // System icons don't change between OS updates.
            || discovery.icon(pane),
        ) {
            pane.icon_path = Some(path);
        }
        valid_keys.insert(key);
    }

    valid_keys
}

impl Gadget for SystemPreferencesGadget {
    type Caps = SystemPreferencesCaps;

    fn id(&self) -> &str {
        "system-preferences"
    }

    fn provision(&self, ctx: &ProvisioningContext) -> anyhow::Result<SystemPreferencesCaps> {
        Ok(SystemPreferencesCaps {
            icon_cache: Arc::clone(&ctx.icon_cache),
        })
    }

    fn enable(&self, caps: SystemPreferencesCaps) {
        match self.discovery.discover() {
            Ok(mut panes) => {
                // Publish the pane list right away with fallback icons.
                {
                    let mut guard = self.cache.write().expect("settings cache not poisoned");
                    *guard = panes.clone();
                }

                // Render and cache SF Symbol icons for each pane.
                let valid_keys =
                    cache_pane_icons(&caps.icon_cache, &*self.discovery, &mut panes);
                caps.icon_cache.cleanup("system-preferences", &valid_keys);

                // Swap in icon-enriched entries.
                let mut guard = self.cache.write().expect("settings cache not poisoned");
                *guard = panes;
            }
            Err(e) => {
                eprintln!("settings pane discovery failed: {e:#}");
            }
        }
    }

    fn entries(&self) -> Vec<CatalogEntry> {
        let cache = self.cache.read().expect("settings cache not poisoned");

        cache
            .iter()
            .map(|pane| CatalogEntry {
                id: pane.id.clone(),
                title: pane.name.clone(),
                subtitle: Some("System Settings".into()),
                icon: Some(
                    pane.icon_path
                        .as_ref()
                        .map(|p| EntryIcon::AssetIcon(p.clone()))
                        .unwrap_or_else(|| EntryIcon::HeroIcon("cog-6-tooth".into())),
                ),
                keywords: vec!["settings".into(), "preferences".into(), "system".into()],
                actions: vec![Action {
                    id: ActionId::Open,
                    label: "Open".into(),
                    keybinding: None,
                }],
            })
            .collect()
    }

    fn execute(
        &self,
        entry: &ScoredEntry,
        action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        match action_id {
            ActionId::Open => {
                self.discovery
                    .open(&entry.id, app)
                    .context("open settings pane")?;
            }
            other => {
                anyhow::bail!(
                    "unsupported action {other:?} for system-preferences entry {}",
                    entry.id
                );
            }
        }

        Ok(PostAction::Dismiss)
    }
}
