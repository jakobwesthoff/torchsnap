// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Typed search for gadgets.
//!
//! Every action on an entry carries a *command*: a value of the
//! gadget's own `Command` type that says what running the action
//! does. The WIT carries commands as opaque strings; this module
//! encodes them to JSON on the way out and decodes them on the way
//! back, so the gadget only ever sees its own type:
//!
//! ```ignore
//! #[derive(Serialize, Deserialize)]
//! enum Command {
//!     Open(String),
//!     CopyUrl(String),
//! }
//!
//! impl Search for Bangs {
//!     type Command = Command;
//!
//!     fn search(query: String, _prefix: Option<String>) -> SearchResponse<Command> {
//!         SearchResponse::Results(vec![ScoredEntry {
//!             actions: Actions::new()
//!                 .primary("Open in Browser", Command::Open(url.clone()))
//!                 .copy(Action::labeled("Copy URL", Command::CopyUrl(url))),
//!             ..
//!         }])
//!     }
//!
//!     fn execute(command: Command) -> Result<PostAction, String> {
//!         match command {
//!             Command::Open(url) => { /* ... */ }
//!             Command::CopyUrl(url) => { /* ... */ }
//!         }
//!     }
//! }
//! ```
//!
//! Each field of [`Actions`] is a slot, and the slot alone decides
//! how the user runs the action: `primary` is Enter and a click,
//! `secondary` is Cmd+Enter, `copy` is Cmd+C, `reveal` is
//! Cmd+Shift+R, `delete` is Cmd+Backspace, `open_settings` is Cmd+,.

use serde::{Serialize, de::DeserializeOwned};

use crate::exports::torchsnap::gadget::search as wit;
use crate::logging::{LogLevel, log};
use crate::torchsnap::gadget::types as wit_types;
use crate::{EntryIcon, PostAction, SearchGuest, data};

// =========================================================
// Actions
// =========================================================

/// One action on an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action<C> {
    /// What the launcher shows. `None` shows the slot's default
    /// label, which exists for every slot but `primary` and
    /// `secondary`.
    pub label: Option<String>,
    /// What running the action does.
    pub command: C,
}

impl<C> Action<C> {
    /// An action shown with its slot's default label.
    pub fn new(command: C) -> Self {
        Self {
            label: None,
            command,
        }
    }

    /// An action with its own label.
    pub fn labeled(label: impl Into<String>, command: C) -> Self {
        Self {
            label: Some(label.into()),
            command,
        }
    }
}

/// The slots of [`Actions`], in the order the launcher lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Primary,
    Secondary,
    Copy,
    Reveal,
    Delete,
    OpenSettings,
}

/// The actions of an entry, one optional field per slot.
///
/// Build it with the struct literal and `..Default::default()`,
/// where filling a slot twice does not compile, or with the builder
/// methods, where a second call replaces the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actions<C> {
    pub primary: Option<Action<C>>,
    pub secondary: Option<Action<C>>,
    pub copy: Option<Action<C>>,
    pub reveal: Option<Action<C>>,
    pub delete: Option<Action<C>>,
    pub open_settings: Option<Action<C>>,
}

// Written out instead of derived: the derive would require
// `C: Default`, and an empty slot set needs no command at all.
impl<C> Default for Actions<C> {
    fn default() -> Self {
        Self {
            primary: None,
            secondary: None,
            copy: None,
            reveal: None,
            delete: None,
            open_settings: None,
        }
    }
}

impl<C> Actions<C> {
    /// No actions.
    pub fn new() -> Self {
        Self::default()
    }

    /// The Enter action. The launcher has no default label for it.
    pub fn primary(mut self, label: impl Into<String>, command: C) -> Self {
        self.primary = Some(Action::labeled(label, command));
        self
    }

    /// The Cmd+Enter action. The launcher has no default label for it.
    pub fn secondary(mut self, label: impl Into<String>, command: C) -> Self {
        self.secondary = Some(Action::labeled(label, command));
        self
    }

    pub fn copy(mut self, action: Action<C>) -> Self {
        self.copy = Some(action);
        self
    }

    pub fn reveal(mut self, action: Action<C>) -> Self {
        self.reveal = Some(action);
        self
    }

    pub fn delete(mut self, action: Action<C>) -> Self {
        self.delete = Some(action);
        self
    }

    pub fn open_settings(mut self, action: Action<C>) -> Self {
        self.open_settings = Some(action);
        self
    }

    /// The filled slots, in [`Slot`] order.
    pub fn iter(&self) -> impl Iterator<Item = (Slot, &Action<C>)> {
        [
            (Slot::Primary, &self.primary),
            (Slot::Secondary, &self.secondary),
            (Slot::Copy, &self.copy),
            (Slot::Reveal, &self.reveal),
            (Slot::Delete, &self.delete),
            (Slot::OpenSettings, &self.open_settings),
        ]
        .into_iter()
        .filter_map(|(slot, action)| action.as_ref().map(|action| (slot, action)))
    }
}

