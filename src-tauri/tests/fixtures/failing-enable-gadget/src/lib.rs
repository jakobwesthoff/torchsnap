// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Failing-Enable Test Plugin
//
// Variant of `minimal-plugin` whose guest `lifecycle::enable`
// panics. Used by the bridge test suite to drive the
// "guest enable() failed → bridge drops the instance" path.
//
// A Rust panic inside a guest export becomes a wasmtime trap
// on the host side, which surfaces as an `Err` from
// `WasmPluginInstance::enable()`. That is the signal the
// bridge uses to tear the instance back down to `None`.
//
// All other guest exports are no-ops — the fixture is only
// interesting during `enable()`, and the other methods exist
// only to satisfy the WIT world's `export` requirement.
// =========================================================

wit_bindgen::generate!({
    path: "../../../../plugins/plugin-sdk/wit",
    world: "gadget",
});

use exports::torchsnap::gadget::lifecycle::Guest as LifecycleGuest;
use exports::torchsnap::gadget::messaging::Guest as MessagingGuest;
use exports::torchsnap::gadget::search::{
    ActionId, CatalogEntry, Guest as SearchGuest, PostAction, SearchResponse,
};
use exports::torchsnap::gadget::tasks::Guest as TasksGuest;

struct FailingEnablePlugin;

export!(FailingEnablePlugin);

impl LifecycleGuest for FailingEnablePlugin {
    fn enable() {
        panic!("failing-enable-plugin: enable() intentionally panics");
    }

    fn disable() {}

    fn on_setting_changed(_key: String, _value: String) {}
}

impl SearchGuest for FailingEnablePlugin {
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

impl MessagingGuest for FailingEnablePlugin {
    fn handle_message(_method: String, _payload: String) -> Result<String, String> {
        Ok(String::new())
    }
}

impl TasksGuest for FailingEnablePlugin {
    fn run_task(_task_id: String) -> Result<(), String> {
        Ok(())
    }
}
