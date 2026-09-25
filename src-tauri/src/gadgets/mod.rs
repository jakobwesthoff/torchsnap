// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Gadget System
//
// Search covers both modes (ADR 0012):
//
// - Catalog gadgets override `entries()` to provide a finite
//   entry list that the host filters with nucleo.
// - Query gadgets override `search()` (and optionally
//   `Gadget::search_prefixes()`) to receive the raw query and
//   return pre-scored results.
// - Hybrid gadgets override both — the host calls both paths
//   unconditionally.
//
// Search lives in two traits (ADR 55). `ErasedSearch` is what
// the host calls: every action's command is an opaque string.
// `Search` is what native gadgets implement, with their own
// command type; a blanket impl provides `ErasedSearch` by
// encoding commands to JSON. The WASM bridge implements
// `ErasedSearch` directly and passes the component's command
// strings through. `Gadget` has `ErasedSearch` as a supertrait,
// so the host keeps holding object-safe `Arc<dyn Gadget>`.
//
// Gadgets receive `Arc<ProvisionedCaps>` at construction
// via the factory-based registration API.
// =========================================================

pub mod app_launcher;
pub mod clipboard;
pub mod commands;
pub mod system_commands;
pub mod system_preferences;

use serde::{Serialize, de::DeserializeOwned};

use crate::commands::types::{CatalogEntry, GadgetResponse, PostAction, ScoredEntry};
use crate::settings::SettingsInit;

// =========================================================
// GadgetShortcut — global shortcut declaration
// =========================================================

/// A global keyboard shortcut that a gadget wants to register.
pub struct GadgetShortcut {
    pub id: &'static str,
    /// Human-readable shortcut name. Settings shows it when another
    /// shortcut collides with this one.
    pub label: &'static str,
    pub default_shortcut: &'static str,
    pub settings_key: &'static str,
}

// =========================================================
// Gadget Trait
// =========================================================

/// Unified gadget trait for both catalog and query gadgets.
///
/// ## Capability provisioning
///
/// Gadgets receive `Arc<ProvisionedCaps>` at construction via
/// the factory-based registration API. Capabilities are plain
/// fields on the gadget struct — no `OnceLock`, no `Option`.
///
/// ## Lifecycle
///
/// 1. Host calls `cap_requests()` (inherent method) and builds caps
/// 2. Host calls the factory closure with caps to construct the gadget
/// 3. Host calls `register_with_caps()` to register the gadget
/// 4. `initialize_settings()` is called synchronously at startup
/// 5. `enable()` is called on a background thread if `enabled.<id>`
///    is `true` in the settings store
/// 6. `entries()` / `search()` (from [`ErasedSearch`]) are called
///    on every search keystroke
/// 7. `execute()` is called with the triggered action's command
/// 8. `setting_changed()` is called whenever a key in
///    `gadgets.<id>.*` changes at runtime
/// 9. `disable()` is called when the user toggles the gadget off
///    or during `RunEvent::Exit`
pub trait Gadget: ErasedSearch {
    fn id(&self) -> &str;

    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit {
        settings
    }

    /// Activate the gadget. Runs on a `spawn_blocking` thread.
    /// Caps are available as plain fields on `self`. Returning
    /// `Err` disables the gadget — no further calls dispatched.
    fn enable(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn disable(&self) {}

    fn setting_changed(&self, _key: &str, _value: serde_json::Value) {}

    fn shortcuts(&self) -> Vec<GadgetShortcut> {
        vec![]
    }

    fn handle_shortcut(&self, _shortcut_id: &str) -> anyhow::Result<PostAction> {
        Ok(PostAction::Nothing)
    }

    fn handle_message(
        &self,
        _method: &str,
        _payload: serde_json::Value,
        _channel: tauri::ipc::Channel<serde_json::Value>,
    ) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("gadget does not handle custom messages")
    }

    fn search_prefixes(&self) -> &[String] {
        &[]
    }
}

