// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WASM Component Bindings
//
// Generates typed Rust bindings from the WIT definitions via
// `wasmtime::component::bindgen!`. This produces:
//
// - A `Plugin` struct with methods to call guest exports
//   (e.g., `call_enable`, `call_entries`, `call_execute`)
// - Traits for each host import interface that we implement
//   on the store state (e.g., `logging::Host`)
// - Typed records/enums mirroring the WIT definitions
//
// This module also provides `From` conversions between the
// generated types and the native `search::types` used by
// the rest of the application.
// =========================================================

wasmtime::component::bindgen!({
    path: "../wit",
    world: "plugin",
});

// =========================================================
// Type Conversions: WIT types → native types
// =========================================================

use crate::search::types as native;
use torchsnap::plugin::types as wit;

impl From<wit::EntryIcon> for native::EntryIcon {
    fn from(icon: wit::EntryIcon) -> Self {
        match icon {
            wit::EntryIcon::HeroIcon(name) => native::EntryIcon::HeroIcon(name),
            wit::EntryIcon::DataUrl(data) => native::EntryIcon::DataUrl(data),
            wit::EntryIcon::AssetIcon(path) => native::EntryIcon::AssetIcon(path),
            wit::EntryIcon::Emoji(emoji) => native::EntryIcon::Emoji(emoji),
        }
    }
}

impl From<wit::ActionId> for native::ActionId {
    fn from(id: wit::ActionId) -> Self {
        match id {
            wit::ActionId::Open => native::ActionId::Open,
            wit::ActionId::Copy => native::ActionId::Copy,
            wit::ActionId::Reveal => native::ActionId::Reveal,
            wit::ActionId::OpenWith => native::ActionId::OpenWith,
            wit::ActionId::Delete => native::ActionId::Delete,
            wit::ActionId::Custom(s) => native::ActionId::Custom(s),
        }
    }
}

impl From<native::ActionId> for wit::ActionId {
    fn from(id: native::ActionId) -> Self {
        match id {
            native::ActionId::Open => wit::ActionId::Open,
            native::ActionId::Copy => wit::ActionId::Copy,
            native::ActionId::Reveal => wit::ActionId::Reveal,
            native::ActionId::OpenWith => wit::ActionId::OpenWith,
            native::ActionId::Delete => wit::ActionId::Delete,
            native::ActionId::Custom(s) => wit::ActionId::Custom(s),
        }
    }
}

impl From<wit::Action> for native::Action {
    fn from(action: wit::Action) -> Self {
        native::Action {
            id: action.id.into(),
            label: action.label,
            // Keybindings are host-managed — plugins don't set them.
            keybinding: None,
        }
    }
}

