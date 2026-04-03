// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Hello World Plugin
//
// Minimal WASM plugin that validates the end-to-end plugin
// pipeline: WIT bindings, wasmtime loading, manifest parsing,
// and catalog search integration.
// =========================================================

wit_bindgen::generate!({
    path: "../../wit",
    world: "plugin",
});

use exports::torchsnap::plugin::lifecycle::Guest as LifecycleGuest;
use exports::torchsnap::plugin::search::Guest as SearchGuest;
use torchsnap::plugin::logging;
use torchsnap::plugin::types::{Action, CatalogEntry, EntryIcon, PostAction};

struct HelloWorld;

export!(HelloWorld);

impl LifecycleGuest for HelloWorld {
    fn enable() {
        logging::log(logging::LogLevel::Info, "Hello World plugin enabled");
    }

    fn disable() {
        logging::log(logging::LogLevel::Info, "Hello World plugin disabled");
    }
}

impl SearchGuest for HelloWorld {
    fn entries() -> Vec<CatalogEntry> {
        vec![CatalogEntry {
            id: "greet".into(),
            title: "Say Hello".into(),
            subtitle: Some("Hello World WASM plugin".into()),
            icon: Some(EntryIcon::HeroIcon("hand-raised".into())),
            keywords: vec!["hello".into(), "greet".into(), "test".into()],
            actions: vec![Action {
                id: "open".into(),
                label: "Run".into(),
            }],
        }]
    }

    fn execute(entry_id: String, _action_id: String) -> Result<PostAction, String> {
        logging::log(
            logging::LogLevel::Info,
            &format!("Executed entry: {entry_id}"),
        );
        Ok(PostAction::Dismiss)
    }
}
