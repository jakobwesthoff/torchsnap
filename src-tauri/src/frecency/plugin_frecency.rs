// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// PluginFrecency — plugin-scoped frecency wrapper
//
// Same pattern as PluginSettings / PluginSettingsNotifier:
// binds the plugin_id at construction so plugins cannot
// access other plugins' frecency data.
// =========================================================

use std::collections::HashMap;
use std::sync::Arc;

use super::{FrecencyItem, FrecencyStore, FrecencyTarget};

/// Plugin-scoped view of the [`FrecencyStore`].
///
/// Binds `plugin_id` at construction — plugins cannot access
/// other plugins' frecency data. Obtained from `PluginContext`
/// during `enable()`.
#[derive(Clone)]
pub struct PluginFrecency {
    store: Arc<FrecencyStore>,
    plugin_id: String,
}

impl PluginFrecency {
    pub fn new(store: Arc<FrecencyStore>, plugin_id: &str) -> Self {
        Self {
            store,
            plugin_id: plugin_id.to_string(),
        }
    }

    /// Record a selection event for the given item.
    ///
    /// **Important:** The host already calls `FrecencyStore::record()`
    /// inside `PluginHost::execute()`. Only call this directly for
    /// custom UI interactions that bypass `execute()` — never for
    /// the same selection the host already handles, or the event
    /// will be double-counted.
    pub fn record(&self, item_id: &str) {
        self.store.record(&self.plugin_id, item_id);
    }

    /// Compute the frecency score for a single item.
    pub fn score(&self, item_id: &str) -> u32 {
        self.store.score(&self.plugin_id, item_id)
    }

    /// Batch-compute frecency scores for multiple items.
    pub fn scores(&self, item_ids: &[&str]) -> HashMap<String, u32> {
        self.store.scores(&self.plugin_id, item_ids)
    }

    /// Apply frecency score bonuses to a mutable slice of results.
    pub fn apply_scores(&self, results: &mut [impl FrecencyTarget]) {
        self.store.apply_scores(&self.plugin_id, results);
    }

    /// Return the top-N most frequently/recently used items.
    pub fn top_items(&self, limit: usize) -> Vec<FrecencyItem> {
        self.store.top_items(&self.plugin_id, limit)
    }

    /// Whether frecency tracking is currently enabled.
    pub fn is_enabled(&self) -> bool {
        self.store.is_enabled()
    }
}
