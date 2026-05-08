// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Search Types
//
// Shared types for the search pipeline. CatalogEntry is
// internal (Rust only). Everything else is serialized across
// the Tauri bridge to the frontend.
// =========================================================

use serde::{Deserialize, Serialize};

use crate::frecency::FrecencyTarget;
use crate::unicode::Utf16Positions;

// =========================================================
// Post-Action Behavior
// =========================================================

/// What the launcher should do after executing a gadget action.
///
/// Returned by `Gadget::execute()`
/// to let the gadget control whether the launcher stays open.
/// Serialized to the frontend so it can act on the decision.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum PostAction {
    /// Do nothing — no launcher state change.
    Nothing,
    /// Hide the launcher (default for most actions).
    Dismiss,
    /// Keep the launcher open (e.g., for multi-select workflows).
    #[allow(dead_code)]
    KeepOpen,
    /// Switch to the gadget's custom UI component. The frontend
    /// mounts the component registered for the executing gadget's
    /// ID and view name, replacing the standard result list.
    ShowCustomUI {
        /// Named view to mount (must match a key in the gadget's
        /// `views` registry on the frontend).
        view: String,
        /// Optional data payload forwarded to the view component.
        data: Option<serde_json::Value>,
    },
    /// Exit the application. Handled by the host; never forwarded to the frontend.
    Quit,
    /// Open the settings window. Handled by the host; never forwarded to the frontend.
    ShowSettings,
    /// Open the developer tools window. Handled by the host; never forwarded to the frontend.
    ShowDevtools,
}

// =========================================================
// Action Types
// =========================================================

/// Well-known action IDs with an escape hatch for custom gadget actions.
///
/// Using an enum rather than bare strings lets us exhaustively match
/// for default keybinding assignment, display hints, and icon mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum ActionId {
    Open,
    Copy,
    Reveal,
    OpenWith,
    Delete,
    /// Jump to the originating gadget's settings panel.
    /// Useful as the primary action on synthetic
    /// "configuration required" entries.
    OpenSettings,
    Custom(String),
}

/// Keybinding info for an action, sent to the frontend for both
/// rendering shortcut hints and registering dynamic key handlers.
///
/// Uses the same modifier vocabulary as the frontend keybinding
/// engine: `"Meta"` (Cmd on macOS, Ctrl on others), `"Shift"`,
/// `"Alt"`. The display label is derived from these in the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionKeybinding {
    /// Modifier keys, e.g. `["Meta"]` or `["Meta", "Shift"]`.
    pub modifiers: Vec<String>,
    /// The key name, e.g. `"Enter"`, `"c"`, `"Backspace"`.
    pub key: String,
}

/// A single action that can be performed on a result entry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub id: ActionId,
    pub label: String,
    pub keybinding: Option<ActionKeybinding>,
}

// =========================================================
// Entry Types
// =========================================================

/// Icon specification for result entries. The frontend resolves
/// these to actual rendered elements.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum EntryIcon {
    /// Name of a Heroicon (e.g. "x-circle", "cog-6-tooth").
    HeroIcon(String),
    /// Base64-encoded data URL for inline images.
    /// Not currently constructed but available for future gadgets.
    #[allow(dead_code)]
    DataUrl(String),
    /// Absolute filesystem path to a cached image file. The frontend
    /// converts this to an asset protocol URL via Tauri's
    /// `convertFileSrc` API.
    AssetIcon(String),
    /// A Unicode emoji character rendered as text in the icon slot.
    Emoji(String),
}

/// A pre-scored result returned by a `Gadget`'s `search()` method.
///
/// Intentionally omits `source` — gadget authors should not set or
/// even think about this field. The host attaches it when wrapping
/// into `SourcedEntry` via `SourcedEntry::new`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoredEntry {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<EntryIcon>,
    pub score: u32,
    pub title_positions: Utf16Positions,
    pub subtitle_positions: Utf16Positions,
    pub actions: Vec<Action>,
    /// Opaque gadget-defined payload round-tripped by the host.
    /// Set in `search()`, passed back to `execute()` via the
    /// entry store. Never serialized to the frontend.
    #[serde(skip)]
    pub data: Option<String>,
}

impl FrecencyTarget for ScoredEntry {
    fn item_id(&self) -> &str {
        &self.id
    }
    fn boost_score(&mut self, bonus: u32) {
        self.score = self.score.saturating_add(bonus);
    }
}

/// A raw catalog entry before scoring. Internal to the Rust side —
/// gadgets produce these, the catalog registry scores them, and
/// `SourcedEntry` is what crosses the bridge to the frontend.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<EntryIcon>,
    /// Additional match targets beyond the title. These are used
    /// for scoring but their match positions are not highlighted.
    pub keywords: Vec<String>,
    /// Ordered list of actions. The first action is the primary
    /// action triggered by Enter.
    pub actions: Vec<Action>,
}

