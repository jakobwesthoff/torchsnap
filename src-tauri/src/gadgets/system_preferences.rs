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
// Icons are rendered via `SettingsDiscovery::icon` and cached
// on disk as WebP. Opening a pane uses `OpenerCap::open_url`
// with a platform-specific deep-link URL. Discovery behavior
// is encapsulated in the discovery trait so this gadget stays
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

use crate::caps::{CapRequest, OpenerPermissions, ProvisionedCaps};
use crate::commands::types::{CatalogEntry, EntryActions, EntryIcon, PostAction};
use crate::icons::IconCache;
use crate::platform::settings_discovery::{SettingsDiscovery, SettingsPane};
use crate::storage::StorageKey;

use super::{Gadget, Search};

pub struct SystemPreferencesGadget {
    caps: Arc<ProvisionedCaps>,
    cache: Arc<RwLock<Vec<SettingsPane>>>,
    discovery: Arc<dyn SettingsDiscovery>,
}

impl SystemPreferencesGadget {
    pub fn cap_requests() -> Vec<CapRequest> {
        vec![
            CapRequest::IconCache,
            CapRequest::Opener {
                permissions: OpenerPermissions {
                    schemes: vec!["*".into()],
                    open_path: false,
                    reveal_path: false,
                },
            },
        ]
    }

    pub fn new(caps: Arc<ProvisionedCaps>, discovery: impl SettingsDiscovery + 'static) -> Self {
        Self {
            caps,
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
    fn id(&self) -> &str {
        "system-preferences"
    }

    fn enable(&self) -> anyhow::Result<()> {
        let icon_cache = self.caps.icon_cache();

        match self.discovery.discover() {
            Ok(mut panes) => {
                // Publish the pane list right away with fallback icons.
                {
                    let mut guard = self.cache.write().expect("settings cache not poisoned");
                    *guard = panes.clone();
                }

                // Render and cache SF Symbol icons for each pane.
                let valid_keys = cache_pane_icons(icon_cache, &*self.discovery, &mut panes);
                icon_cache.cleanup("system-preferences", &valid_keys);

                // Swap in icon-enriched entries.
                let mut guard = self.cache.write().expect("settings cache not poisoned");
                *guard = panes;
            }
            Err(e) => {
                eprintln!("settings pane discovery failed: {e:#}");
            }
        }
        Ok(())
    }
}

/// What a pane entry does: open the pane with the given id.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Command {
    OpenPane(String),
}

/// Enter opens the pane.
fn pane_actions(pane_id: &str) -> EntryActions<Command> {
    EntryActions::new().primary("Open", Command::OpenPane(pane_id.to_string()))
}

impl Search for SystemPreferencesGadget {
    type Command = Command;

    fn entries(&self) -> Vec<CatalogEntry<Command>> {
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
                actions: pane_actions(&pane.id),
            })
            .collect()
    }

    fn execute(&self, command: Command) -> anyhow::Result<PostAction> {
        let opener = self.caps.opener();
        let Command::OpenPane(pane_id) = command;

        #[cfg(target_os = "macos")]
        {
            let url = crate::platform::macos::MacosSettingsDiscovery::pane_url(&pane_id);
            opener
                .open_url(&url)
                .map_err(|e| anyhow::anyhow!(e))
                .context("open settings pane")?;
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (opener, pane_id);
            anyhow::bail!("system-preferences open not implemented on this platform");
        }

        Ok(PostAction::Dismiss)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::types::Action;

    #[test]
    fn pane_opens_on_enter() {
        let actions = pane_actions("com.apple.Wi-Fi-Settings.extension");
        assert_eq!(
            actions.primary,
            Some(Action::labeled(
                "Open",
                Command::OpenPane("com.apple.Wi-Fi-Settings.extension".into())
            ))
        );
        assert_eq!(actions.iter().count(), 1);
    }
}
