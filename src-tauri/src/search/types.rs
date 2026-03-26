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

// =========================================================
// Post-Action Behavior
// =========================================================

/// What the launcher should do after executing a plugin action.
///
/// Returned by `CatalogPlugin::execute()` and `QueryPlugin::execute()`
/// to let the plugin control whether the launcher stays open.
/// Serialized to the frontend so it can act on the decision.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum PostAction {
    /// Hide the launcher (default for most actions).
    Dismiss,
    /// Keep the launcher open (e.g., for multi-select workflows).
    #[allow(dead_code)]
    KeepOpen,
}

// =========================================================
// Action Types
// =========================================================

/// Well-known action IDs with an escape hatch for custom plugin actions.
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
    /// Not currently constructed but available for future plugins.
    #[allow(dead_code)]
    DataUrl(String),
    /// Absolute filesystem path to a cached image file. The frontend
    /// converts this to an asset protocol URL via Tauri's
    /// `convertFileSrc` API.
    AssetIcon(String),
    /// A Unicode emoji character rendered as text in the icon slot.
    Emoji(String),
}

/// What a `QueryPlugin::search()` returns: either standard results
/// for the host to render in `ResultList`, or a custom UI request
/// where the plugin's frontend component takes over the result area.
///
/// The decision is per-query — a plugin may return `Results` for
/// some queries and `CustomUI` for others.
#[derive(Debug, Clone)]
pub enum SearchResponse {
    /// Standard result list — host renders via `ResultList`.
    Results(Vec<QueryResult>),
    /// Plugin requests custom UI. Results are still provided for the
    /// plugin component to use as props.
    CustomUI(Vec<QueryResult>),
}

impl SearchResponse {
    /// Extract the results regardless of variant.
    pub fn into_results(self) -> Vec<QueryResult> {
        match self {
            SearchResponse::Results(r) | SearchResponse::CustomUI(r) => r,
        }
    }

    /// Whether the plugin requested custom UI.
    pub fn is_custom_ui(&self) -> bool {
        matches!(self, SearchResponse::CustomUI(_))
    }
}

/// A pre-scored result returned by a `QueryPlugin`.
///
/// Same shape as `ScoredEntry` but without `source` — the registry
/// fills that from `plugin.id()` when converting to `ScoredEntry`.
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<EntryIcon>,
    pub score: u32,
    pub title_positions: Vec<u32>,
    pub subtitle_positions: Vec<u32>,
    pub actions: Vec<Action>,
}

impl QueryResult {
    /// Convert to a `ScoredEntry` by attaching the plugin's source ID.
    pub fn into_scored_entry(self, source: String) -> ScoredEntry {
        ScoredEntry {
            id: self.id,
            title: self.title,
            subtitle: self.subtitle,
            icon: self.icon,
            score: self.score,
            title_positions: self.title_positions,
            subtitle_positions: self.subtitle_positions,
            source,
            actions: self.actions,
        }
    }
}

/// A raw catalog entry before scoring. Internal to the Rust side —
/// plugins produce these, the catalog registry scores them, and
/// `ScoredEntry` is what crosses the bridge to the frontend.
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

/// A scored entry ready for the frontend. Contains the entry data
/// plus match metadata (score and highlight positions).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoredEntry {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<EntryIcon>,
    pub score: u32,
    /// Character positions in `title` that matched the query.
    /// Used by the frontend to highlight matched characters.
    // TODO: These are nucleo grapheme indices (1:1 with JS string
    // indices for ASCII, but need conversion for emoji/CJK).
    pub title_positions: Vec<u32>,
    /// Character positions in `subtitle` that matched.
    pub subtitle_positions: Vec<u32>,
    /// Which plugin produced this entry (plugin ID).
    pub source: String,
    pub actions: Vec<Action>,
}

// =========================================================
// Channel Messages
// =========================================================

/// Result of a registry search — entries plus routing metadata.
pub struct SearchResult {
    pub entries: Vec<ScoredEntry>,
    /// Plugin ID when a query plugin returned `SearchResponse::CustomUI`.
    pub custom_plugin_view: Option<String>,
    /// The prefix that triggered exclusive routing (e.g., `":"`).
    pub matched_prefix: Option<String>,
}

impl SearchResult {
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            custom_plugin_view: None,
            matched_prefix: None,
        }
    }
}

/// Messages streamed over a Tauri channel during a search.
///
/// The frontend receives these progressively: `CatalogResults`
/// arrives first (sub-millisecond for static catalogs), then
/// `Done` signals completion.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SearchMessage {
    /// The `rename_all` on the enum only renames variant tags, not
    /// fields within variants. Fields need explicit renaming.
    #[serde(rename_all = "camelCase")]
    CatalogResults {
        entries: Vec<ScoredEntry>,
        /// When a query plugin returned `SearchResponse::CustomUI`,
        /// this contains the plugin's ID so the frontend can mount
        /// the plugin's React component. `None` for standard list
        /// rendering.
        custom_plugin_view: Option<String>,
        /// The prefix that triggered exclusive routing. Sent to the
        /// frontend so the plugin component knows which prefix was
        /// matched. `None` when no prefix routing occurred.
        matched_prefix: Option<String>,
    },
    Done,
}
