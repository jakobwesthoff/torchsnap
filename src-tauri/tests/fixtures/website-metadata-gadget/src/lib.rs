// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Website-metadata Test Gadget
//
// Test-only WASM gadget used by the Rust integration suite in
// `src-tauri/src/wasm/runtime.rs`. Exercises the
// `website-metadata::lookup` host import via
// `messaging::handle-message` dispatch so tests get a clean
// `Result<String, String>` oracle to assert against.
//
// Rebuild via `just build-test-fixtures` whenever the shared
// WIT world changes.
// =========================================================

wit_bindgen::generate!({
    path: "../../../../gadgets/gadget-sdk/wit",
    world: "gadget",
});

use exports::torchsnap::gadget::lifecycle::Guest as LifecycleGuest;
use exports::torchsnap::gadget::messaging::Guest as MessagingGuest;
use exports::torchsnap::gadget::search::{
    ActionId, CatalogEntry, Guest as SearchGuest, PostAction, ScoredEntry, SearchResponse,
};
use exports::torchsnap::gadget::tasks::Guest as TasksGuest;
use torchsnap::gadget::types::EntryIcon;
use torchsnap::gadget::website_metadata::{lookup, LookupMode, LookupResult};

struct WebsiteMetadataPlugin;

export!(WebsiteMetadataPlugin);

impl LifecycleGuest for WebsiteMetadataPlugin {
    fn enable() -> Result<(), String> { Ok(()) }
    fn disable() {}
    fn on_setting_changed(_key: String, _value: String) {}
}

impl SearchGuest for WebsiteMetadataPlugin {
    fn entries() -> Vec<CatalogEntry> {
        Vec::new()
    }

    fn search(_query: String, _matched_prefix: Option<String>) -> SearchResponse {
        SearchResponse::Nothing
    }

    fn execute(_entry: ScoredEntry, _action_id: ActionId) -> Result<PostAction, String> {
        Ok(PostAction::Nothing)
    }
}

impl MessagingGuest for WebsiteMetadataPlugin {
    /// Dispatch table for test commands:
    ///
    /// - `"lookup-blocking"` — blocking-mode lookup of `payload`.
    ///   Returns a compact stringified form of the result so the
    ///   host test can assert against it without parsing JSON:
    ///     - `"hit:<title>:<description>:<favicon-tag>"`
    ///     - `"reachable-no-data"`
    ///     - `"unreachable"`
    ///     - `"pending"` (only ever produced by `lookup-cached`)
    ///   Errors propagate the WIT error variant.
    ///
    /// - `"lookup-cached"` — cached-mode lookup. Same response
    ///   shape; on cold miss the gadget sees `pending`.
    fn handle_message(method: String, payload: String) -> Result<String, String> {
        let mode = match method.as_str() {
            "lookup-blocking" => LookupMode::Blocking,
            "lookup-cached" => LookupMode::Cached,
            other => return Err(format!("unknown method: {other}")),
        };

        let result = lookup(&payload, mode).map_err(|e| format!("{e:?}"))?;
        Ok(format_result(result))
    }
}

impl TasksGuest for WebsiteMetadataPlugin {
    fn run_task(_task_id: String) -> Result<(), String> {
        Ok(())
    }
}

/// Compact, parseable serialisation of `LookupResult` for tests.
///
/// `hit` carries title/description/favicon tag separated by colons.
/// Empty strings stand in for `None`. The favicon tag is the variant
/// kind only (e.g. `asset-icon`) since the asset path is a host
/// temp-dir path not worth asserting against in integration tests.
fn format_result(r: LookupResult) -> String {
    match r {
        LookupResult::Hit(entry) => {
            let title = entry.title.as_deref().unwrap_or("");
            let desc = entry.description.as_deref().unwrap_or("");
            let icon = match entry.favicon {
                EntryIcon::HeroIcon(_) => "hero-icon",
                EntryIcon::DataUrl(_) => "data-url",
                EntryIcon::AssetIcon(_) => "asset-icon",
                EntryIcon::Emoji(_) => "emoji",
                EntryIcon::AppIcon(_) => "app-icon",
            };
            format!("hit:{title}:{desc}:{icon}")
        }
        LookupResult::ReachableNoData => "reachable-no-data".to_string(),
        LookupResult::Unreachable => "unreachable".to_string(),
        LookupResult::Pending => "pending".to_string(),
    }
}
