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
            wit::EntryIcon::AppIcon(identifier) => native::EntryIcon::AppIcon(identifier),
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
            native::EntryIcon::AppIcon(identifier) => wit::EntryIcon::AppIcon(identifier),
        }
    }
}

// Actions carry the component's command strings unchanged; the
// host never decodes them. Keys and default labels come from the
// frontend's slot table, so nothing is filled in here.
impl From<torchsnap::gadget::types::Action> for native::Action {
    fn from(action: torchsnap::gadget::types::Action) -> Self {
        native::Action {
            label: action.label,
            command: action.command,
        }
    }
}

impl From<wit::EntryActions> for native::EntryActions {
    fn from(actions: wit::EntryActions) -> Self {
        native::EntryActions {
            primary: actions.primary.map(Into::into),
            secondary: actions.secondary.map(Into::into),
            copy: actions.copy.map(Into::into),
            reveal: actions.reveal.map(Into::into),
            delete: actions.delete.map(Into::into),
            open_settings: actions.open_settings.map(Into::into),
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
            actions: entry.actions.into(),
        }
    }
}

impl From<wit::PostAction> for native::PostAction {
    fn from(action: wit::PostAction) -> Self {
        match action {
            wit::PostAction::Nothing => native::PostAction::Nothing,
            wit::PostAction::Dismiss => native::PostAction::Dismiss,
            wit::PostAction::KeepOpen => native::PostAction::KeepOpen,
            wit::PostAction::OpenSettings => native::PostAction::OpenSettings,
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
            actions: entry.actions.into(),
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
// Icon resolution on the response path
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
//
// `AppIcon` carries a platform-native application identifier
// rather than a path. An injected [`ResolveAppIcon`] turns it
// into an absolute cached icon path, which is then assigned
// directly into the `AssetIcon` slot — that path is
// host-resolved, not gadget-authored, so it skips the
// `classify_gadget_asset_path` validation above. Without a
// resolver, or when the resolver can't find the application,
// the icon is dropped and a warning is recorded.
// =========================================================

use crate::network::website_metadata::protocol::HOST_FAVICON_SCHEME;

/// Resolves a platform-native application identifier (on macOS a
/// bundle identifier) to the absolute path of a cached icon file
/// for that application. Returns `None` when the identifier can't
/// be resolved to an installed application.
pub trait ResolveAppIcon {
    fn resolve(&self, gadget_id: &str, identifier: &str) -> Option<String>;
}

/// Rewrite gadget-relative `AssetIcon` paths and resolve `AppIcon`
/// entries in a search result payload. Returns warning messages for
/// any rejected icons — the caller (bridge) is responsible for
/// routing them to the gadget's log.
#[must_use = "warnings should be surfaced to the gadget's log"]
pub fn resolve_search_response_asset_icons(
    response: &mut native::GadgetResponse,
    gadget_id: &str,
    app_icons: Option<&dyn ResolveAppIcon>,
) -> Vec<String> {
    let entries = match response {
        native::GadgetResponse::Results(r) => r.as_mut_slice(),
        native::GadgetResponse::CustomUI { results, .. } => results.as_mut_slice(),
        native::GadgetResponse::InlineUI { results, .. } => results.as_mut_slice(),
    };
    let mut warnings = Vec::new();
    for entry in entries {
        resolve_entry_icon(&mut entry.icon, gadget_id, app_icons, &mut warnings);
    }
    warnings
}

/// Rewrite gadget-relative `AssetIcon` paths and resolve `AppIcon`
/// entries in catalog entries. Returns warning messages — see
/// [`resolve_search_response_asset_icons`].
#[must_use = "warnings should be surfaced to the gadget's log"]
pub fn resolve_catalog_entries_asset_icons(
    entries: &mut [native::CatalogEntry],
    gadget_id: &str,
    app_icons: Option<&dyn ResolveAppIcon>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    for entry in entries {
        resolve_entry_icon(&mut entry.icon, gadget_id, app_icons, &mut warnings);
    }
    warnings
}

fn resolve_entry_icon(
    slot: &mut Option<native::EntryIcon>,
    gadget_id: &str,
    app_icons: Option<&dyn ResolveAppIcon>,
    warnings: &mut Vec<String>,
) {
    if let Some(native::EntryIcon::AppIcon(identifier)) = slot.as_ref() {
        // Cloned so the identifier is still available for the warning
        // message after `slot` is reassigned below.
        let identifier = identifier.clone();
        match app_icons.and_then(|r| r.resolve(gadget_id, &identifier)) {
            Some(cached_path) => *slot = Some(native::EntryIcon::AssetIcon(cached_path)),
            None if app_icons.is_none() => {
                warnings.push(format!(
                    "AppIcon `{identifier}` requires the `icon-cache` permission; \
                     dropping icon."
                ));
                *slot = None;
            }
            None => {
                warnings.push(format!(
                    "app icon for `{identifier}` could not be resolved; dropping icon."
                ));
                *slot = None;
            }
        }
        return;
    }

    let path = match slot.as_mut() {
        Some(native::EntryIcon::AssetIcon(path)) => path,
        _ => return,
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
            actions: empty_wit_actions(),
        }
    }

    fn empty_wit_actions() -> wit::EntryActions {
        wit::EntryActions {
            primary: None,
            secondary: None,
            copy: None,
            reveal: None,
            delete: None,
            open_settings: None,
        }
    }

    fn wit_action(label: Option<&str>, command: &str) -> torchsnap::gadget::types::Action {
        torchsnap::gadget::types::Action {
            label: label.map(str::to_string),
            command: command.to_string(),
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
            actions: wit::EntryActions {
                primary: Some(wit_action(Some("Join"), r#"{"Toggle":"abc"}"#)),
                copy: Some(wit_action(None, r#"{"CopyId":"abc"}"#)),
                ..empty_wit_actions()
            },
        }
    }

    #[test]
    fn scored_entry_maps_all_fields() {
        let native_entry: native::ScoredEntry = full_wit_scored_entry().into();
        assert_eq!(native_entry.id, "e1");
        assert_eq!(native_entry.title, "Title");
        assert_eq!(native_entry.subtitle.as_deref(), Some("Sub"));
        assert!(
            matches!(native_entry.icon, Some(native::EntryIcon::HeroIcon(ref s)) if s == "star")
        );
        assert_eq!(native_entry.score, 99);
        assert_eq!(native_entry.title_positions.0, vec![0, 1, 2]);
        assert_eq!(native_entry.subtitle_positions.0, vec![5]);
    }

    #[test]
    fn slots_keep_labels_and_pass_commands_through_unchanged() {
        let native_entry: native::ScoredEntry = full_wit_scored_entry().into();
        assert_eq!(
            native_entry.actions.primary,
            Some(native::Action::labeled(
                "Join",
                r#"{"Toggle":"abc"}"#.to_string()
            ))
        );
        assert_eq!(
            native_entry.actions.copy,
            Some(native::Action {
                label: None,
                command: r#"{"CopyId":"abc"}"#.to_string(),
            })
        );
        assert_eq!(native_entry.actions.iter().count(), 2);
    }

    #[test]
    fn every_wit_slot_maps_to_the_same_native_slot() {
        let actions: native::EntryActions = wit::EntryActions {
            primary: Some(wit_action(None, "1")),
            secondary: Some(wit_action(None, "2")),
            copy: Some(wit_action(None, "3")),
            reveal: Some(wit_action(None, "4")),
            delete: Some(wit_action(None, "5")),
            open_settings: Some(wit_action(None, "6")),
        }
        .into();
        let commands: Vec<(native::Slot, &str)> = actions
            .iter()
            .map(|(slot, action)| (slot, action.command.as_str()))
            .collect();
        assert_eq!(
            commands,
            [
                (native::Slot::Primary, "1"),
                (native::Slot::Secondary, "2"),
                (native::Slot::Copy, "3"),
                (native::Slot::Reveal, "4"),
                (native::Slot::Delete, "5"),
                (native::Slot::OpenSettings, "6"),
            ]
        );
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
            actions: wit::EntryActions {
                primary: Some(wit_action(Some("Open"), "open")),
                ..empty_wit_actions()
            },
        };
        let native_entry: native::CatalogEntry = wit_entry.into();
        assert_eq!(native_entry.id, "cat-1");
        assert_eq!(native_entry.title, "Preferences");
        assert_eq!(native_entry.subtitle.as_deref(), Some("System settings"));
        assert!(matches!(native_entry.icon, Some(native::EntryIcon::Emoji(ref s)) if s == "⚙️"));
        assert_eq!(native_entry.keywords, vec!["settings", "config"]);
        assert_eq!(
            native_entry.actions.primary,
            Some(native::Action::labeled("Open", "open".to_string()))
        );
    }

    // =====================================================
    // PostAction conversion
    // =====================================================

    #[test]
    fn post_action_all_variants() {
        assert!(matches!(
            native::PostAction::from(wit::PostAction::Nothing),
            native::PostAction::Nothing
        ));
        assert!(matches!(
            native::PostAction::from(wit::PostAction::Dismiss),
            native::PostAction::Dismiss
        ));
        assert!(matches!(
            native::PostAction::from(wit::PostAction::KeepOpen),
            native::PostAction::KeepOpen
        ));
        assert!(matches!(
            native::PostAction::from(wit::PostAction::OpenSettings),
            native::PostAction::OpenSettings
        ));
    }

    // =====================================================
    // resolve_entry_icon
    // =====================================================

    #[test]
    fn relative_asset_path_is_rewritten_to_protocol_url() {
        let mut slot = Some(native::EntryIcon::AssetIcon("assets/icon.svg".into()));
        let mut warnings = Vec::new();
        resolve_entry_icon(&mut slot, "zerotier", None, &mut warnings);
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
        resolve_entry_icon(&mut slot, "any-gadget", None, &mut warnings);
        assert!(slot.is_none(), "absolute path must drop the icon");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("/var/cache/favicon.png"));
        assert!(warnings[0].contains("absolute"));
    }

    #[test]
    fn absolute_windows_path_drops_icon_and_warns() {
        let mut slot = Some(native::EntryIcon::AssetIcon("C:\\cache\\fav.png".into()));
        let mut warnings = Vec::new();
        resolve_entry_icon(&mut slot, "any-gadget", None, &mut warnings);
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
        resolve_entry_icon(&mut slot, "zerotier", None, &mut warnings);
        assert!(slot.is_none());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("torchsnap-gadget://"));
        assert!(warnings[0].contains("URL"));
    }

    #[test]
    fn non_asset_icon_variants_are_untouched() {
        let mut slot = Some(native::EntryIcon::HeroIcon("globe-alt".into()));
        let mut warnings = Vec::new();
        resolve_entry_icon(&mut slot, "zerotier", None, &mut warnings);
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
        resolve_entry_icon(&mut slot, "any-gadget", None, &mut warnings);

        match &slot {
            Some(native::EntryIcon::AssetIcon(p)) => assert_eq!(p, original),
            other => panic!("host-favicon URL must survive as AssetIcon, got {other:?}"),
        }
        assert!(
            warnings.is_empty(),
            "host-favicon URL must not produce warnings, got: {warnings:?}",
        );
    }

    // =====================================================
    // resolve_entry_icon — AppIcon
    // =====================================================

    /// Stub [`ResolveAppIcon`] for tests. Returns a fixed value
    /// (or `None`) and records the arguments of its last call.
    struct StubAppIconResolver {
        result: Option<String>,
        last_call: std::cell::RefCell<Option<(String, String)>>,
    }

    impl StubAppIconResolver {
        fn returning(result: Option<&str>) -> Self {
            Self {
                result: result.map(str::to_string),
                last_call: std::cell::RefCell::new(None),
            }
        }
    }

    impl ResolveAppIcon for StubAppIconResolver {
        fn resolve(&self, gadget_id: &str, identifier: &str) -> Option<String> {
            *self.last_call.borrow_mut() = Some((gadget_id.to_string(), identifier.to_string()));
            self.result.clone()
        }
    }

    #[test]
    fn app_icon_with_resolver_rewrites_to_cached_asset_path() {
        let resolver = StubAppIconResolver::returning(Some("/cache/gadget/abc.webp"));
        let mut slot = Some(native::EntryIcon::AppIcon("com.if.Amphetamine".into()));
        let mut warnings = Vec::new();
        resolve_entry_icon(&mut slot, "awake", Some(&resolver), &mut warnings);
        match slot {
            Some(native::EntryIcon::AssetIcon(p)) => assert_eq!(p, "/cache/gadget/abc.webp"),
            other => panic!("expected AssetIcon, got {other:?}"),
        }
        assert!(
            warnings.is_empty(),
            "no warnings expected, got {warnings:?}"
        );
    }

    #[test]
    fn app_icon_resolver_receives_gadget_id_and_identifier() {
        let resolver = StubAppIconResolver::returning(Some("/cache/gadget/abc.webp"));
        let mut slot = Some(native::EntryIcon::AppIcon("com.if.Amphetamine".into()));
        let mut warnings = Vec::new();
        resolve_entry_icon(&mut slot, "awake", Some(&resolver), &mut warnings);
        assert_eq!(
            resolver.last_call.borrow().as_ref(),
            Some(&("awake".to_string(), "com.if.Amphetamine".to_string()))
        );
    }

    #[test]
    fn app_icon_unresolvable_drops_icon_and_warns() {
        let resolver = StubAppIconResolver::returning(None);
        let mut slot = Some(native::EntryIcon::AppIcon("com.example.Missing".into()));
        let mut warnings = Vec::new();
        resolve_entry_icon(&mut slot, "awake", Some(&resolver), &mut warnings);
        assert!(slot.is_none(), "unresolvable app icon must drop the icon");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("com.example.Missing"));
    }

    #[test]
    fn app_icon_without_resolver_drops_icon_and_warns_about_permission() {
        let mut slot = Some(native::EntryIcon::AppIcon("com.if.Amphetamine".into()));
        let mut warnings = Vec::new();
        resolve_entry_icon(&mut slot, "awake", None, &mut warnings);
        assert!(slot.is_none(), "missing resolver must drop the icon");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("icon-cache"));
    }

    #[test]
    fn non_app_icon_variants_untouched_with_resolver_present() {
        let resolver = StubAppIconResolver::returning(Some("/should/not/be/used.webp"));

        let mut hero_slot = Some(native::EntryIcon::HeroIcon("globe-alt".into()));
        let mut warnings = Vec::new();
        resolve_entry_icon(&mut hero_slot, "zerotier", Some(&resolver), &mut warnings);
        assert!(matches!(&hero_slot, Some(native::EntryIcon::HeroIcon(s)) if s == "globe-alt"));

        let mut emoji_slot = Some(native::EntryIcon::Emoji("🔥".into()));
        resolve_entry_icon(&mut emoji_slot, "zerotier", Some(&resolver), &mut warnings);
        assert!(matches!(&emoji_slot, Some(native::EntryIcon::Emoji(s)) if s == "🔥"));

        let mut data_url_slot = Some(native::EntryIcon::DataUrl(
            "data:image/png;base64,AA".into(),
        ));
        resolve_entry_icon(
            &mut data_url_slot,
            "zerotier",
            Some(&resolver),
            &mut warnings,
        );
        assert!(matches!(
            &data_url_slot,
            Some(native::EntryIcon::DataUrl(_))
        ));

        assert!(warnings.is_empty());
    }

    /// Drive an `AppIcon` entry through every public resolve entry
    /// point — catalog entries and all three `GadgetResponse`
    /// shapes — with and without a resolver, and confirm no
    /// `EntryIcon::AppIcon` survives the pass in any of them.
    #[test]
    fn app_icon_never_survives_resolve_pass() {
        fn assert_no_app_icon_remains(icon: &Option<native::EntryIcon>, label: &str) {
            assert!(
                !matches!(icon, Some(native::EntryIcon::AppIcon(_))),
                "{label} still carries an AppIcon after resolution"
            );
        }

        fn scored_entry_with_app_icon(id: &str) -> native::ScoredEntry {
            native::ScoredEntry {
                id: id.to_string(),
                title: id.to_string(),
                subtitle: None,
                icon: Some(native::EntryIcon::AppIcon("com.if.Amphetamine".into())),
                score: 1,
                title_positions: crate::unicode::Utf16Positions::empty(),
                subtitle_positions: crate::unicode::Utf16Positions::empty(),
                actions: native::EntryActions::new(),
            }
        }

        for resolver in [
            None,
            Some(StubAppIconResolver::returning(Some("/cache/icon.webp"))),
        ] {
            let resolver_ref = resolver.as_ref().map(|r| r as &dyn ResolveAppIcon);

            let mut catalog_entries = vec![native::CatalogEntry {
                id: "cat".to_string(),
                title: "Catalog Entry".to_string(),
                subtitle: None,
                icon: Some(native::EntryIcon::AppIcon("com.if.Amphetamine".into())),
                keywords: vec![],
                actions: native::EntryActions::new(),
            }];
            let _ =
                resolve_catalog_entries_asset_icons(&mut catalog_entries, "awake", resolver_ref);
            assert_no_app_icon_remains(&catalog_entries[0].icon, "catalog entry");

            let mut results_response =
                native::GadgetResponse::Results(vec![scored_entry_with_app_icon("r1")]);
            let _ =
                resolve_search_response_asset_icons(&mut results_response, "awake", resolver_ref);
            match &results_response {
                native::GadgetResponse::Results(entries) => {
                    assert_no_app_icon_remains(&entries[0].icon, "Results entry")
                }
                other => panic!("expected Results, got {other:?}"),
            }

            let mut custom_ui_response = native::GadgetResponse::CustomUI {
                view: "picker".to_string(),
                data: None,
                results: vec![scored_entry_with_app_icon("c1")],
            };
            let _ =
                resolve_search_response_asset_icons(&mut custom_ui_response, "awake", resolver_ref);
            match &custom_ui_response {
                native::GadgetResponse::CustomUI { results, .. } => {
                    assert_no_app_icon_remains(&results[0].icon, "CustomUI entry")
                }
                other => panic!("expected CustomUI, got {other:?}"),
            }

            let mut inline_ui_response = native::GadgetResponse::InlineUI {
                view: "result".to_string(),
                data: None,
                results: vec![scored_entry_with_app_icon("i1")],
            };
            let _ =
                resolve_search_response_asset_icons(&mut inline_ui_response, "awake", resolver_ref);
            match &inline_ui_response {
                native::GadgetResponse::InlineUI { results, .. } => {
                    assert_no_app_icon_remains(&results[0].icon, "InlineUI entry")
                }
                other => panic!("expected InlineUI, got {other:?}"),
            }
        }
    }

    // =====================================================
    // EntryIcon::AppIcon conversions
    // =====================================================

    #[test]
    fn app_icon_wit_to_native_round_trip() {
        let native_icon: native::EntryIcon =
            wit::EntryIcon::AppIcon("com.if.Amphetamine".to_string()).into();
        match native_icon {
            native::EntryIcon::AppIcon(id) => assert_eq!(id, "com.if.Amphetamine"),
            other => panic!("expected AppIcon, got {other:?}"),
        }
    }

    #[test]
    fn app_icon_native_to_wit_round_trip() {
        let wit_icon: wit::EntryIcon =
            native::EntryIcon::AppIcon("com.if.Amphetamine".to_string()).into();
        match wit_icon {
            wit::EntryIcon::AppIcon(id) => assert_eq!(id, "com.if.Amphetamine"),
            other => panic!("expected AppIcon, got {other:?}"),
        }
    }
}