impl From<wit::CatalogEntry> for native::CatalogEntry {
    fn from(entry: wit::CatalogEntry) -> Self {
        native::CatalogEntry {
            id: entry.id,
            title: entry.title,
            subtitle: entry.subtitle,
            icon: entry.icon.map(Into::into),
            keywords: entry.keywords,
            actions: entry.actions.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<wit::PostAction> for native::PostAction {
    fn from(action: wit::PostAction) -> Self {
        match action {
            wit::PostAction::Nothing => native::PostAction::Nothing,
            wit::PostAction::Dismiss => native::PostAction::Dismiss,
            wit::PostAction::KeepOpen => native::PostAction::KeepOpen,
        }
    }
}

impl From<wit::ScoredEntry> for native::ScoredEntry {
    fn from(entry: wit::ScoredEntry) -> Self {
        native::ScoredEntry {
            id: entry.id,
            title: entry.title,
            subtitle: entry.subtitle,
            icon: entry.icon.map(Into::into),
            score: entry.score,
            title_positions: crate::unicode::Utf16Positions(entry.title_highlight_positions),
            subtitle_positions: crate::unicode::Utf16Positions(entry.subtitle_highlight_positions),
            actions: entry.actions.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<wit::SearchResponse> for native::PluginResponse {
    fn from(response: wit::SearchResponse) -> Self {
        match response {
            wit::SearchResponse::Nothing => native::PluginResponse::Results(vec![]),
            wit::SearchResponse::Results(entries) => {
                native::PluginResponse::Results(entries.into_iter().map(Into::into).collect())
            }
            wit::SearchResponse::CustomUi(vr) => native::PluginResponse::CustomUI {
                view: vr.view,
                data: parse_optional_json(vr.data),
                results: vr.results.into_iter().map(Into::into).collect(),
            },
            wit::SearchResponse::InlineUi(vr) => native::PluginResponse::InlineUI {
                view: vr.view,
                data: parse_optional_json(vr.data),
                results: vr.results.into_iter().map(Into::into).collect(),
            },
        }
    }
}

/// Parse an optional JSON-encoded string into a `serde_json::Value`.
/// Returns `None` if the input is `None` or if the JSON is malformed
/// (graceful degradation — a plugin sending bad JSON shouldn't crash
/// the host).
fn parse_optional_json(json: Option<String>) -> Option<serde_json::Value> {
    json.and_then(|s| serde_json::from_str(&s).ok())
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a minimal WIT scored entry for tests.
    fn wit_scored_entry(id: &str, score: u32) -> wit::ScoredEntry {
        wit::ScoredEntry {
            id: id.to_string(),
            title: id.to_string(),
            subtitle: None,
            icon: None,
            score,
            title_highlight_positions: vec![],
            subtitle_highlight_positions: vec![],
            actions: vec![],
        }
    }

    #[test]
    fn nothing_maps_to_empty_results() {
        let response: native::PluginResponse = wit::SearchResponse::Nothing.into();
        match response {
            native::PluginResponse::Results(entries) => assert!(entries.is_empty()),
            other => panic!("expected Results, got {other:?}"),
        }
    }

    #[test]
    fn results_maps_to_results() {
        let entries = vec![wit_scored_entry("a", 100), wit_scored_entry("b", 50)];
        let response: native::PluginResponse = wit::SearchResponse::Results(entries).into();
        match response {
            native::PluginResponse::Results(entries) => {
                assert_eq!(entries.len(), 2);
                assert_eq!(entries[0].id, "a");
                assert_eq!(entries[0].score, 100);
                assert_eq!(entries[1].id, "b");
            }
            other => panic!("expected Results, got {other:?}"),
        }
    }

    #[test]
    fn custom_ui_maps_correctly() {
        let vr = wit::ViewResponse {
            view: "picker".to_string(),
            data: Some(r#"{"key": "value"}"#.to_string()),
            results: vec![wit_scored_entry("item", 42)],
        };
        let response: native::PluginResponse = wit::SearchResponse::CustomUi(vr).into();
        match response {
            native::PluginResponse::CustomUI {
                view,
                data,
                results,
            } => {
                assert_eq!(view, "picker");
                assert_eq!(data.unwrap()["key"], "value");
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].id, "item");
            }
            other => panic!("expected CustomUI, got {other:?}"),
        }
    }

    #[test]
    fn inline_ui_maps_correctly() {
        let vr = wit::ViewResponse {
            view: "result".to_string(),
            data: Some(r#"{"num": 42}"#.to_string()),
            results: vec![],
        };
        let response: native::PluginResponse = wit::SearchResponse::InlineUi(vr).into();
        match response {
            native::PluginResponse::InlineUI {
                view,
                data,
                results,
            } => {
                assert_eq!(view, "result");
                assert_eq!(data.unwrap()["num"], 42);
                assert!(results.is_empty());
            }
            other => panic!("expected InlineUI, got {other:?}"),
        }
    }

    #[test]
    fn custom_ui_null_data() {
        let vr = wit::ViewResponse {
            view: "echo".to_string(),
            data: None,
            results: vec![],
        };
        let response: native::PluginResponse = wit::SearchResponse::CustomUi(vr).into();
        match response {
            native::PluginResponse::CustomUI { data, .. } => {
                assert!(data.is_none());
            }
            other => panic!("expected CustomUI, got {other:?}"),
        }
    }

    #[test]
    fn custom_ui_invalid_json_data_degrades_gracefully() {
        let vr = wit::ViewResponse {
            view: "echo".to_string(),
            data: Some("not valid json {{{".to_string()),
            results: vec![],
        };
        let response: native::PluginResponse = wit::SearchResponse::CustomUi(vr).into();
        match response {
            native::PluginResponse::CustomUI { view, data, .. } => {
                assert_eq!(view, "echo");
                // Malformed JSON degrades to None rather than crashing.
                assert!(data.is_none());
            }
            other => panic!("expected CustomUI, got {other:?}"),
        }
    }

    #[test]
    fn custom_ui_preserves_results() {
        let entries = vec![
            wit_scored_entry("a", 100),
            wit_scored_entry("b", 80),
            wit_scored_entry("c", 60),
        ];
        let vr = wit::ViewResponse {
            view: "history".to_string(),
            data: None,
            results: entries,
        };
        let response: native::PluginResponse = wit::SearchResponse::CustomUi(vr).into();
        match response {
            native::PluginResponse::CustomUI { results, .. } => {
                assert_eq!(results.len(), 3);
                assert_eq!(results[0].score, 100);
                assert_eq!(results[2].id, "c");
            }
            other => panic!("expected CustomUI, got {other:?}"),
        }
    }

    #[test]
    fn parse_optional_json_none() {
        assert!(parse_optional_json(None).is_none());
    }

    #[test]
    fn parse_optional_json_valid() {
        let result = parse_optional_json(Some(r#"{"a": 1}"#.to_string()));
        assert_eq!(result.unwrap()["a"], 1);
    }

    #[test]
    fn parse_optional_json_malformed() {
        let result = parse_optional_json(Some("broken".to_string()));
        assert!(result.is_none());
    }
}
