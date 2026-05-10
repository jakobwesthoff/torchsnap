// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WASM Component Bindings
//
// Generates typed Rust bindings from the WIT definitions via
// `wasmtime::component::bindgen!`. This produces:
//
// - A `Gadget` struct with methods to call guest exports
//   (e.g., `call_enable`, `call_entries`, `call_execute`)
// - Traits for each host import interface that we implement
//   on the store state (e.g., `logging::Host`)
// - Typed records/enums mirroring the WIT definitions
//
// This module also provides `From` conversions between the
// generated types and the native `commands::types` used by
// the rest of the application.
// =========================================================

wasmtime::component::bindgen!({
    path: "../gadgets/gadget-sdk/wit",
    world: "gadget",
    // Tell wit-bindgen to use our concrete `SqlHandleEntry`
    // type as the resource representation for the
    // `sql-storage.sql-handle` resource. By default, bindgen
    // generates an empty unit struct — `with` lets us
    // swap in a real type carrying the `Arc<SqlStorage>`
    // associated with each outstanding handle.
    with: {
        "torchsnap:gadget/sql-storage.sql-handle": super::runtime::SqlHandleEntry,
    },
});

// =========================================================
// Empty `Host` impl for the types-only interface
// =========================================================
//
// `interface types` exposes only variant/record definitions,
// no functions. wasmtime bindgen still generates a `Host`
// trait for it (with no methods) and the world's
// `add_to_linker` pass requires every imported interface to
// have an impl. One-line empty impl satisfies the trait.

use crate::wasm::runtime::GadgetState;

impl torchsnap::gadget::types::Host for GadgetState {}

// =========================================================
// Type Conversions: WIT types → native types
// =========================================================

// All search/UI types now live inside the `search` guest
// interface, not a separate imported `types` interface — see
// ADR 0029. Because `search` is a guest export from the
// host's perspective, its generated module lives under
// `exports::torchsnap::gadget::search` instead of the
// import-side path the old `types` interface used.
use crate::commands::types as native;
use exports::torchsnap::gadget::search as wit;

// =========================================================
// Frecency: host → WIT
//
// The `frecency` import lets gadgets read their own top-N
// items from the host's frecency store. Host items come out
// of `GadgetFrecency::top_items` as the native `FrecencyItem`
// struct; this conversion maps them onto the bindgen-
// generated record so the Host impl can return them across
// the WIT boundary directly.
// =========================================================

impl From<crate::frecency::FrecencyItem> for torchsnap::gadget::frecency::FrecencyItem {
    fn from(item: crate::frecency::FrecencyItem) -> Self {
        torchsnap::gadget::frecency::FrecencyItem {
            item_id: item.item_id,
            score: item.score,
        }
    }
}

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

impl From<native::EntryIcon> for wit::EntryIcon {
    fn from(icon: native::EntryIcon) -> Self {
        match icon {
            native::EntryIcon::HeroIcon(name) => wit::EntryIcon::HeroIcon(name),
            native::EntryIcon::DataUrl(data) => wit::EntryIcon::DataUrl(data),
            native::EntryIcon::AssetIcon(path) => wit::EntryIcon::AssetIcon(path),
            native::EntryIcon::Emoji(emoji) => wit::EntryIcon::Emoji(emoji),
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
            wit::ActionId::OpenSettings => native::ActionId::OpenSettings,
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
            native::ActionId::OpenSettings => wit::ActionId::OpenSettings,
            native::ActionId::Custom(s) => wit::ActionId::Custom(s),
        }
    }
}

impl From<wit::Action> for native::Action {
    fn from(action: wit::Action) -> Self {
        let id: native::ActionId = action.id.into();
        // Keybindings for well-known `ActionId`s are filled by the
        // host. Without this, secondary actions on WASM gadget
        // entries are invisible in the launcher footer (the footer
        // filters out actions without a keybinding) and unreachable
        // by keyboard (`useKeyboardNavigation` only registers
        // handlers for actions that carry one).
        let keybinding = default_keybinding_for(&id);
        native::Action {
            id,
            label: action.label,
            keybinding,
        }
    }
}

