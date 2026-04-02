// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Emoji Picker Plugin
//
// Activated by the ":" prefix. Searches emoji by shortcode and
// keyword with two-pass nucleo matching: shortcode matches rank
// above keyword matches. Copies the selected emoji to the
// clipboard.
//
// Data is sourced from emojibase (npm package) and embedded at
// compile time via include_str!(). Shortcodes come from both
// the GitHub and emojibase preset files for broad coverage.
// =========================================================

use std::collections::HashMap;
use std::sync::RwLock;

use anyhow::Context;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use serde::Deserialize;
use tauri_plugin_clipboard_manager::ClipboardExt;

use super::{Plugin, PluginContext};
use crate::frecency::PluginFrecency;
use crate::search::types::{
    Action, ActionId, ActionKeybinding, CancellationToken, EntryIcon, PostAction, ResultChannel,
    ScoredEntry,
};
use crate::unicode::{GraphemePositions, Utf16Positions};

// =========================================================
// Emojibase Data Deserialization
// =========================================================

/// A single entry from the emojibase `en/data.json` full format.
///
/// We intentionally omit fields we don't need (skins, version,
/// hexcode, text, type, subgroup) — serde skips them silently.
#[derive(Deserialize)]
struct EmojibaseEntry {
    emoji: String,
    label: String,
    #[serde(default)]
    tags: Vec<String>,
    group: Option<u32>,
    order: Option<u32>,
}

/// Emojibase shortcode files map hexcode → string or array of strings.
#[derive(Deserialize)]
#[serde(untagged)]
enum ShortcodeValue {
    Single(String),
    Multiple(Vec<String>),
}

impl ShortcodeValue {
    fn into_vec(self) -> Vec<String> {
        match self {
            ShortcodeValue::Single(s) => vec![s],
            ShortcodeValue::Multiple(v) => v,
        }
    }
}

// =========================================================
// Internal Emoji Data
// =========================================================

/// Processed emoji entry ready for search.
struct EmojiData {
    /// The Unicode emoji character(s).
    emoji: String,
    /// Descriptive label (e.g., "grinning face").
    label: String,
    /// Shortcodes from github + emojibase presets (e.g., "rocket").
    /// Stored without the surrounding colons.
    shortcodes: Vec<String>,
    /// Search keywords/tags from emojibase.
    tags: Vec<String>,
    /// Unicode CLDR group (smileys=0, people=1, … flags=8).
    /// `None` for entries outside any group (e.g., regional indicators).
    group: Option<u32>,
    /// Sort key within the group for stable ordering.
    /// `None` for entries without a defined position.
    order: Option<u32>,
}

// =========================================================
// Embedded Data
// =========================================================

const EMOJI_DATA_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/emoji-data.json"));
const SHORTCODES_GITHUB_JSON: &str =
    include_str!(concat!(env!("OUT_DIR"), "/emoji-shortcodes-github.json"));
const SHORTCODES_EMOJIBASE_JSON: &str =
    include_str!(concat!(env!("OUT_DIR"), "/emoji-shortcodes-emojibase.json"));

/// Number of results to show when the query is empty (just ":"
/// typed). Acts as a browse preview.
const EMPTY_QUERY_LIMIT: usize = 10_000;

/// Minimum number of frecency items required before we show a
/// frecency-ordered list instead of the default browse order.
/// Below this threshold the frecency list feels arbitrarily
/// sparse, so we fall back to showing all emoji.
const FRECENCY_MIN_ITEMS: usize = 2;

/// Score bonus added to shortcode matches so they always rank
/// above keyword-only matches for the same query.
const SHORTCODE_SCORE_BONUS: u32 = 100;

// =========================================================
// Plugin Implementation
// =========================================================

pub struct EmojiPickerPlugin {
    entries: RwLock<Vec<EmojiData>>,
    frecency: RwLock<Option<PluginFrecency>>,
}