// =========================================================
// Entries and responses
// =========================================================

/// An entry for the host's fuzzy matching, returned by
/// [`Search::entries`].
#[derive(Debug, Clone)]
pub struct CatalogEntry<C> {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<EntryIcon>,
    pub keywords: Vec<String>,
    pub actions: Actions<C>,
}

/// A scored result, returned by [`Search::search`]. The gadget
/// computes the score and the highlight positions.
#[derive(Debug, Clone)]
pub struct ScoredEntry<C> {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<EntryIcon>,
    pub score: u32,
    /// UTF-16 code unit offsets to highlight in the title.
    pub title_highlight_positions: Vec<u32>,
    /// UTF-16 code unit offsets to highlight in the subtitle.
    pub subtitle_highlight_positions: Vec<u32>,
    pub actions: Actions<C>,
}

/// A custom or inline view. `data` is a JSON-encoded string handed
/// to the frontend view component as-is.
#[derive(Debug, Clone)]
pub struct ViewResponse<C> {
    pub view: String,
    pub data: Option<String>,
    pub results: Vec<ScoredEntry<C>>,
}

/// What [`Search::search`] returns.
#[derive(Debug, Clone)]
pub enum SearchResponse<C> {
    /// Nothing to contribute for this query.
    Nothing,
    /// Scored results merged into the host's list.
    Results(Vec<ScoredEntry<C>>),
    /// Full custom UI, replacing the result list.
    CustomUi(ViewResponse<C>),
    /// Inline UI above the result list.
    InlineUi(ViewResponse<C>),
}

// =========================================================
// The trait
// =========================================================

/// Typed search for a gadget.
///
/// A blanket impl provides the generated [`SearchGuest`] for every
/// type that implements `Search`.
pub trait Search {
    /// What an action does. Encoded to JSON when entries leave the
    /// gadget and decoded when the user runs an action.
    type Command: Serialize + DeserializeOwned;

    /// Entries for the host's fuzzy matching. Called on every
    /// search without a prefix.
    fn entries() -> Vec<CatalogEntry<Self::Command>> {
        Vec::new()
    }

    /// Gadget-driven search, called on every query keystroke.
    fn search(query: String, matched_prefix: Option<String>) -> SearchResponse<Self::Command>;

    /// Run the command of the action the user triggered.
    fn execute(command: Self::Command) -> Result<PostAction, String>;
}

impl<T: Search> SearchGuest for T {
    fn entries() -> Vec<wit::CatalogEntry> {
        let (entries, errors) = encode_catalog_entries(T::entries());
        log_dropped_entries(&errors);
        entries
    }

    fn search(query: String, matched_prefix: Option<String>) -> wit::SearchResponse {
        let (response, errors) = encode_response(T::search(query, matched_prefix));
        log_dropped_entries(&errors);
        response
    }

    fn execute(command: String) -> Result<PostAction, String> {
        let command =
            data::decode::<T::Command>(&command).map_err(|e| format!("command decode: {e}"))?;
        T::execute(command)
    }
}

fn log_dropped_entries(errors: &[String]) {
    for error in errors {
        log(
            LogLevel::Warn,
            "dropped an entry whose command could not be encoded",
            &[("error".to_string(), error.clone())],
            None,
        );
    }
}

// =========================================================
// Encoding
//
// Kept free of host calls so it can be tested on the host
// target. An entry whose command cannot be encoded is dropped as
// a whole: keeping it without that action could leave it without
// its primary action. The errors go back to the caller for
// logging.
// =========================================================

fn encode_action<C: Serialize>(action: Action<C>) -> Result<wit_types::Action, String> {
    Ok(wit_types::Action {
        label: action.label,
        command: data::encode(&action.command)?,
    })
}

fn encode_slot<C: Serialize>(
    action: Option<Action<C>>,
) -> Result<Option<wit_types::Action>, String> {
    action.map(encode_action).transpose()
}

fn encode_actions<C: Serialize>(actions: Actions<C>) -> Result<wit::EntryActions, String> {
    Ok(wit::EntryActions {
        primary: encode_slot(actions.primary)?,
        secondary: encode_slot(actions.secondary)?,
        copy: encode_slot(actions.copy)?,
        reveal: encode_slot(actions.reveal)?,
        delete: encode_slot(actions.delete)?,
        open_settings: encode_slot(actions.open_settings)?,
    })
}

fn encode_scored_entry<C: Serialize>(entry: ScoredEntry<C>) -> Result<wit::ScoredEntry, String> {
    Ok(wit::ScoredEntry {
        actions: encode_actions(entry.actions)?,
        id: entry.id,
        title: entry.title,
        subtitle: entry.subtitle,
        icon: entry.icon,
        score: entry.score,
        title_highlight_positions: entry.title_highlight_positions,
        subtitle_highlight_positions: entry.subtitle_highlight_positions,
    })
}