/// Default keybinding assignment for the well-known [`native::ActionId`]
/// variants. `Open` stays unbound — the launcher wires Enter to the
/// selected entry's primary action separately. `Custom` actions are
/// gadget-specific and get no host-side default.
fn default_keybinding_for(id: &native::ActionId) -> Option<native::ActionKeybinding> {
    use native::{ActionId, ActionKeybinding};
    let (mods, key): (&[&str], &str) = match id {
        ActionId::Open => return None,
        ActionId::Copy => (&["Meta"], "c"),
        ActionId::Reveal => (&["Meta", "Shift"], "r"),
        ActionId::OpenWith => (&["Meta", "Shift"], "o"),
        ActionId::Delete => (&["Meta"], "Backspace"),
        ActionId::OpenSettings => (&["Meta"], ","),
        ActionId::Custom(_) => return None,
    };
    Some(ActionKeybinding {
        modifiers: mods.iter().map(|s| (*s).to_string()).collect(),
        key: key.to_string(),
    })
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
            data: entry.data,
        }
    }
}

impl From<native::ScoredEntry> for wit::ScoredEntry {
    fn from(entry: native::ScoredEntry) -> Self {
        wit::ScoredEntry {
            id: entry.id,
            title: entry.title,
            subtitle: entry.subtitle,
            icon: entry.icon.map(Into::into),
            score: entry.score,
            title_highlight_positions: entry.title_positions.0,
            subtitle_highlight_positions: entry.subtitle_positions.0,
            actions: entry.actions.into_iter().map(Into::into).collect(),
            data: entry.data,
        }
    }
}

impl From<native::Action> for wit::Action {
    fn from(action: native::Action) -> Self {
        wit::Action {
            id: action.id.into(),
            label: action.label,
        }
    }
}

impl From<wit::SearchResponse> for native::GadgetResponse {
    fn from(response: wit::SearchResponse) -> Self {
        match response {
            wit::SearchResponse::Nothing => native::GadgetResponse::Results(vec![]),
            wit::SearchResponse::Results(entries) => {
                native::GadgetResponse::Results(entries.into_iter().map(Into::into).collect())
            }
            wit::SearchResponse::CustomUi(vr) => native::GadgetResponse::CustomUI {
                view: vr.view,
                data: parse_optional_json(vr.data),
                results: vr.results.into_iter().map(Into::into).collect(),
            },
            wit::SearchResponse::InlineUi(vr) => native::GadgetResponse::InlineUI {
                view: vr.view,
                data: parse_optional_json(vr.data),
                results: vr.results.into_iter().map(Into::into).collect(),
            },
        }
    }
}

/// Parse an optional JSON-encoded string into a `serde_json::Value`.
/// Returns `None` if the input is `None` or if the JSON is malformed
/// (graceful degradation — a gadget sending bad JSON shouldn't crash
/// the host).
fn parse_optional_json(json: Option<String>) -> Option<serde_json::Value> {
    json.and_then(|s| serde_json::from_str(&s).ok())
}

// =========================================================
// AssetIcon resolution on the response path
//
// `AssetIcon` carries one of three things:
//
// 1. Gadget-relative paths (`assets/icon.svg`). Gadgets
//    bundle assets inside their `.torchsnap` archive and
//    reference them by relative path. We rewrite these into
//    `torchsnap-gadget://localhost/<gadget-id>/<path>` URLs
//    so the gadget asset URI scheme (`wasm/protocol.rs`)
//    serves them to the launcher.
//
// 2. `torchsnap-favicon://localhost/<key>.<ext>` URLs
//    produced by `WebsiteMetadataService`. The host owns
//    the scheme via `network::website_metadata::protocol`
//    and serves bytes from the favicon cache. These travel
//    host → guest → host (the gadget embeds the URL the
//    host handed it back into the search response), so the
//    resolver passes them through unchanged.
//
// 3. Anything else — absolute filesystem paths, arbitrary
//    qualified URLs from gadgets. Rejected: the icon is
//    dropped (`None`) and a warning is recorded for the
//    caller to log. Gadgets have no business pointing
//    `AssetIcon` at arbitrary host filesystem locations or
//    cross-gadget protocol URLs; the host-favicon scheme is
//    the one whitelisted exception.
// =========================================================

use crate::network::website_metadata::protocol::HOST_FAVICON_SCHEME;

