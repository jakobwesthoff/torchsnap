// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Template Plugin
//
// Starting point for new Torchsnap WASM plugins. Demonstrates:
//
// - Catalog entries via `entries()`
// - Custom UI via prefix-triggered `search()`
// - Action execution via `execute()`
// - Structured logging with spans
//
// Copy this directory, rename the plugin ID in manifest.toml
// and Cargo.toml, and build from here.
// =========================================================

wit_bindgen::generate!({
    path: "../../wit",
    world: "plugin",
});

use exports::torchsnap::plugin::lifecycle::Guest as LifecycleGuest;
use exports::torchsnap::plugin::search::{
    Action, ActionId, CatalogEntry, EntryIcon, Guest as SearchGuest, PostAction,
    SearchResponse, ViewResponse,
};
use torchsnap::plugin::logging;

struct TemplatePlugin;

export!(TemplatePlugin);

impl LifecycleGuest for TemplatePlugin {
    fn enable() {
        logging::log(
            logging::LogLevel::Info,
            "Template plugin enabled",
            &[],
            None,
        );
    }

    fn disable() {
        logging::log(
            logging::LogLevel::Info,
            "Template plugin disabled",
            &[],
            None,
        );
    }
}

impl SearchGuest for TemplatePlugin {
    fn entries() -> Vec<CatalogEntry> {
        vec![CatalogEntry {
            id: "template-demo".into(),
            title: "Template Demo".into(),
            subtitle: Some("Template WASM plugin demo".into()),
            icon: Some(EntryIcon::HeroIcon("puzzle-piece".into())),
            keywords: vec!["template".into(), "demo".into()],
            actions: vec![Action {
                id: ActionId::Open,
                label: "Open".into(),
            }],
        }]
    }

    fn search(query: String, matched_prefix: Option<String>) -> SearchResponse {
        if query.is_empty() {
            return SearchResponse::Nothing;
        }

        // When triggered via the "tpl:" prefix, show a custom
        // frontend view. This demonstrates the full dynamic
        // loading pipeline: WASM → host → torchsnap-plugin://
        // protocol → dynamic import() → React component.
        if matched_prefix.is_some() {
            let data = serde_json::json!({ "query": query });
            return SearchResponse::CustomUi(ViewResponse {
                view: "demo".into(),
                data: Some(data.to_string()),
                results: vec![],
            });
        }

        SearchResponse::Nothing
    }

    fn execute(entry_id: String, _action_id: ActionId) -> Result<PostAction, String> {
        logging::log(
            logging::LogLevel::Info,
            &format!("Executed entry: {entry_id}"),
            &[("entry_id".into(), entry_id)],
            None,
        );
        Ok(PostAction::Dismiss)
    }
}