fn encode_catalog_entry<C: Serialize>(entry: CatalogEntry<C>) -> Result<wit::CatalogEntry, String> {
    Ok(wit::CatalogEntry {
        actions: encode_actions(entry.actions)?,
        id: entry.id,
        title: entry.title,
        subtitle: entry.subtitle,
        icon: entry.icon,
        keywords: entry.keywords,
    })
}

/// Encode every entry, dropping the ones that fail.
fn encode_all<E, W>(
    entries: Vec<E>,
    encode: impl Fn(E) -> Result<W, String>,
) -> (Vec<W>, Vec<String>) {
    let mut encoded = Vec::with_capacity(entries.len());
    let mut errors = Vec::new();
    for entry in entries {
        match encode(entry) {
            Ok(entry) => encoded.push(entry),
            Err(error) => errors.push(error),
        }
    }
    (encoded, errors)
}

fn encode_catalog_entries<C: Serialize>(
    entries: Vec<CatalogEntry<C>>,
) -> (Vec<wit::CatalogEntry>, Vec<String>) {
    encode_all(entries, encode_catalog_entry)
}

fn encode_view<C: Serialize>(view: ViewResponse<C>) -> (wit::ViewResponse, Vec<String>) {
    let (results, errors) = encode_all(view.results, encode_scored_entry);
    (
        wit::ViewResponse {
            view: view.view,
            data: view.data,
            results,
        },
        errors,
    )
}