/// Rewrite gadget-relative `AssetIcon` paths in a search result
/// payload. Returns warning messages for any rejected icons —
/// the caller (bridge) is responsible for routing them to the
/// gadget's log.
#[must_use = "warnings should be surfaced to the gadget's log"]
pub fn resolve_search_response_asset_icons(
    response: &mut native::GadgetResponse,
    gadget_id: &str,
) -> Vec<String> {
    let entries = match response {
        native::GadgetResponse::Results(r) => r.as_mut_slice(),
        native::GadgetResponse::CustomUI { results, .. } => results.as_mut_slice(),
        native::GadgetResponse::InlineUI { results, .. } => results.as_mut_slice(),
    };
    let mut warnings = Vec::new();
    for entry in entries {
        resolve_entry_icon(&mut entry.icon, gadget_id, &mut warnings);
    }
    warnings
}

/// Rewrite gadget-relative `AssetIcon` paths in catalog entries.
/// Returns warning messages — see [`resolve_search_response_asset_icons`].
#[must_use = "warnings should be surfaced to the gadget's log"]
pub fn resolve_catalog_entries_asset_icons(
    entries: &mut [native::CatalogEntry],
    gadget_id: &str,
) -> Vec<String> {
    let mut warnings = Vec::new();
    for entry in entries {
        resolve_entry_icon(&mut entry.icon, gadget_id, &mut warnings);
    }
    warnings
}

fn resolve_entry_icon(
    slot: &mut Option<native::EntryIcon>,
    gadget_id: &str,
    warnings: &mut Vec<String>,
) {
    let Some(native::EntryIcon::AssetIcon(path)) = slot.as_mut() else {
        return;
    };
    match classify_gadget_asset_path(path) {
        AssetPathClass::Relative => {
            *path = format!("torchsnap-gadget://localhost/{gadget_id}/{path}");
        }
        AssetPathClass::HostFavicon => {
            // Host-issued favicon URL. Passes through unchanged
            // — the host owns the scheme and the cache root, so
            // there is no gadget-issued payload to validate.
        }
        AssetPathClass::AbsolutePath => {
            warnings.push(format!(
                "AssetIcon path `{path}` is absolute; gadgets must reference \
                 bundled assets with a relative path (e.g. `assets/icon.svg`). \
                 Dropping icon."
            ));
            *slot = None;
        }
        AssetPathClass::QualifiedUrl => {
            warnings.push(format!(
                "AssetIcon path `{path}` is a fully-qualified URL; gadgets \
                 must reference bundled assets with a relative path (e.g. \
                 `assets/icon.svg`). Dropping icon."
            ));
            *slot = None;
        }
    }
}

enum AssetPathClass {
    Relative,
    AbsolutePath,
    QualifiedUrl,
    HostFavicon,
}

