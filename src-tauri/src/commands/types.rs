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
    /// Open the settings window on the executing gadget's section. Handled by the
    /// host; never forwarded to the frontend.
    OpenSettings,
    /// Open the developer tools window. Handled by the host; never forwarded to the frontend.
    ShowDevtools,
}

// =========================================================
// Action Types
//
// An entry's actions sit in fixed slots, and the slot alone
// decides how the user runs an action (ADR 55). Each action
// carries a command: a gadget-defined value the host stores
// with the entry and hands back to the gadget's `execute()`.
// `C` is the command type. The host holds commands as opaque
// `String`s; native gadgets use their own type through
// `gadgets::Search`, which encodes it to a `String`.
// =========================================================

/// The slots of an entry's actions, in the order the launcher
/// lists them. Serialized to the frontend, which maps each slot
/// to its key binding and default label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Slot {
    Primary,
    Secondary,
    Copy,
    Reveal,
    Delete,
    OpenSettings,
}

/// One action: what the launcher shows and what running it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action<C = String> {
    /// `None` shows the slot's default label in the frontend.
    pub label: Option<String>,
    pub command: C,
}

impl<C> Action<C> {
    /// An action with its own label.
    pub fn labeled(label: impl Into<String>, command: C) -> Self {
        Self {
            label: Some(label.into()),
            command,
        }
    }
}

/// The actions of an entry, one optional field per slot. The
/// builder covers the slots native gadgets fill; the others are
/// set through the public fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryActions<C = String> {
    pub primary: Option<Action<C>>,
    pub secondary: Option<Action<C>>,
    pub copy: Option<Action<C>>,
    pub reveal: Option<Action<C>>,
    pub delete: Option<Action<C>>,
    pub open_settings: Option<Action<C>>,
}

// Written out instead of derived: the derive would require
// `C: Default`, and an empty slot set needs no command at all.
impl<C> Default for EntryActions<C> {
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

impl<C> EntryActions<C> {
    /// No actions.
    pub fn new() -> Self {
        Self::default()
    }

    /// The Enter action. The frontend has no default label for it.
    pub fn primary(mut self, label: impl Into<String>, command: C) -> Self {
        self.primary = Some(Action::labeled(label, command));
        self
    }

    /// The Cmd+Enter action. The frontend has no default label for it.
    pub fn secondary(mut self, label: impl Into<String>, command: C) -> Self {
        self.secondary = Some(Action::labeled(label, command));
        self
    }

    /// The action in `slot`, if filled.
    pub fn get(&self, slot: Slot) -> Option<&Action<C>> {
        match slot {
            Slot::Primary => self.primary.as_ref(),
            Slot::Secondary => self.secondary.as_ref(),
            Slot::Copy => self.copy.as_ref(),
            Slot::Reveal => self.reveal.as_ref(),
            Slot::Delete => self.delete.as_ref(),
            Slot::OpenSettings => self.open_settings.as_ref(),
        }
    }

    /// The filled slots, in [`Slot`] order.
    pub fn iter(&self) -> impl Iterator<Item = (Slot, &Action<C>)> {
        [
            Slot::Primary,
            Slot::Secondary,
            Slot::Copy,
            Slot::Reveal,
            Slot::Delete,
            Slot::OpenSettings,
        ]
        .into_iter()
        .filter_map(|slot| self.get(slot).map(|action| (slot, action)))
    }

    /// Convert every command, failing on the first command that
    /// does not convert.
    pub fn try_map<D, E>(
        self,
        mut convert: impl FnMut(C) -> Result<D, E>,
    ) -> Result<EntryActions<D>, E> {
        let mut slot = |action: Option<Action<C>>| -> Result<Option<Action<D>>, E> {
            action
                .map(|action| {
                    Ok(Action {
                        label: action.label,
                        command: convert(action.command)?,
                    })
                })
                .transpose()
        };
        Ok(EntryActions {
            primary: slot(self.primary)?,
            secondary: slot(self.secondary)?,
            copy: slot(self.copy)?,
            reveal: slot(self.reveal)?,
            delete: slot(self.delete)?,
            open_settings: slot(self.open_settings)?,
        })
    }
}

/// What the frontend receives per action: the slot and the
/// gadget's label. Commands never leave the host.
#[derive(Serialize)]
struct SlotLabel<'a> {
    slot: Slot,
    label: Option<&'a str>,
}