/// A `ScoredEntry` attributed to its originating gadget, ready
/// for the frontend.
///
/// Uses `#[serde(flatten)]` so the serialized form is a flat
/// object (no nesting). This is fine because `SourcedEntry` is
/// serialize-only. If `Deserialize` is ever needed, note that
/// `flatten` degrades deserialization error messages and uses a
/// slower `Map`-based collection path — at that point consider
/// whether manual field copying is preferable.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcedEntry {
    /// Which gadget produced this entry (gadget ID).
    pub source: String,
    #[serde(flatten)]
    pub inner: ScoredEntry,
}

impl SourcedEntry {
    /// Wrap a `ScoredEntry` with its originating gadget ID.
    pub fn new(source: String, inner: ScoredEntry) -> Self {
        Self { source, inner }
    }

    /// Deterministic composite sort key: score DESC, source ASC, id ASC.
    ///
    /// This ordering is the single source of truth on the Rust side.
    /// The TypeScript frontend has an equivalent comparator in
    /// `src/launcher/compareEntries.ts` that MUST stay in sync with
    /// this implementation. Any change here requires a matching change
    /// there (and vice versa).
    pub fn cmp_sort_key(&self, other: &Self) -> std::cmp::Ordering {
        other
            .inner
            .score
            .cmp(&self.inner.score)
            .then_with(|| self.source.cmp(&other.source))
            .then_with(|| self.inner.id.cmp(&other.inner.id))
    }
}

impl FrecencyTarget for SourcedEntry {
    fn item_id(&self) -> &str {
        &self.inner.id
    }
    fn boost_score(&mut self, bonus: u32) {
        self.inner.score = self.inner.score.saturating_add(bonus);
    }
}

// =========================================================
// Gadget-to-Host Channel
//
// Gadgets push results into a `ResultChannel` during search.
// The host reads `GadgetResponse` values from the receiving end
// and translates them into `SearchMessage`s for the frontend.
// =========================================================

/// Return type for `Gadget::search()`. Not serialized — only
/// used between gadget and host within the same process.
#[derive(Debug, Clone)]
pub enum GadgetResponse {
    /// Standard result list entries.
    Results(Vec<ScoredEntry>),
    /// Gadget requests full custom UI (replaces the result list).
    CustomUI {
        view: String,
        data: Option<serde_json::Value>,
        results: Vec<ScoredEntry>,
    },
    /// Gadget requests inline UI (rendered above the result list).
    InlineUI {
        view: String,
        data: Option<serde_json::Value>,
        results: Vec<ScoredEntry>,
    },
}

// =========================================================
// Channel Messages
// =========================================================

/// Reference to a gadget view component for frontend resolution.
///
/// Sent to the frontend so it can look up the correct React component
/// in the gadget registry: `registry[gadgetId].views[view]` for
/// `CustomUI`, `registry[gadgetId].inlineViews[view]` for `InlineUI`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GadgetViewRef {
    pub gadget_id: String,
    pub view: String,
    pub data: Option<serde_json::Value>,
}

/// Messages streamed over a Tauri channel during a search.
///
/// The frontend receives these progressively: catalog results
/// arrive first (sub-millisecond for static catalogs), then
/// query gadget results stream in as each gadget completes, and
/// `Done` signals that all gadgets have finished.
///
/// There is a single `SearchResults` variant for all result
/// sources (catalogs and query gadgets alike). The frontend
/// merges each message into its accumulated sorted array using
/// the same algorithm — no special-casing needed.
// The size gap between `SearchResults` and `Done` is large, but
// these values are transient — created, serialized over a Tauri
// channel, and dropped immediately. They are never stored in
// collections or passed around by value in hot paths, so the
// extra stack space of `Done` matching `SearchResults` is not
// a practical concern.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SearchMessage {
    /// A batch of search results from a catalog or query gadget.
    ///
    /// The `rename_all` on the enum only renames variant tags, not
    /// fields within variants. Fields need explicit renaming.
    #[serde(rename_all = "camelCase")]
    SearchResults {
        /// The frontend keys its per-source accumulator off
        /// this so a source going from results to empty on a
        /// later keystroke can evict its prior entries.
        source: ResultSource,
        entries: Vec<SourcedEntry>,
        /// When a query gadget requested custom UI, this contains
        /// a view reference so the frontend can mount the gadget's
        /// React component. `None` for standard list rendering.
        custom_gadget_view: Option<GadgetViewRef>,
        /// When a query gadget requested inline UI, this contains
        /// a view reference for the inline component rendered above
        /// the result list.
        inline_gadget_view: Option<GadgetViewRef>,
        /// The prefix that triggered exclusive routing. Sent to the
        /// frontend so the gadget component knows which prefix was
        /// matched. `None` when no prefix routing occurred.
        matched_prefix: Option<String>,
    },
    Done,
}

/// `Catalog` is one aggregated batch mixing rows from every
/// catalog-providing gadget — it replaces the catalog layer
/// wholesale, not per-gadget.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ResultSource {
    Gadget { id: String },
    Catalog,
}