// =========================================================
// Search Traits
// =========================================================

/// Search as the host calls it. Commands are opaque strings.
pub trait ErasedSearch: Send + Sync {
    fn entries(&self) -> Vec<CatalogEntry> {
        vec![]
    }

    fn search(&self, _query: &str, _matched_prefix: Option<&str>) -> Option<GadgetResponse> {
        None
    }

    /// Run the command of the action the user triggered.
    fn execute(&self, command: &str) -> anyhow::Result<PostAction>;
}

/// Search for native gadgets, typed by the gadget's command.
pub trait Search: Send + Sync {
    /// What an action does. Encoded to JSON when entries reach the
    /// host and decoded when the user runs an action.
    type Command: Serialize + DeserializeOwned;

    fn entries(&self) -> Vec<CatalogEntry<Self::Command>> {
        vec![]
    }

    fn search(
        &self,
        _query: &str,
        _matched_prefix: Option<&str>,
    ) -> Option<GadgetResponse<Self::Command>> {
        None
    }

    fn execute(&self, command: Self::Command) -> anyhow::Result<PostAction>;
}

impl<T: Search> ErasedSearch for T {
    fn entries(&self) -> Vec<CatalogEntry> {
        encode_all(Search::entries(self), |entry| {
            entry.try_map_commands(encode_command)
        })
    }

    fn search(&self, query: &str, matched_prefix: Option<&str>) -> Option<GadgetResponse> {
        Search::search(self, query, matched_prefix).map(encode_response)
    }

    fn execute(&self, command: &str) -> anyhow::Result<PostAction> {
        let command = serde_json::from_str::<T::Command>(command)
            .map_err(|e| anyhow::anyhow!("command decode: {e}"))?;
        Search::execute(self, command)
    }
}

fn encode_command<C: Serialize>(command: C) -> Result<String, serde_json::Error> {
    serde_json::to_string(&command)
}

/// Encode every entry, dropping and logging the ones whose command
/// does not encode. Dropping the whole entry instead of the one
/// action keeps an entry from losing its primary action.
fn encode_all<E, W>(entries: Vec<E>, encode: impl Fn(E) -> Result<W, serde_json::Error>) -> Vec<W> {
    entries
        .into_iter()
        .filter_map(|entry| match encode(entry) {
            Ok(entry) => Some(entry),
            Err(e) => {
                eprintln!("dropped an entry whose command could not be encoded: {e}");
                None
            }
        })
        .collect()
}

fn encode_scored<C: Serialize>(entries: Vec<ScoredEntry<C>>) -> Vec<ScoredEntry> {
    encode_all(entries, |entry| entry.try_map_commands(encode_command))
}

