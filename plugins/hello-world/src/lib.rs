// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Hello World Plugin
//
// Validates the end-to-end WASM plugin pipeline with both
// catalog and query search modes:
//
// - Catalog: returns a single "Say Hello" entry
// - Query:   generates 5000 petname entries on enable() and
//            fuzzy-matches them using nucleo-matcher
// =========================================================

wit_bindgen::generate!({
    path: "../../wit",
    world: "plugin",
});

use std::cell::RefCell;

use exports::torchsnap::plugin::lifecycle::Guest as LifecycleGuest;
use exports::torchsnap::plugin::search::Guest as SearchGuest;
use torchsnap::plugin::logging;
use torchsnap::plugin::types::{
    Action, ActionId, CatalogEntry, EntryIcon, PostAction, SearchResponse,
};

mod petnames;

struct HelloWorld;

export!(HelloWorld);

// =========================================================
// Petname Storage
//
// WASM is single-threaded, so RefCell is safe. The entries
// are generated once in enable() and read in search().
// =========================================================

thread_local! {
    static PETNAMES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

impl LifecycleGuest for HelloWorld {
    fn enable() {
        let names = petnames::generate_petnames(50_000);
        logging::log(
            logging::LogLevel::Info,
            &format!("Hello World plugin enabled with {} petnames", names.len()),
        );
        PETNAMES.with(|cell| *cell.borrow_mut() = names);
    }

    fn disable() {
        PETNAMES.with(|cell| cell.borrow_mut().clear());
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
                id: ActionId::Open,
                label: "Run".into(),
            }],
        }]
    }

    fn search(query: String, _matched_prefix: Option<String>) -> SearchResponse {
        if query.is_empty() {
            return SearchResponse::Nothing;
        }

        let results = PETNAMES.with(|cell| {
            let names = cell.borrow();
            petnames::fuzzy_search(&query, &names)
        });

        if results.is_empty() {
            SearchResponse::Nothing
        } else {
            SearchResponse::Results(results)
        }
    }

    fn execute(entry_id: String, _action_id: ActionId) -> Result<PostAction, String> {
        logging::log(
            logging::LogLevel::Info,
            &format!("Executed entry: {entry_id}"),
        );
        Ok(PostAction::Dismiss)
    }
}
