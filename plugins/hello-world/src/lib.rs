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
// - Query:   generates 50,000 petname entries on enable()
//            and fuzzy-matches them using nucleo-matcher
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
    ViewResponse,
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
        let span = logging::span_start("enable", None, &[]);
        let names = petnames::generate_petnames(50_000);
        logging::log(
            logging::LogLevel::Info,
            &format!("Hello World plugin enabled with {} petnames", names.len()),
            &[],
            Some(span),
        );
        PETNAMES.with(|cell| *cell.borrow_mut() = names);
        logging::span_end(
            span,
            &[("petname_count".into(), "50000".into())],
        );
    }

    fn disable() {
        PETNAMES.with(|cell| cell.borrow_mut().clear());
        logging::log(logging::LogLevel::Info, "Hello World plugin disabled", &[], None);
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

    fn search(query: String, matched_prefix: Option<String>) -> SearchResponse {
        if query.is_empty() {
            return SearchResponse::Nothing;
        }

        // When triggered via the "!" prefix, show a custom UI
        // that echoes the query. This demonstrates the full
        // frontend dynamic loading pipeline.
        if matched_prefix.is_some() {
            return SearchResponse::CustomUi(ViewResponse {
                view: "echo".into(),
                data: Some(format!(r#"{{"query":"{}"}}"#, query.replace('"', "\\\""))),
                results: vec![],
            });
        }

        // Without a prefix, fuzzy-search the petname corpus.
        let span = logging::span_start(
            "fuzzy-search",
            None,
            &[("query".into(), query.clone())],
        );

        let results = PETNAMES.with(|cell| {
            let names = cell.borrow();
            petnames::fuzzy_search(&query, &names)
        });

        let result_count = results.len();
        logging::span_end(
            span,
            &[("result_count".into(), result_count.to_string())],
        );

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
            &[("entry_id".into(), entry_id.clone())],
            None,
        );
        Ok(PostAction::Dismiss)
    }
}