#[cfg(test)]
mod scored_entry_tests {
    use super::*;

    fn sample_scored_entry() -> ScoredEntry {
        ScoredEntry {
            id: "test-id".to_string(),
            title: "Test Title".to_string(),
            subtitle: Some("A subtitle".to_string()),
            icon: Some(EntryIcon::HeroIcon("star".to_string())),
            score: 42,
            title_positions: Utf16Positions(vec![0, 1, 2]),
            subtitle_positions: Utf16Positions::empty(),
            actions: vec![Action {
                id: ActionId::Open,
                label: "Open".to_string(),
                keybinding: None,
            }],
            data: None,
        }
    }

    #[test]
    fn data_field_excluded_from_serialization() {
        let mut entry = sample_scored_entry();
        entry.data = Some("secret payload".to_string());
        let json = serde_json::to_value(&entry).expect("serialize");
        assert!(json.get("data").is_none(), "data must not appear in serialized JSON");
    }

    #[test]
    fn data_field_excluded_from_sourced_entry_serialization() {
        let mut entry = sample_scored_entry();
        entry.data = Some("secret payload".to_string());
        let sourced = SourcedEntry::new("gadget".to_string(), entry);
        let json = serde_json::to_value(&sourced).expect("serialize");
        assert!(json.get("data").is_none(), "data must not leak through flatten");
    }

    #[test]
    fn serialization_contains_expected_fields() {
        let json = serde_json::to_value(sample_scored_entry()).expect("serialize");
        assert!(json.get("id").is_some());
        assert!(json.get("title").is_some());
        assert!(json.get("subtitle").is_some());
        assert!(json.get("icon").is_some());
        assert!(json.get("score").is_some());
        assert!(json.get("titlePositions").is_some());
        assert!(json.get("subtitlePositions").is_some());
        assert!(json.get("actions").is_some());
    }

    #[test]
    fn serialization_field_values_match() {
        let json = serde_json::to_value(sample_scored_entry()).expect("serialize");
        assert_eq!(json["id"], "test-id");
        assert_eq!(json["title"], "Test Title");
        assert_eq!(json["subtitle"], "A subtitle");
        assert_eq!(json["score"], 42);
    }

    #[test]
    fn sourced_entry_serializes_flat() {
        let sourced = SourcedEntry::new("my-gadget".to_string(), sample_scored_entry());
        let json = serde_json::to_value(&sourced).expect("serialize");

        assert_eq!(json["source"], "my-gadget");
        assert_eq!(json["id"], "test-id");
        assert_eq!(json["title"], "Test Title");
        assert!(
            json.get("inner").is_none(),
            "flatten must not produce an 'inner' key"
        );
    }

    #[test]
    fn cmp_sort_key_score_descending() {
        let high = SourcedEntry::new("a".into(), ScoredEntry { score: 100, ..sample_scored_entry() });
        let low = SourcedEntry::new("a".into(), ScoredEntry { score: 50, ..sample_scored_entry() });
        assert!(high.cmp_sort_key(&low).is_lt(), "higher score sorts first");
    }

    #[test]
    fn cmp_sort_key_source_ascending_on_tie() {
        let a = SourcedEntry::new("alpha".into(), sample_scored_entry());
        let b = SourcedEntry::new("beta".into(), sample_scored_entry());
        assert!(a.cmp_sort_key(&b).is_lt(), "lower source sorts first on score tie");
    }

    #[test]
    fn cmp_sort_key_id_ascending_on_double_tie() {
        let mut e1 = sample_scored_entry();
        e1.id = "aaa".to_string();
        let mut e2 = sample_scored_entry();
        e2.id = "zzz".to_string();
        let a = SourcedEntry::new("same".into(), e1);
        let b = SourcedEntry::new("same".into(), e2);
        assert!(a.cmp_sort_key(&b).is_lt(), "lower id sorts first on source+score tie");
    }
}

#[cfg(test)]
mod result_source_tests {
    use super::ResultSource;

    #[test]
    fn gadget_variant_serializes_with_type_and_id_fields() {
        let json = serde_json::to_value(ResultSource::Gadget {
            id: "bangs".to_string(),
        })
        .expect("serialize gadget source");
        assert_eq!(json, serde_json::json!({ "type": "gadget", "id": "bangs" }));
    }

    #[test]
    fn catalog_variant_serializes_with_only_type_field() {
        let json = serde_json::to_value(ResultSource::Catalog).expect("serialize catalog source");
        assert_eq!(json, serde_json::json!({ "type": "catalog" }));
    }

    #[test]
    fn gadget_id_round_trips_special_characters() {
        let json = serde_json::to_value(ResultSource::Gadget {
            id: "my-weird.gadget-id".to_string(),
        })
        .expect("serialize weird id");
        assert_eq!(
            json["id"],
            serde_json::Value::String("my-weird.gadget-id".into())
        );
    }
}