/// Serialized as a list of `{ slot, label }` in slot order.
impl<C> Serialize for EntryActions<C> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter().map(|(slot, action)| SlotLabel {
            slot,
            label: action.label.as_deref(),
        }))
    }
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
    /// Platform-native application identifier (on macOS a bundle
    /// identifier). Rewritten to `AssetIcon` — or dropped — by the
    /// WASM response pass before entries reach the frontend.
    AppIcon(String),
}

/// A pre-scored result returned by a gadget's `search()` method.
///
/// Intentionally omits `source` — gadget authors should not set or
/// even think about this field. The host attaches it when wrapping
/// into `SourcedEntry` via `SourcedEntry::new`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoredEntry<C = String> {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<EntryIcon>,
    pub score: u32,
    pub title_positions: Utf16Positions,
    pub subtitle_positions: Utf16Positions,
    pub actions: EntryActions<C>,
}

impl<C> ScoredEntry<C> {
    /// Convert the commands of every action; see
    /// [`EntryActions::try_map`].
    pub fn try_map_commands<D, E>(
        self,
        convert: impl FnMut(C) -> Result<D, E>,
    ) -> Result<ScoredEntry<D>, E> {
        Ok(ScoredEntry {
            actions: self.actions.try_map(convert)?,
            id: self.id,
            title: self.title,
            subtitle: self.subtitle,
            icon: self.icon,
            score: self.score,
            title_positions: self.title_positions,
            subtitle_positions: self.subtitle_positions,
        })
    }
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
pub struct CatalogEntry<C = String> {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<EntryIcon>,
    /// Additional match targets beyond the title. These are used
    /// for scoring but their match positions are not highlighted.
    pub keywords: Vec<String>,
    pub actions: EntryActions<C>,
}

impl<C> CatalogEntry<C> {
    /// Convert the commands of every action; see
    /// [`EntryActions::try_map`].
    pub fn try_map_commands<D, E>(
        self,
        convert: impl FnMut(C) -> Result<D, E>,
    ) -> Result<CatalogEntry<D>, E> {
        Ok(CatalogEntry {
            actions: self.actions.try_map(convert)?,
            id: self.id,
            title: self.title,
            subtitle: self.subtitle,
            icon: self.icon,
            keywords: self.keywords,
        })
    }
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

/// Return type of a gadget's `search()`. Not serialized: it only
/// passes between gadget and host within the same process.
#[derive(Debug, Clone)]
pub enum GadgetResponse<C = String> {
    /// Standard result list entries.
    Results(Vec<ScoredEntry<C>>),
    /// Gadget requests full custom UI (replaces the result list).
    CustomUI {
        view: String,
        data: Option<serde_json::Value>,
        results: Vec<ScoredEntry<C>>,
    },
    /// Gadget requests inline UI (rendered above the result list).
    InlineUI {
        view: String,
        data: Option<serde_json::Value>,
        results: Vec<ScoredEntry<C>>,
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
            actions: EntryActions {
                copy: Some(Action {
                    label: None,
                    command: "secret-copy".to_string(),
                }),
                ..EntryActions::new().primary("Open", "secret-open".to_string())
            },
        }
    }

    #[test]
    fn actions_serialize_as_slot_and_label_without_commands() {
        let json = serde_json::to_value(sample_scored_entry()).expect("serialize");
        assert_eq!(
            json["actions"],
            serde_json::json!([
                { "slot": "primary", "label": "Open" },
                { "slot": "copy", "label": null },
            ])
        );
        assert!(
            !json.to_string().contains("secret"),
            "commands must never reach the frontend: {json}"
        );
    }

    #[test]
    fn commands_do_not_leak_through_sourced_entry_flatten() {
        let sourced = SourcedEntry::new("gadget".to_string(), sample_scored_entry());
        let json = serde_json::to_value(&sourced).expect("serialize");
        assert!(!json.to_string().contains("secret"), "{json}");
    }

