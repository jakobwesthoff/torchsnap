// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Minimal Test Plugin
//
// Test-only WASM plugin used by the Rust test suite in
// `src-tauri/src/wasm/{runtime,bridge}.rs`. Implements every
// guest export of the shared WIT world with a no-op body so
// the bridge / runtime can instantiate it and exercise its
// lifecycle without running any meaningful guest logic.
//
// This file exists to produce `minimal_plugin.wasm` — the
// committed artifact is what the tests actually load via
// `include_bytes!` or a `DirectorySource` pointed at this
// directory. Rebuild via `just build-test-fixtures` whenever
// the shared WIT world changes.
// =========================================================

wit_bindgen::generate!({
    path: "../../../../wit",
    world: "plugin",
});

use exports::torchsnap::plugin::lifecycle::Guest as LifecycleGuest;
use exports::torchsnap::plugin::messaging::Guest as MessagingGuest;
use exports::torchsnap::plugin::search::{
    ActionId, CatalogEntry, Guest as SearchGuest, PostAction, SearchResponse,
};
use exports::torchsnap::plugin::tasks::Guest as TasksGuest;

struct MinimalPlugin;

export!(MinimalPlugin);

impl LifecycleGuest for MinimalPlugin {
    fn enable() {}

    fn disable() {}

    fn on_setting_changed(_key: String, _value: String) {}
}

impl SearchGuest for MinimalPlugin {
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

impl MessagingGuest for MinimalPlugin {
    fn handle_message(_method: String, _payload: String) -> Result<String, String> {
        Ok(String::new())
    }
}

impl TasksGuest for MinimalPlugin {
    fn run_task(_task_id: String) -> Result<(), String> {
        Ok(())
    }
}