impl EmojiPickerPlugin {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
            frecency: RwLock::new(None),
        }
    }

    /// Parse the embedded emojibase JSON and merge shortcodes from
    /// both GitHub and emojibase preset files.
    fn parse_emoji_data() -> Vec<EmojiData> {
        let raw_entries: Vec<EmojibaseEntry> =
            serde_json::from_str(EMOJI_DATA_JSON).expect("parse embedded emoji data JSON");

        // Shortcode files are keyed by hexcode (e.g., "1F680").
        let github_shortcodes: HashMap<String, ShortcodeValue> =
            serde_json::from_str(SHORTCODES_GITHUB_JSON)
                .expect("parse embedded GitHub shortcodes JSON");
        let emojibase_shortcodes: HashMap<String, ShortcodeValue> =
            serde_json::from_str(SHORTCODES_EMOJIBASE_JSON)
                .expect("parse embedded emojibase shortcodes JSON");

        // Build a hexcode → merged shortcode list map. GitHub
        // shortcodes come first (more recognizable), then emojibase
        // ones that aren't duplicates.
        let mut shortcode_map: HashMap<String, Vec<String>> = HashMap::new();
        for (hexcode, value) in github_shortcodes {
            shortcode_map
                .entry(hexcode)
                .or_default()
                .extend(value.into_vec());
        }
        for (hexcode, value) in emojibase_shortcodes {
            let existing = shortcode_map.entry(hexcode).or_default();
            for sc in value.into_vec() {
                if !existing.contains(&sc) {
                    existing.push(sc);
                }
            }
        }

        // Convert the raw emojibase entries, then sort by (group, order)
        // so browse order matches the standard Unicode CLDR grouping
        // (smileys first, flags last). Without this, emojibase's raw
        // array order puts regional indicator letters at the top.
        let mut data: Vec<EmojiData> = raw_entries
            .into_iter()
            .map(|entry| {
                let hexcode = entry
                    .emoji
                    .chars()
                    .map(|c| format!("{:X}", c as u32))
                    // Join with hyphen for multi-codepoint sequences
                    // (e.g., flags, ZWJ sequences).
                    .collect::<Vec<_>>()
                    .join("-");

                // Strip variation selectors (FE0F) from the key to
                // match emojibase shortcode file conventions.
                let hexcode_stripped = hexcode.replace("-FE0F", "");

                let shortcodes = shortcode_map
                    .remove(&hexcode)
                    .or_else(|| shortcode_map.remove(&hexcode_stripped))
                    .unwrap_or_default();

                EmojiData {
                    emoji: entry.emoji,
                    label: entry.label,
                    shortcodes,
                    tags: entry.tags,
                    group: entry.group,
                    order: entry.order,
                }
            })
            .collect();

        // Entries without a group (e.g., regional indicator letters)
        // sort last. Within grouped entries, sort by (group, order).
        data.sort_by_key(|e| match (e.group, e.order) {
            (Some(g), Some(o)) => (0, g, o),
            _ => (1, 0, 0),
        });
        data
    }
}

impl EmojiPickerPlugin {
    /// Build the result list for an empty query (just ":" typed).
    ///
    /// If frecency tracking has enough history (>= FRECENCY_MIN_ITEMS),
    /// returns those items ordered by frecency score. Otherwise falls
    /// back to the full emojibase browse order so the grid isn't
    /// awkwardly sparse for new users.
    fn empty_scored_entries(&self, entries: &[EmojiData]) -> Vec<ScoredEntry> {
        let frecency = self.frecency.read().expect("emoji frecency read lock");

        if let Some(ref frec) = *frecency {
            let top = frec.top_items(EMPTY_QUERY_LIMIT);

            if top.len() >= FRECENCY_MIN_ITEMS {
                // Build a lookup from emoji char → EmojiData for the
                // frecency items. item_id is the emoji character.
                let entry_map: HashMap<&str, &EmojiData> =
                    entries.iter().map(|e| (e.emoji.as_str(), e)).collect();

                return top
                    .iter()
                    .filter_map(|item| {
                        let entry = entry_map.get(item.item_id.as_str())?;
                        if entry.shortcodes.is_empty() {
                            return None;
                        }
                        Some(emoji_to_scored_entry(
                            entry,
                            item.score,
                            GraphemePositions::empty(),
                        ))
                    })
                    .collect();
            }
        }

        // Fallback: show all emoji in default emojibase browse order.
        entries
            .iter()
            .filter(|e| !e.shortcodes.is_empty())
            .take(EMPTY_QUERY_LIMIT)
            .map(|e| emoji_to_scored_entry(e, 0, GraphemePositions::empty()))
            .collect()
    }
}