    #[test]
    fn slots_serialize_in_camel_case() {
        let json = serde_json::to_value([Slot::Primary, Slot::OpenSettings]).expect("serialize");
        assert_eq!(json, serde_json::json!(["primary", "openSettings"]));
        let slot: Slot = serde_json::from_str("\"openSettings\"").expect("deserialize");
        assert_eq!(slot, Slot::OpenSettings);
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
        let high = SourcedEntry::new(
            "a".into(),
            ScoredEntry {
                score: 100,
                ..sample_scored_entry()
            },
        );
        let low = SourcedEntry::new(
            "a".into(),
            ScoredEntry {
                score: 50,
                ..sample_scored_entry()
            },
        );
        assert!(high.cmp_sort_key(&low).is_lt(), "higher score sorts first");
    }

    #[test]
    fn cmp_sort_key_source_ascending_on_tie() {
        let a = SourcedEntry::new("alpha".into(), sample_scored_entry());
        let b = SourcedEntry::new("beta".into(), sample_scored_entry());
        assert!(
            a.cmp_sort_key(&b).is_lt(),
            "lower source sorts first on score tie"
        );
    }

    #[test]
    fn cmp_sort_key_id_ascending_on_double_tie() {
        let mut e1 = sample_scored_entry();
        e1.id = "aaa".to_string();
        let mut e2 = sample_scored_entry();
        e2.id = "zzz".to_string();
        let a = SourcedEntry::new("same".into(), e1);
        let b = SourcedEntry::new("same".into(), e2);
        assert!(
            a.cmp_sort_key(&b).is_lt(),
            "lower id sorts first on source+score tie"
        );
    }
}

#[cfg(test)]
mod entry_actions_tests {
    use super::*;

    fn unlabeled(command: u32) -> Option<Action<u32>> {
        Some(Action {
            label: None,
            command,
        })
    }

    fn all_slots() -> EntryActions<u32> {
        EntryActions {
            copy: unlabeled(3),
            reveal: unlabeled(4),
            delete: Some(Action::labeled("Forget", 5)),
            open_settings: unlabeled(6),
            ..EntryActions::new().secondary("Leave", 2).primary("Join", 1)
        }
    }

    #[test]
    fn empty_actions_have_no_slots() {
        let actions = EntryActions::<u32>::new();
        assert_eq!(actions.iter().count(), 0);
        assert!(actions.get(Slot::Primary).is_none());
    }

    #[test]
    fn get_returns_the_action_in_each_slot() {
        let actions = all_slots();
        for (slot, command) in [
            (Slot::Primary, 1),
            (Slot::Secondary, 2),
            (Slot::Copy, 3),
            (Slot::Reveal, 4),
            (Slot::Delete, 5),
            (Slot::OpenSettings, 6),
        ] {
            assert_eq!(
                actions.get(slot).map(|a| a.command),
                Some(command),
                "{slot:?}"
            );
        }
    }

    #[test]
    fn iter_lists_filled_slots_in_slot_order() {
        let actions = EntryActions {
            delete: unlabeled(5),
            ..EntryActions::new().primary("Join", 1)
        };
        let slots: Vec<Slot> = actions.iter().map(|(slot, _)| slot).collect();
        assert_eq!(slots, [Slot::Primary, Slot::Delete]);
    }

    #[test]
    fn builder_labels_primary_and_secondary() {
        let actions = all_slots();
        assert_eq!(actions.primary, Some(Action::labeled("Join", 1)));
        assert_eq!(actions.secondary, Some(Action::labeled("Leave", 2)));
        assert_eq!(actions.copy, unlabeled(3));
    }

    #[test]
    fn try_map_converts_every_command_and_keeps_labels() {
        let mapped = all_slots()
            .try_map(|n| Ok::<_, ()>(n.to_string()))
            .expect("all convert");
        assert_eq!(
            mapped.delete,
            Some(Action::labeled("Forget", "5".to_string()))
        );
        assert_eq!(mapped.iter().count(), 6);
    }

    #[test]
    fn try_map_fails_when_one_command_fails() {
        let result = all_slots().try_map(|n| if n == 4 { Err("bad") } else { Ok(n) });
        assert_eq!(result, Err("bad"));
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
