// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// System Preferences Plugin
//
// Catalog plugin that exposes system settings panes as
// searchable entries. Settings panes are discovered at startup
// via the platform's `SettingsDiscovery` implementation.
//
// On macOS, icons are rendered from SF Symbols via the shared
// icon cache. Opening a pane uses the platform-specific URL
// scheme (x-apple.systempreferences: on macOS).
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
use tauri_plugin_opener::OpenerExt;

use crate::icons::{IconCache, IconCacheKey};
use crate::platform::settings_discovery::{SettingsDiscovery, SettingsPane};
use crate::search::types::{Action, ActionId, CatalogEntry, EntryIcon, PostAction};

use super::CatalogPlugin;

pub struct SystemPreferencesPlugin {
    cache: Arc<RwLock<Vec<SettingsPane>>>,
    discovery: Arc<dyn SettingsDiscovery>,
    icon_cache: Arc<IconCache>,
}

impl SystemPreferencesPlugin {
    pub fn new(
        discovery: impl SettingsDiscovery + 'static,
        icon_cache: Arc<IconCache>,
    ) -> Self {
        Self {
            cache: Arc::new(RwLock::new(Vec::new())),
            discovery: Arc::new(discovery),
            icon_cache,
        }
    }
}

impl CatalogPlugin for SystemPreferencesPlugin {
    fn id(&self) -> &str {
        "system-preferences"
    }

    fn setup(&self) {
        match self.discovery.discover() {
            Ok(mut panes) => {
                // Publish the pane list right away with fallback icons.
                {
                    let mut guard = self.cache.write().expect("settings cache not poisoned");
                    *guard = panes.clone();
                }

                // Render and cache SF Symbol icons for each pane.
                let valid_keys = cache_pane_icons(&self.icon_cache, &mut panes);
                self.icon_cache.cleanup("system-preferences", &valid_keys);

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
                keywords: vec![
                    "settings".into(),
                    "preferences".into(),
                    "system".into(),
                ],
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
        entry_id: &str,
        action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        match action_id {
            ActionId::Open => {
                open_settings_pane(entry_id, app)?;
            }
            other => {
                anyhow::bail!(
                    "unsupported action {other:?} for system-preferences entry {entry_id}"
                );
            }
        }

        Ok(PostAction::Dismiss)
    }
}

// =========================================================
// Icon Caching
// =========================================================

/// Render and cache icons for each settings pane, populating
/// their `icon_path` field. Returns the set of valid cache keys
/// for orphan cleanup.
fn cache_pane_icons(
    icon_cache: &IconCache,
    panes: &mut [SettingsPane],
) -> HashSet<IconCacheKey> {
    let mut valid_keys = HashSet::with_capacity(panes.len());

    for pane in panes.iter_mut() {
        let key = IconCacheKey::new(&pane.id);

        let icon_source = pane.icon_source.clone();
        if let Some(path) = icon_cache.ensure_icon(
            "system-preferences",
            &key,
            None, // System icons don't change between OS updates.
            || render_pane_icon(icon_source.as_deref()),
        ) {
            pane.icon_path = Some(path);
        }
        valid_keys.insert(key);
    }

    valid_keys
}

/// Render the icon for a settings pane from its platform-specific
/// icon source. On macOS this is an SF Symbol name.
fn render_pane_icon(
    icon_source: Option<&str>,
) -> anyhow::Result<Option<image::DynamicImage>> {
    let Some(symbol_name) = icon_source else {
        return Ok(None);
    };

    #[cfg(target_os = "macos")]
    {
        crate::platform::macos::sf_symbols::render_sf_symbol(symbol_name)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = symbol_name;
        Ok(None)
    }
}

// =========================================================
// Platform-specific Open Logic
// =========================================================

/// Open a system settings pane by its platform-specific ID.
#[cfg(target_os = "macos")]
fn open_settings_pane(pane_id: &str, app: &tauri::AppHandle) -> anyhow::Result<()> {
    let url = format!("x-apple.systempreferences:{pane_id}");
    app.opener()
        .open_url(&url, None::<&str>)
        .context("open system preferences pane")
}

#[cfg(not(target_os = "macos"))]
fn open_settings_pane(pane_id: &str, _app: &tauri::AppHandle) -> anyhow::Result<()> {
    // TODO: Windows — ShellExecute with ms-settings: URI
    // TODO: Linux — xdg-open or dbus
    anyhow::bail!("opening settings pane {pane_id} is not supported on this platform")
}