impl Plugin for EmojiPickerPlugin {
    fn id(&self) -> &str {
        "emoji-picker"
    }

    fn search_prefixes(&self) -> &[&str] {
        &[":"]
    }

    fn setup(&self, _app: &tauri::AppHandle, ctx: &PluginContext) {
        let data = Self::parse_emoji_data();
        let mut entries = self.entries.write().expect("emoji entries write lock");
        *entries = data;

        // Stash the frecency handle for empty-query ordering.
        let mut frecency = self.frecency.write().expect("emoji frecency write lock");
        *frecency = Some(ctx.frecency.clone());
    }

    fn search(
        &self,
        query: &str,
        matched_prefix: Option<&str>,
        results: &ResultChannel,
        _cancel: &CancellationToken,
    ) {
        // The emoji picker only operates in prefix mode. When called
        // without a prefix (no-prefix fan-out), contribute nothing.
        if matched_prefix.is_none() {
            return;
        }

        let entries = self.entries.read().expect("emoji entries read lock");

        if entries.is_empty() {
            // setup() hasn't completed yet.
            results.send_custom_ui("picker".into(), None, Vec::new());
            return;
        }

        // -------------------------------------------------------
        // Empty query (just ":" typed): show frecency-ordered
        // results if enough history exists, otherwise fall back
        // to the default emojibase browse order.
        // -------------------------------------------------------
        if query.is_empty() {
            results.send_custom_ui("picker".into(), None, self.empty_scored_entries(&entries));
            return;
        }

        // -------------------------------------------------------
        // Two-pass nucleo matching (ADR 0012 context):
        //   Pass 1: shortcodes (with score bonus)
        //   Pass 2: keywords/tags (entries not yet matched)
        // -------------------------------------------------------
        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        let mut char_buf = Vec::new();
        let mut indices_buf = Vec::new();

        struct EmojiMatch {
            score: u32,
            title_positions: GraphemePositions,
            subtitle_positions: GraphemePositions,
            /// Index into `EmojiData::shortcodes` for the best-matching
            /// shortcode (pass 1) or 0 for keyword matches (pass 2).
            shortcode_idx: usize,
        }

        // Track which entries matched in pass 1 so we skip them
        // in pass 2. Key: index into `entries`.
        let mut matched: HashMap<usize, EmojiMatch> = HashMap::new();

        // Pass 1: shortcode matching.
        for (idx, entry) in entries.iter().enumerate() {
            let mut best_score: Option<u32> = None;
            let mut best_positions = Vec::new();
            let mut best_shortcode_idx = 0;

            for (sc_idx, shortcode) in entry.shortcodes.iter().enumerate() {
                indices_buf.clear();
                let haystack = Utf32Str::new(shortcode, &mut char_buf);
                if let Some(score) = pattern.indices(haystack, &mut matcher, &mut indices_buf) {
                    let boosted = score + SHORTCODE_SCORE_BONUS;
                    if best_score.is_none_or(|s| boosted > s) {
                        best_score = Some(boosted);
                        best_positions = indices_buf.clone();
                        best_shortcode_idx = sc_idx;
                    }
                }
            }

            if let Some(score) = best_score {
                matched.insert(
                    idx,
                    EmojiMatch {
                        score,
                        title_positions: GraphemePositions(best_positions),
                        subtitle_positions: GraphemePositions::empty(),
                        shortcode_idx: best_shortcode_idx,
                    },
                );
            }
        }

        // Pass 2: keyword/tag matching for entries not matched in pass 1.
        // Matches against `label + " " + tags`. Positions that fall
        // within the label length become subtitle_positions; positions
        // in the tag portion are discarded (tags aren't displayed).
        for (idx, entry) in entries.iter().enumerate() {
            if matched.contains_key(&idx) {
                continue;
            }

            let combined = if entry.tags.is_empty() {
                entry.label.clone()
            } else {
                format!("{} {}", entry.label, entry.tags.join(" "))
            };

            indices_buf.clear();
            let haystack = Utf32Str::new(&combined, &mut char_buf);
            if let Some(score) = pattern.indices(haystack, &mut matcher, &mut indices_buf) {
                let label_char_len = entry.label.chars().count() as u32;
                let subtitle_positions: Vec<u32> = indices_buf
                    .iter()
                    .copied()
                    .filter(|&p| p < label_char_len)
                    .collect();

                // No title positions — pass 2 matched keywords, not
                // the shortcode displayed as title.
                matched.insert(
                    idx,
                    EmojiMatch {
                        score,
                        title_positions: GraphemePositions::empty(),
                        subtitle_positions: GraphemePositions(subtitle_positions),
                        shortcode_idx: 0,
                    },
                );
            }
        }

        // -------------------------------------------------------
        // Build results, sort by score descending.
        // -------------------------------------------------------
        let mut scored_results: Vec<ScoredEntry> = matched
            .into_iter()
            .filter_map(|(idx, m)| {
                let entry = &entries[idx];

                // Skip entries without shortcodes — they can't be
                // displayed meaningfully (no title to show).
                if entry.shortcodes.is_empty() {
                    return None;
                }

                // Use the best-matching shortcode for the title, or
                // fall back to the first one for keyword matches.
                let display_shortcode = &entry.shortcodes[m.shortcode_idx];

                // Adjust title positions to account for the ":" prefix
                // we add to the displayed shortcode.
                let adjusted_title_pos =
                    GraphemePositions(m.title_positions.0.iter().map(|p| p + 1).collect());

                let title = format!(":{display_shortcode}:");
                let subtitle = entry.label.clone();

                Some(ScoredEntry {
                    id: entry.emoji.clone(),
                    title_positions: adjusted_title_pos.to_utf16(&title),
                    subtitle_positions: m.subtitle_positions.to_utf16(&subtitle),
                    title,
                    subtitle: Some(subtitle),
                    icon: Some(EntryIcon::Emoji(entry.emoji.clone())),
                    score: m.score,
                    actions: vec![Action {
                        id: ActionId::Copy,
                        label: "Copy to Clipboard".into(),
                        keybinding: Some(ActionKeybinding {
                            modifiers: vec![],
                            key: "Enter".into(),
                        }),
                    }],
                })
            })
            .collect();

        // Apply frecency bonuses so frequently-used emoji float up.
        let frecency = self.frecency.read().expect("emoji frecency read lock");
        if let Some(ref frec) = *frecency {
            frec.apply_scores(&mut scored_results);
        }

        scored_results.sort_by(|a, b| b.score.cmp(&a.score));
        results.send_custom_ui("picker".into(), None, scored_results);
    }

    fn execute(
        &self,
        entry_id: &str,
        _action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        // entry_id is the emoji character itself.
        app.clipboard()
            .write_text(entry_id)
            .context("write emoji to clipboard")?;
        Ok(PostAction::Dismiss)
    }
}

/// Convert an `EmojiData` entry to a `ScoredEntry` for display.
fn emoji_to_scored_entry(
    entry: &EmojiData,
    score: u32,
    title_positions: GraphemePositions,
) -> ScoredEntry {
    let display_shortcode = entry.shortcodes.first().map(|s| s.as_str()).unwrap_or("");
    let title = format!(":{display_shortcode}:");

    ScoredEntry {
        id: entry.emoji.clone(),
        title_positions: title_positions.to_utf16(&title),
        subtitle_positions: Utf16Positions::empty(),
        title,
        subtitle: Some(entry.label.clone()),
        icon: Some(EntryIcon::Emoji(entry.emoji.clone())),
        score,
        actions: vec![Action {
            id: ActionId::Copy,
            label: "Copy to Clipboard".into(),
            keybinding: Some(ActionKeybinding {
                modifiers: vec![],
                key: "Enter".into(),
            }),
        }],
    }
}