fn encode_response<C: Serialize>(response: GadgetResponse<C>) -> GadgetResponse {
    match response {
        GadgetResponse::Results(results) => GadgetResponse::Results(encode_scored(results)),
        GadgetResponse::CustomUI {
            view,
            data,
            results,
        } => GadgetResponse::CustomUI {
            view,
            data,
            results: encode_scored(results),
        },
        GadgetResponse::InlineUI {
            view,
            data,
            results,
        } => GadgetResponse::InlineUI {
            view,
            data,
            results: encode_scored(results),
        },
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;
    use crate::commands::types::{Action, EntryActions};
    use crate::unicode::Utf16Positions;
    use serde::{Deserialize, Serializer};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    enum Command {
        Open(String),
        Fail,
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

    fn entry<C>(id: &str, actions: EntryActions<C>) -> ScoredEntry<C> {
        ScoredEntry {
            id: id.to_string(),
            title: id.to_string(),
            subtitle: None,
            icon: None,
            score: 1,
            title_positions: Utf16Positions::empty(),
            subtitle_positions: Utf16Positions::empty(),
            actions,
        }
    }

    struct TypedGadget;

    impl Search for TypedGadget {
        type Command = Command;

        fn entries(&self) -> Vec<CatalogEntry<Command>> {
            vec![CatalogEntry {
                id: "quit".into(),
                title: "Quit".into(),
                subtitle: None,
                icon: None,
                keywords: vec!["exit".into()],
                actions: EntryActions::new().primary("Quit", Command::Open("quit".into())),
            }]
        }

        fn search(&self, query: &str, _: Option<&str>) -> Option<GadgetResponse<Command>> {
            Some(GadgetResponse::InlineUI {
                view: "v".into(),
                data: None,
                results: vec![entry(
                    query,
                    EntryActions {
                        copy: Some(Action {
                            label: None,
                            command: Command::Open(query.into()),
                        }),
                        ..EntryActions::new()
                    },
                )],
            })
        }

        fn execute(&self, command: Command) -> anyhow::Result<PostAction> {
            match command {
                Command::Open(_) => Ok(PostAction::Dismiss),
                Command::Fail => anyhow::bail!("refused"),
            }
        }
    }

    struct BrokenGadget;

    impl Search for BrokenGadget {
        type Command = Unencodable;

        fn search(&self, _: &str, _: Option<&str>) -> Option<GadgetResponse<Unencodable>> {
            Some(GadgetResponse::Results(vec![
                entry(
                    "bad",
                    EntryActions {
                        copy: Some(Action {
                            label: None,
                            command: Unencodable,
                        }),
                        ..EntryActions::new()
                    },
                ),
                entry("good", EntryActions::new()),
            ]))
        }

        fn execute(&self, _: Unencodable) -> anyhow::Result<PostAction> {
            Ok(PostAction::Nothing)
        }
    }

    #[test]
    fn catalog_commands_are_encoded_as_json() {
        let entries = ErasedSearch::entries(&TypedGadget);
        let primary = entries[0].actions.primary.as_ref().expect("primary kept");
        assert_eq!(primary.label.as_deref(), Some("Quit"));
        assert_eq!(
            serde_json::from_str::<Command>(&primary.command).expect("decodes"),
            Command::Open("quit".into())
        );
        assert_eq!(entries[0].keywords, vec!["exit".to_string()]);
    }

    #[test]
    fn view_responses_keep_their_variant_and_encode_their_results() {
        let Some(GadgetResponse::InlineUI { view, results, .. }) =
            ErasedSearch::search(&TypedGadget, "abc", None)
        else {
            panic!("inline response stays inline");
        };
        assert_eq!(view, "v");
        let copy = results[0].actions.copy.as_ref().expect("copy kept");
        assert_eq!(
            serde_json::from_str::<Command>(&copy.command).expect("decodes"),
            Command::Open("abc".into())
        );
    }

    #[test]
    fn entry_with_an_unencodable_command_is_dropped() {
        let Some(GadgetResponse::Results(results)) = ErasedSearch::search(&BrokenGadget, "", None)
        else {
            panic!("results stay results");
        };
        let ids: Vec<&str> = results.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["good"]);
    }

    #[test]
    fn execute_decodes_the_command_for_the_gadget() {
        let command = serde_json::to_string(&Command::Open("x".into())).expect("encodes");
        let post_action = ErasedSearch::execute(&TypedGadget, &command).expect("runs");
        assert!(matches!(post_action, PostAction::Dismiss));
    }

    #[test]
    fn execute_passes_the_gadget_error_through() {
        let command = serde_json::to_string(&Command::Fail).expect("encodes");
        let error = ErasedSearch::execute(&TypedGadget, &command).expect_err("refused");
        assert_eq!(error.to_string(), "refused");
    }

    #[test]
    fn undecodable_command_is_rejected_before_gadget_code_runs() {
        let error =
            ErasedSearch::execute(&TypedGadget, "{\"Other\":1}").expect_err("not a Command");
        assert!(error.to_string().starts_with("command decode:"), "{error}");
    }
}