fn encode_response<C: Serialize>(
    response: SearchResponse<C>,
) -> (wit::SearchResponse, Vec<String>) {
    match response {
        SearchResponse::Nothing => (wit::SearchResponse::Nothing, Vec::new()),
        SearchResponse::Results(results) => {
            let (results, errors) = encode_all(results, encode_scored_entry);
            (wit::SearchResponse::Results(results), errors)
        }
        SearchResponse::CustomUi(view) => {
            let (view, errors) = encode_view(view);
            (wit::SearchResponse::CustomUi(view), errors)
        }
        SearchResponse::InlineUi(view) => {
            let (view, errors) = encode_view(view);
            (wit::SearchResponse::InlineUi(view), errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serializer};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    enum Command {
        Open(String),
        Copy(String),
    }

    /// A command whose encoding always fails, like a map with
    /// non-string keys would.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    struct Unencodable;

    impl Serialize for Unencodable {
        fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("cannot encode"))
        }
    }

    fn entry<C>(id: &str, actions: Actions<C>) -> ScoredEntry<C> {
        ScoredEntry {
            id: id.to_string(),
            title: format!("Title {id}"),
            subtitle: None,
            icon: None,
            score: 7,
            title_highlight_positions: vec![0],
            subtitle_highlight_positions: vec![],
            actions,
        }
    }

    // ---- Actions ------------------------------------------

    #[test]
    fn builder_fills_the_named_slots() {
        let actions = Actions::new()
            .primary("Join", Command::Open("a".into()))
            .copy(Action::new(Command::Copy("a".into())));

        assert_eq!(
            actions.primary,
            Some(Action::labeled("Join", Command::Open("a".into())))
        );
        assert_eq!(actions.copy, Some(Action::new(Command::Copy("a".into()))));
        assert_eq!(actions.secondary, None);
        assert_eq!(actions.reveal, None);
        assert_eq!(actions.delete, None);
        assert_eq!(actions.open_settings, None);
    }

    #[test]
    fn builder_replaces_a_slot_filled_twice() {
        let actions = Actions::new()
            .copy(Action::new(Command::Copy("first".into())))
            .copy(Action::new(Command::Copy("second".into())));

        assert_eq!(
            actions.copy,
            Some(Action::new(Command::Copy("second".into())))
        );
    }

    #[test]
    fn iter_lists_filled_slots_in_slot_order() {
        let actions = Actions::new()
            .open_settings(Action::new(Command::Open("s".into())))
            .delete(Action::labeled("Forget", Command::Open("d".into())))
            .reveal(Action::new(Command::Open("r".into())))
            .secondary("Leave", Command::Open("2".into()))
            .primary("Join", Command::Open("1".into()));

        let slots: Vec<Slot> = actions.iter().map(|(slot, _)| slot).collect();
        assert_eq!(
            slots,
            [
                Slot::Primary,
                Slot::Secondary,
                Slot::Reveal,
                Slot::Delete,
                Slot::OpenSettings
            ]
        );
    }

    #[test]
    fn iter_is_empty_without_actions() {
        assert_eq!(Actions::<Command>::new().iter().count(), 0);
    }

    // ---- Encoding -----------------------------------------

    #[test]
    fn commands_are_encoded_as_json_and_decode_back() {
        let encoded = encode_scored_entry(entry(
            "a",
            Actions::new()
                .primary("Open", Command::Open("https://example.com".into()))
                .copy(Action::new(Command::Copy("https://example.com".into()))),
        ))
        .expect("encodes");

        let primary = encoded.actions.primary.expect("primary kept");
        assert_eq!(primary.label.as_deref(), Some("Open"));
        assert_eq!(
            data::decode::<Command>(&primary.command),
            Ok(Command::Open("https://example.com".into()))
        );
        let copy = encoded.actions.copy.expect("copy kept");
        assert_eq!(copy.label, None);
        assert!(encoded.actions.secondary.is_none());
    }

    #[test]
    fn entry_fields_survive_encoding() {
        let encoded = encode_scored_entry(entry("a", Actions::<Command>::new())).expect("encodes");
        assert_eq!(encoded.id, "a");
        assert_eq!(encoded.title, "Title a");
        assert_eq!(encoded.score, 7);
        assert_eq!(encoded.title_highlight_positions, vec![0]);
    }

    #[test]
    fn catalog_entry_keeps_its_keywords() {
        let (entries, errors) = encode_catalog_entries(vec![CatalogEntry {
            id: "c".into(),
            title: "Quit".into(),
            subtitle: None,
            icon: None,
            keywords: vec!["exit".into()],
            actions: Actions::new().primary("Quit", Command::Open("quit".into())),
        }]);
        assert!(errors.is_empty());
        assert_eq!(entries[0].keywords, vec!["exit".to_string()]);
        assert!(entries[0].actions.primary.is_some());
    }

    #[test]
    fn entry_with_an_unencodable_command_is_dropped_and_reported() {
        let (response, errors) = encode_response(SearchResponse::Results(vec![
            entry("bad", Actions::new().copy(Action::new(Unencodable))),
            entry("good", Actions::<Unencodable>::new()),
        ]));

        let wit::SearchResponse::Results(results) = response else {
            panic!("results stay results");
        };
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "good");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("cannot encode"), "{}", errors[0]);
    }

    #[test]
    fn view_responses_keep_view_and_data_and_encode_their_results() {
        let view = ViewResponse {
            view: "history".into(),
            data: Some("{}".into()),
            results: vec![entry(
                "h",
                Actions::new().copy(Action::new(Command::Copy("2".into()))),
            )],
        };
        for (response, is_custom) in [
            (SearchResponse::CustomUi(view.clone()), true),
            (SearchResponse::InlineUi(view), false),
        ] {
            let (encoded, errors) = encode_response(response);
            assert!(errors.is_empty());
            let view = match (encoded, is_custom) {
                (wit::SearchResponse::CustomUi(view), true) => view,
                (wit::SearchResponse::InlineUi(view), false) => view,
                _ => panic!("the variant is kept"),
            };
            assert_eq!(view.view, "history");
            assert_eq!(view.data.as_deref(), Some("{}"));
            assert!(view.results[0].actions.copy.is_some());
        }
    }

    #[test]
    fn nothing_stays_nothing() {
        let (response, errors) = encode_response(SearchResponse::<Command>::Nothing);
        assert!(matches!(response, wit::SearchResponse::Nothing));
        assert!(errors.is_empty());
    }

    // ---- Execute ------------------------------------------

    struct TestGadget;

    impl Search for TestGadget {
        type Command = Command;

        fn search(_: String, _: Option<String>) -> SearchResponse<Command> {
            SearchResponse::Nothing
        }

        fn execute(command: Command) -> Result<PostAction, String> {
            match command {
                Command::Open(_) => Ok(PostAction::Dismiss),
                Command::Copy(text) => Err(format!("refused {text}")),
            }
        }
    }

    #[test]
    fn execute_hands_the_decoded_command_to_the_gadget() {
        let command = data::encode(&Command::Open("x".into())).expect("encodes");
        assert_eq!(
            <TestGadget as SearchGuest>::execute(command),
            Ok(PostAction::Dismiss)
        );
    }

    #[test]
    fn execute_passes_the_gadget_error_through() {
        let command = data::encode(&Command::Copy("y".into())).expect("encodes");
        assert_eq!(
            <TestGadget as SearchGuest>::execute(command),
            Err("refused y".to_string())
        );
    }

    #[test]
    fn undecodable_command_is_rejected_before_gadget_code_runs() {
        let error = <TestGadget as SearchGuest>::execute(r#"{"Unknown":1}"#.to_string())
            .expect_err("not a Command");
        assert!(error.starts_with("command decode:"), "{error}");
    }

    #[test]
    fn entries_default_to_none() {
        assert!(<TestGadget as Search>::entries().is_empty());
    }
}
