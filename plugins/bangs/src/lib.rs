// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Bangs Plugin — placeholder scaffold
//
// The guest port lands in the next commit. This skeleton
// exists so every commit in the chain builds: the scaffold
// step wires up the workspace membership, manifest,
// migrations, and frontend layout, but not yet the guest
// logic.
// =========================================================

use torchsnap_plugin_sdk::prelude::*;

struct BangsPlugin;
define_plugin!(BangsPlugin);

impl LifecycleGuest for BangsPlugin {
    fn enable() {}
    fn disable() {}
    fn on_setting_changed(_key: String, _value: String) {}
}

impl SearchGuest for BangsPlugin {
    fn entries() -> Vec<CatalogEntry> {
        Vec::new()
    }

    fn search(_query: String, _matched_prefix: Option<String>) -> SearchResponse {
        SearchResponse::Nothing
    }

    fn execute(_entry_id: String, _action_id: ActionId) -> Result<PostAction, String> {
        Ok(PostAction::Nothing)
    }
}

impl_noop_messaging!(BangsPlugin);
impl_noop_tasks!(BangsPlugin);
