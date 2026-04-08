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
use torchsnap::plugin::{logging, settings};

struct TemplatePlugin;

export!(TemplatePlugin);

impl LifecycleGuest for TemplatePlugin {
    fn enable() {
        // Read the current values of every plugin setting at
        // startup so the log shows what the user (or the
        // manifest defaults) configured. Real plugins would
        // typically cache these in `static` `AtomicBool`/`OnceLock`
        // values, but for the template the round-trip is the
        // point — we want plugin authors to see how
        // `settings::get` returns JSON-encoded strings that
        // they parse with `serde_json`.
        let greeting = read_string_setting("greeting").unwrap_or_else(|| "(unset)".into());
        let verbose = read_bool_setting("verbose").unwrap_or(false);

        logging::log(
            logging::LogLevel::Info,
            "Template plugin enabled",
            &[
                ("greeting".into(), greeting),
                ("verbose".into(), verbose.to_string()),
            ],
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

    fn on_setting_changed(key: String, value: String) {
        // The host's CoalescingDispatcher (ADR 0026) has
        // already deduplicated rapid same-key writes by the
        // time we get here, so we can react to every fire
        // without worrying about flapping. Real plugins would
        // typically push the parsed value into a cache —
        // here we just log the change so plugin authors can
        // see the round-trip in their dev tools.
        logging::log(
            logging::LogLevel::Info,
            &format!("Setting `{key}` changed"),
            &[("key".into(), key), ("value".into(), value)],
            None,
        );
    }
}

// =========================================================
// Settings helpers
//
// `settings::get` returns the raw JSON encoding of the
// stored value (`true`, `42`, `"hello"`, `{"a":1}`, …) as
// `Option<String>`. Each plugin parses it into whatever
// shape it expects via `serde_json::from_str`. The two
// helpers below cover the common bool / string cases for
// the template; real plugins extend the pattern with their
// own typed wrappers.
// =========================================================

fn read_bool_setting(key: &str) -> Option<bool> {
    serde_json::from_str(&settings::get(key)?).ok()
}

fn read_string_setting(key: &str) -> Option<String> {
    serde_json::from_str(&settings::get(key)?).ok()
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