fn classify_gadget_asset_path(path: &str) -> AssetPathClass {
    // Match the host-favicon scheme before the generic `://`
    // arm so it doesn't fall through to `QualifiedUrl`.
    if path.starts_with(HOST_FAVICON_SCHEME) {
        return AssetPathClass::HostFavicon;
    }
    if path.contains("://") {
        return AssetPathClass::QualifiedUrl;
    }
    if path.starts_with('/') {
        return AssetPathClass::AbsolutePath;
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
    {
        return AssetPathClass::AbsolutePath;
    }
    AssetPathClass::Relative
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
            data: None,
        }
    }

    #[test]
    fn nothing_maps_to_empty_results() {
        let response: native::GadgetResponse = wit::SearchResponse::Nothing.into();
        match response {
            native::GadgetResponse::Results(entries) => assert!(entries.is_empty()),
            other => panic!("expected Results, got {other:?}"),
        }
    }

    #[test]
    fn results_maps_to_results() {
        let entries = vec![wit_scored_entry("a", 100), wit_scored_entry("b", 50)];
        let response: native::GadgetResponse = wit::SearchResponse::Results(entries).into();
        match response {
            native::GadgetResponse::Results(entries) => {
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
        let response: native::GadgetResponse = wit::SearchResponse::CustomUi(vr).into();
        match response {
            native::GadgetResponse::CustomUI {
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
        let response: native::GadgetResponse = wit::SearchResponse::InlineUi(vr).into();
        match response {
            native::GadgetResponse::InlineUI {
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
        let response: native::GadgetResponse = wit::SearchResponse::CustomUi(vr).into();
        match response {
            native::GadgetResponse::CustomUI { data, .. } => {
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
        let response: native::GadgetResponse = wit::SearchResponse::CustomUi(vr).into();
        match response {
            native::GadgetResponse::CustomUI { view, data, .. } => {
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
        let response: native::GadgetResponse = wit::SearchResponse::CustomUi(vr).into();
        match response {
            native::GadgetResponse::CustomUI { results, .. } => {
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

    // =====================================================
    // ScoredEntry conversion (WIT → native)
    // =====================================================

    fn full_wit_scored_entry() -> wit::ScoredEntry {
        wit::ScoredEntry {
            id: "e1".to_string(),
            title: "Title".to_string(),
            subtitle: Some("Sub".to_string()),
            icon: Some(wit::EntryIcon::HeroIcon("star".to_string())),
            score: 99,
            title_highlight_positions: vec![0, 1, 2],
            subtitle_highlight_positions: vec![5],
            actions: vec![wit::Action {
                id: wit::ActionId::Open,
                label: "Open".to_string(),
            }],
            data: Some("payload".to_string()),
        }
    }

    #[test]
    fn scored_entry_maps_all_fields() {
        let native_entry: native::ScoredEntry = full_wit_scored_entry().into();
        assert_eq!(native_entry.id, "e1");
        assert_eq!(native_entry.title, "Title");
        assert_eq!(native_entry.subtitle.as_deref(), Some("Sub"));
        assert!(matches!(native_entry.icon, Some(native::EntryIcon::HeroIcon(ref s)) if s == "star"));
        assert_eq!(native_entry.score, 99);
        assert_eq!(native_entry.title_positions.0, vec![0, 1, 2]);
        assert_eq!(native_entry.subtitle_positions.0, vec![5]);
        assert_eq!(native_entry.actions.len(), 1);
        assert_eq!(native_entry.actions[0].label, "Open");
        assert_eq!(native_entry.data.as_deref(), Some("payload"));
    }

    #[test]
    fn scored_entry_data_none_preserved() {
        let mut wit_entry = full_wit_scored_entry();
        wit_entry.data = None;
        let native_entry: native::ScoredEntry = wit_entry.into();
        assert!(native_entry.data.is_none());
    }

    // =====================================================
    // ScoredEntry conversion (native → WIT)
    // =====================================================

    fn full_native_scored_entry() -> native::ScoredEntry {
        native::ScoredEntry {
            id: "n1".to_string(),
            title: "Native Title".to_string(),
            subtitle: Some("Native Sub".to_string()),
            icon: Some(native::EntryIcon::Emoji("🔥".to_string())),
            score: 77,
            title_positions: crate::unicode::Utf16Positions(vec![3, 4]),
            subtitle_positions: crate::unicode::Utf16Positions(vec![]),
            actions: vec![native::Action {
                id: native::ActionId::Copy,
                label: "Copy".to_string(),
                keybinding: Some(native::ActionKeybinding {
                    modifiers: vec!["Meta".to_string()],
                    key: "c".to_string(),
                }),
            }],
            data: Some(r#"{"url":"https://example.com"}"#.to_string()),
        }
    }

    #[test]
    fn native_to_wit_scored_entry_maps_all_fields() {
        let wit_entry: wit::ScoredEntry = full_native_scored_entry().into();
        assert_eq!(wit_entry.id, "n1");
        assert_eq!(wit_entry.title, "Native Title");
        assert_eq!(wit_entry.subtitle.as_deref(), Some("Native Sub"));
        assert!(matches!(wit_entry.icon, Some(wit::EntryIcon::Emoji(ref s)) if s == "🔥"));
        assert_eq!(wit_entry.score, 77);
        assert_eq!(wit_entry.title_highlight_positions, vec![3, 4]);
        assert!(wit_entry.subtitle_highlight_positions.is_empty());
        assert_eq!(wit_entry.actions.len(), 1);
        assert!(matches!(wit_entry.actions[0].id, wit::ActionId::Copy));
        assert_eq!(wit_entry.data.as_deref(), Some(r#"{"url":"https://example.com"}"#));
    }

    #[test]
    fn native_to_wit_action_drops_keybinding() {
        let action = native::Action {
            id: native::ActionId::Reveal,
            label: "Show".to_string(),
            keybinding: Some(native::ActionKeybinding {
                modifiers: vec!["Meta".to_string(), "Shift".to_string()],
                key: "r".to_string(),
            }),
        };
        let wit_action: wit::Action = action.into();
        assert!(matches!(wit_action.id, wit::ActionId::Reveal));
        assert_eq!(wit_action.label, "Show");
    }

    #[test]
    fn scored_entry_round_trips_through_wit() {
        let original = full_native_scored_entry();
        let wit: wit::ScoredEntry = original.clone().into();
        let back: native::ScoredEntry = wit.into();
        assert_eq!(back.id, original.id);
        assert_eq!(back.title, original.title);
        assert_eq!(back.subtitle, original.subtitle);
        assert_eq!(back.score, original.score);
        assert_eq!(back.title_positions.0, original.title_positions.0);
        assert_eq!(back.data, original.data);
    }

    // =====================================================
    // ActionId conversions
    // =====================================================

    #[test]
    fn action_id_wit_to_native_all_variants() {
        assert!(matches!(native::ActionId::from(wit::ActionId::Open), native::ActionId::Open));
        assert!(matches!(native::ActionId::from(wit::ActionId::Copy), native::ActionId::Copy));
        assert!(matches!(native::ActionId::from(wit::ActionId::Reveal), native::ActionId::Reveal));
        assert!(matches!(native::ActionId::from(wit::ActionId::OpenWith), native::ActionId::OpenWith));
        assert!(matches!(native::ActionId::from(wit::ActionId::Delete), native::ActionId::Delete));
        assert!(matches!(native::ActionId::from(wit::ActionId::OpenSettings), native::ActionId::OpenSettings));
        match native::ActionId::from(wit::ActionId::Custom("foo".into())) {
            native::ActionId::Custom(s) => assert_eq!(s, "foo"),
            other => panic!("expected Custom, got {other:?}"),
        }
    }

    #[test]
    fn action_id_native_to_wit_all_variants() {
        assert!(matches!(wit::ActionId::from(native::ActionId::Open), wit::ActionId::Open));
        assert!(matches!(wit::ActionId::from(native::ActionId::Copy), wit::ActionId::Copy));
        assert!(matches!(wit::ActionId::from(native::ActionId::Reveal), wit::ActionId::Reveal));
        assert!(matches!(wit::ActionId::from(native::ActionId::OpenWith), wit::ActionId::OpenWith));
        assert!(matches!(wit::ActionId::from(native::ActionId::Delete), wit::ActionId::Delete));
        assert!(matches!(wit::ActionId::from(native::ActionId::OpenSettings), wit::ActionId::OpenSettings));
        match wit::ActionId::from(native::ActionId::Custom("bar".into())) {
            wit::ActionId::Custom(s) => assert_eq!(s, "bar"),
            other => panic!("expected Custom, got {other:?}"),
        }
    }

    // =====================================================
    // CatalogEntry conversion
    // =====================================================

    #[test]
    fn catalog_entry_maps_all_fields() {
        let wit_entry = wit::CatalogEntry {
            id: "cat-1".to_string(),
            title: "Preferences".to_string(),
            subtitle: Some("System settings".to_string()),
            icon: Some(wit::EntryIcon::Emoji("⚙️".to_string())),
            keywords: vec!["settings".to_string(), "config".to_string()],
            actions: vec![wit::Action {
                id: wit::ActionId::Open,
                label: "Open".to_string(),
            }],
        };
        let native_entry: native::CatalogEntry = wit_entry.into();
        assert_eq!(native_entry.id, "cat-1");
        assert_eq!(native_entry.title, "Preferences");
        assert_eq!(native_entry.subtitle.as_deref(), Some("System settings"));
        assert!(matches!(native_entry.icon, Some(native::EntryIcon::Emoji(ref s)) if s == "⚙️"));
        assert_eq!(native_entry.keywords, vec!["settings", "config"]);
        assert_eq!(native_entry.actions.len(), 1);
    }

    // =====================================================
    // PostAction conversion
    // =====================================================

    #[test]
    fn post_action_all_variants() {
        assert!(matches!(native::PostAction::from(wit::PostAction::Nothing), native::PostAction::Nothing));
        assert!(matches!(native::PostAction::from(wit::PostAction::Dismiss), native::PostAction::Dismiss));
        assert!(matches!(native::PostAction::from(wit::PostAction::KeepOpen), native::PostAction::KeepOpen));
    }

    // =====================================================
    // resolve_entry_icon
    // =====================================================

    #[test]
    fn relative_asset_path_is_rewritten_to_protocol_url() {
        let mut slot = Some(native::EntryIcon::AssetIcon("assets/icon.svg".into()));
        let mut warnings = Vec::new();
        resolve_entry_icon(&mut slot, "zerotier", &mut warnings);
        match slot {
            Some(native::EntryIcon::AssetIcon(p)) => {
                assert_eq!(p, "torchsnap-gadget://localhost/zerotier/assets/icon.svg");
            }
            other => panic!("expected AssetIcon, got {other:?}"),
        }
        assert!(
            warnings.is_empty(),
            "no warnings expected, got {warnings:?}"
        );
    }

    #[test]
    fn absolute_unix_path_drops_icon_and_warns() {
        let mut slot = Some(native::EntryIcon::AssetIcon(
            "/var/cache/favicon.png".into(),
        ));
        let mut warnings = Vec::new();
        resolve_entry_icon(&mut slot, "any-gadget", &mut warnings);
        assert!(slot.is_none(), "absolute path must drop the icon");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("/var/cache/favicon.png"));
        assert!(warnings[0].contains("absolute"));
    }

    #[test]
    fn absolute_windows_path_drops_icon_and_warns() {
        let mut slot = Some(native::EntryIcon::AssetIcon("C:\\cache\\fav.png".into()));
        let mut warnings = Vec::new();
        resolve_entry_icon(&mut slot, "any-gadget", &mut warnings);
        assert!(slot.is_none(), "absolute path must drop the icon");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("C:\\cache\\fav.png"));
    }

    #[test]
    fn already_qualified_url_drops_icon_and_warns() {
        // Layering: gadgets must not reach across to other
        // gadgets' archives by hand-crafting protocol URLs.
        let mut slot = Some(native::EntryIcon::AssetIcon(
            "torchsnap-gadget://localhost/other/secret.svg".into(),
        ));
        let mut warnings = Vec::new();
        resolve_entry_icon(&mut slot, "zerotier", &mut warnings);
        assert!(slot.is_none());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("torchsnap-gadget://"));
        assert!(warnings[0].contains("URL"));
    }

    #[test]
    fn non_asset_icon_variants_are_untouched() {
        let mut slot = Some(native::EntryIcon::HeroIcon("globe-alt".into()));
        let mut warnings = Vec::new();
        resolve_entry_icon(&mut slot, "zerotier", &mut warnings);
        assert!(matches!(&slot, Some(native::EntryIcon::HeroIcon(s)) if s == "globe-alt"));
        assert!(warnings.is_empty());
    }

    #[test]
    fn host_favicon_url_passes_through_unchanged() {
        // Regression: host-issued favicons travel as
        // `torchsnap-favicon://localhost/<key>.<ext>` URLs from
        // the website-metadata service through the gadget's
        // search response back to the host. The resolver must
        // pass these through unchanged — they're host-managed,
        // not gadget-issued. Before this rule existed, the
        // generic `://` arm classified them as `QualifiedUrl`
        // and dropped the icon, leaving the launcher to render
        // the `command-line` HeroIcon fallback for every URL/
        // bang result.
        let original = "torchsnap-favicon://localhost/abc123.webp";
        let mut slot = Some(native::EntryIcon::AssetIcon(original.into()));
        let mut warnings = Vec::new();
        resolve_entry_icon(&mut slot, "any-gadget", &mut warnings);

        match &slot {
            Some(native::EntryIcon::AssetIcon(p)) => assert_eq!(p, original),
            other => panic!("host-favicon URL must survive as AssetIcon, got {other:?}"),
        }
        assert!(
            warnings.is_empty(),
            "host-favicon URL must not produce warnings, got: {warnings:?}",
        );
    }
}
