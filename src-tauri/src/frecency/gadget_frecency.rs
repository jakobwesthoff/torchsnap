// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// GadgetFrecency — gadget-scoped frecency wrapper
//
// Same pattern as GadgetSettings: binds the gadget_id at
// construction so gadgets cannot access other gadgets'
// frecency data.
// =========================================================

use std::collections::HashMap;
use std::sync::Arc;

use super::{FrecencyItem, FrecencyStore, FrecencyTarget};

/// Gadget-scoped view of the [`FrecencyStore`].
///
/// Binds `gadget_id` at construction — gadgets cannot access
/// other gadgets' frecency data. Obtained from `GadgetContext`
/// during `enable()`.
#[derive(Clone)]
pub struct GadgetFrecency {
    store: Arc<FrecencyStore>,
    gadget_id: String,
}

impl GadgetFrecency {
    pub fn new(store: Arc<FrecencyStore>, gadget_id: &str) -> Self {
        Self {
            store,
            gadget_id: gadget_id.to_string(),
        }
    }

    /// Record a selection event for the given item.
    ///
    /// **Important:** The host already calls `FrecencyStore::record()`
    /// inside `GadgetHost::execute()`. Only call this directly for
    /// custom UI interactions that bypass `execute()` — never for
    /// the same selection the host already handles, or the event
    /// will be double-counted.
    // `record`, `score`, `scores`, and `apply_scores` are the delegate
    // surface for native gadgets that bypass `execute()`; `top_items`
    // and `is_enabled` below are the members WASM guests reach through
    // `wasm::runtime::host::frecency`. No in-repo native gadget calls
    // these four directly — the host already records and scores
    // through `FrecencyStore` inside `GadgetHost::execute()`.
    #[allow(dead_code)]
    pub fn record(&self, item_id: &str) {
        self.store.record(&self.gadget_id, item_id);
    }

    /// Compute the frecency score for a single item.
    #[allow(dead_code)]
    pub fn score(&self, item_id: &str) -> u32 {
        self.store.score(&self.gadget_id, item_id)
    }

    /// Batch-compute frecency scores for multiple items.
    #[allow(dead_code)]
    pub fn scores(&self, item_ids: &[&str]) -> HashMap<String, u32> {
        self.store.scores(&self.gadget_id, item_ids)
    }

    /// Apply frecency score bonuses to a mutable slice of results.
    #[allow(dead_code)]
    pub fn apply_scores(&self, results: &mut [impl FrecencyTarget]) {
        self.store.apply_scores(&self.gadget_id, results);
    }

    /// Return the top-N most frequently/recently used items.
    pub fn top_items(&self, limit: usize) -> Vec<FrecencyItem> {
        self.store.top_items(&self.gadget_id, limit)
    }

    /// Whether frecency tracking is currently enabled.
    pub fn is_enabled(&self) -> bool {
        self.store.is_enabled()
    }
}
