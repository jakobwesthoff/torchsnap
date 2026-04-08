// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Template Plugin
//
// Starting point for new Torchsnap WASM plugins. Showcases
// the full plugin authoring surface so you can copy this
// directory, rename the plugin ID in `manifest.toml` and
// `Cargo.toml`, and build from here.
//
// What this file demonstrates:
//
// - Catalog entries via `entries()`
// - Custom UI via prefix-triggered `search()`
// - Action execution via `execute()`
// - Structured logging with metadata
// - Reading plugin settings via the `settings::get` host
//   import and parsing the JSON-encoded values
// - Reacting to user setting changes via the
//   `on_setting_changed` lifecycle export
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
        // Read each setting once at startup so internal state
        // matches whatever the user configured (or whatever
        // `manifest.toml`'s `[settings]` defaults set). For
        // settings read frequently from a hot path, cache the
        // parsed value in a `static` `AtomicBool` / `OnceLock`
        // and refresh it from `on_setting_changed` below; this
        // example just logs the values to keep the wiring
        // visible.
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
        // The host coalesces rapid same-key writes (e.g. a
        // slider being dragged) before this fires, so it is
        // safe to react to every invocation — there is no
        // need to debounce on the plugin side. Push the
        // parsed value into your in-memory cache here so
        // subsequent reads see the latest state. This example
        // just logs the change.
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
// `settings::get` returns the value as a JSON-encoded
// string (`"true"`, `"42"`, `"\"hello\""`, `"{\"a\":1}"`,
// …) wrapped in `Option`. Parse it into whatever shape your
// plugin uses via `serde_json::from_str`. The two helpers
// below cover the common bool and string cases — extend the
// pattern with your own typed accessors as needed.
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
