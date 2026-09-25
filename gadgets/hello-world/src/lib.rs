// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Hello World Gadget
//
// Validates the end-to-end WASM gadget pipeline with both
// catalog and query search modes:
//
// - Catalog: returns a single "Say Hello" entry
// - Query:   generates 50,000 petname entries on enable()
//            and fuzzy-matches them using nucleo-matcher
// =========================================================

use std::cell::RefCell;

use serde::{Deserialize, Serialize};
use torchsnap_gadget_sdk::prelude::*;

mod petnames;

struct HelloWorld;
define_gadget!(HelloWorld);

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
    fn enable() -> Result<(), String> {
        let span = logging::span_start("enable", None, &[]);
        let names = petnames::generate_petnames(50_000);
        logging::log(
            logging::LogLevel::Info,
            &format!("Hello World gadget enabled with {} petnames", names.len()),
            &[],
            Some(span),
        );
        PETNAMES.with(|cell| *cell.borrow_mut() = names);
        logging::span_end(span, &[("petname_count".into(), "50000".into())]);
        Ok(())
    }

    fn disable() {
        PETNAMES.with(|cell| cell.borrow_mut().clear());
        logging::log(
            logging::LogLevel::Info,
            "Hello World gadget disabled",
            &[],
            None,
        );
    }

    /// Hello-world has no settings, so the host never invokes
    /// this with any meaningful key. The default no-op
    /// implementation satisfies the lifecycle interface.
    fn on_setting_changed(_key: String, _value: String) {}
}

/// What the gadget's actions do.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Command {
    /// The "Say Hello" catalog entry: log a greeting.
    Greet,
    /// Copy a petname to the clipboard.
    CopyName(String),
}

/// A petname's actions: copying is the main action, so it runs on
/// Enter as well as on Cmd+C.
fn petname_actions(name: &str) -> Actions<Command> {
    Actions::new()
        .primary("Copy", Command::CopyName(name.to_string()))
        .copy(Action::new(Command::CopyName(name.to_string())))
}

impl Search for HelloWorld {
    type Command = Command;

    fn entries() -> Vec<CatalogEntry<Command>> {
        vec![CatalogEntry {
            id: "greet".into(),
            title: "Say Hello".into(),
            subtitle: Some("Hello World WASM gadget".into()),
            icon: Some(EntryIcon::HeroIcon("hand-raised".into())),
            keywords: vec!["hello".into(), "greet".into(), "test".into()],
            actions: Actions::new().primary("Run", Command::Greet),
        }]
    }

    fn search(query: String, matched_prefix: Option<String>) -> SearchResponse<Command> {
        if query.is_empty() {
            return SearchResponse::Nothing;
        }

        // When triggered via the "!" prefix, show a custom UI
        // that echoes the query. This demonstrates the full
        // frontend dynamic loading pipeline.
        if matched_prefix.is_some() {
            let data = serde_json::json!({ "query": query });
            return SearchResponse::CustomUi(ViewResponse {
                view: "echo".into(),
                data: Some(data.to_string()),
                results: vec![],
            });
        }

        // Without a prefix, fuzzy-search the petname corpus.
        let span = logging::span_start("fuzzy-search", None, &[("query".into(), query.clone())]);

        let results = PETNAMES.with(|cell| {
            let names = cell.borrow();
            petnames::fuzzy_search(&query, &names)
        });

        let result_count = results.len();
        logging::span_end(span, &[("result_count".into(), result_count.to_string())]);

        if results.is_empty() {
            SearchResponse::Nothing
        } else {
            SearchResponse::Results(results)
        }
    }

    fn execute(command: Command) -> Result<PostAction, String> {
        match command {
            Command::Greet => {
                logging::log(
                    logging::LogLevel::Info,
                    "Hello from the WASM gadget",
                    &[],
                    None,
                );
            }
            Command::CopyName(name) => {
                clipboard::write_text(&name).map_err(|e| format!("copy to clipboard: {e}"))?;
            }
        }
        Ok(PostAction::Dismiss)
    }
}

// Hello-world has no frontend RPC calls and declares no
// scheduled tasks in its manifest; the SDK no-op macros
// satisfy the mandatory WIT exports without hand-rolled
// stubs. Any misrouted call surfaces as a clear error in
// host logs.
impl_noop_messaging!(HelloWorld);
impl_noop_tasks!(HelloWorld);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn petname_copies_on_enter_and_on_cmd_c() {
        let actions = petname_actions("brave-otter");
        assert_eq!(
            actions.primary,
            Some(Action::labeled(
                "Copy",
                Command::CopyName("brave-otter".into())
            ))
        );
        assert_eq!(
            actions.copy,
            Some(Action::new(Command::CopyName("brave-otter".into())))
        );
    }

    #[test]
    fn fuzzy_search_results_carry_the_petname_actions() {
        let names = vec!["brave-otter".to_string(), "calm-heron".to_string()];
        let results = petnames::fuzzy_search("otter", &names);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].actions, petname_actions("brave-otter"));
    }
}
