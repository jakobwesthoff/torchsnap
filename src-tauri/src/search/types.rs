// =========================================================
// Search Types
//
// Shared types for the search pipeline. CatalogEntry is
// internal (Rust only). Everything else is serialized across
// the Tauri bridge to the frontend.
// =========================================================

use serde::{Deserialize, Serialize};

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

/// Display-only keybinding info sent to the frontend for rendering
/// shortcut hints in the result footer and action palette.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionKeybinding {
    /// Human-readable label, e.g. "⏎" or "⌘⏎".
    pub label: String,
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
    DataUrl(String),
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

/// Messages streamed over a Tauri channel during a search.
///
/// The frontend receives these progressively: `CatalogResults`
/// arrives first (sub-millisecond for static catalogs), then
/// `Done` signals completion. `PluginResults` will be added
/// later for query-mode plugins that stream results.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SearchMessage {
    CatalogResults { entries: Vec<ScoredEntry> },
    // TODO: PluginResults { plugin_id: String, entries: Vec<ScoredEntry> }
    Done,
}
