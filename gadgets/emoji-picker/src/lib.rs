// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Emoji Picker Gadget (WASM)
//
// Activated by the ":" prefix. Searches emoji by shortcode
// and keyword with two-pass nucleo matching: shortcode hits
// rank above label/tag hits for the same query. Copies the
// selected emoji character to the system clipboard via the
// `clipboard::write-text` host import.
//
// Data is sourced from the `emojibase-data` npm package and
// embedded at compile time via `include_str!`. `build.rs`
// copies the JSON files from `frontend/node_modules/` (where
// `bun install` places them before Cargo runs) into `OUT_DIR`.
// Shortcodes are merged from both the GitHub and emojibase
// preset files for broader coverage.
//
// Frecency integration: the host records every selection
// automatically before dispatching `execute()` and applies
// score bonuses to `search()` results after they return —
// neither requires guest action. The guest only calls
// `frecency::top-items` directly to drive the empty-query
// browse mode, surfacing the user's most-used emoji when
// they open the picker with just ":".
// =========================================================

use std::cell::OnceCell;
use std::collections::HashMap;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use serde::Deserialize;
use torchsnap_gadget_sdk::prelude::*;

struct EmojiPickerPlugin;
define_gadget!(EmojiPickerPlugin);

// =========================================================
// Tunables
// =========================================================

/// Upper bound on entries returned for an empty query (just
/// ":" typed). Acts as a browse-preview ceiling.
const EMPTY_QUERY_LIMIT: usize = 10_000;

/// Minimum frecency history before the empty-query grid
/// prefers the frecency ordering over the default emojibase
/// browse order. Below this threshold the frecency list
/// feels awkwardly sparse, so we fall back to showing
/// everything instead.
const FRECENCY_MIN_ITEMS: usize = 2;

/// Score bonus added to shortcode-match hits so they rank
/// above keyword-only hits for the same query.
const SHORTCODE_SCORE_BONUS: u32 = 100;

// =========================================================
// Embedded data
//
// `build.rs` copies three JSON blobs from emojibase into
// OUT_DIR; `include_str!` then bakes them into the binary.
// =========================================================

const EMOJI_DATA: &str = include_str!(concat!(env!("OUT_DIR"), "/emoji-data.json"));
const SHORTCODES_GITHUB: &str =
    include_str!(concat!(env!("OUT_DIR"), "/emoji-shortcodes-github.json"));
const SHORTCODES_EMOJIBASE: &str =
    include_str!(concat!(env!("OUT_DIR"), "/emoji-shortcodes-emojibase.json"));

// =========================================================
// Emojibase JSON shapes
//
// Only the fields we actually consume are listed — serde
// silently drops `skins`, `version`, `hexcode`, `text`,
// `type`, `subgroup`, etc. Keeping the struct minimal means
// an emojibase-data upgrade that adds new fields won't
// break the build.
// =========================================================

#[derive(Deserialize)]
struct EmojibaseEntry {
    emoji: String,
    label: String,
    #[serde(default)]
    tags: Vec<String>,
    group: Option<u32>,
    order: Option<u32>,
}

/// The shortcode files map hexcode → shortcode, but a single
/// emoji can carry multiple shortcodes (e.g. both `grinning`
/// and `grinning_face`). The untagged enum accepts both the
/// scalar and array shapes the JSON uses.
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
// Processed catalogue row
//
// What we actually query against at search time — parsed
// once during `enable()` and kept in a thread-local
// `OnceCell`. WASM is single-threaded, so no locking is
// needed; `thread_local!` is just the ergonomic way to get
// interior mutability without hand-rolling `unsafe`.
// =========================================================

struct EmojiData {
    emoji: String,
    label: String,
    /// Shortcodes (without surrounding colons) merged from
    /// the GitHub and emojibase preset files. GitHub entries
    /// come first — their shortcodes are more recognisable
    /// to most users, so we prefer them as the display title.
    shortcodes: Vec<String>,
    tags: Vec<String>,
    group: Option<u32>,
    order: Option<u32>,
}

thread_local! {
    /// Populated by `enable()` before any search can land.
    /// `search()` accesses it through a shared borrow, so
    /// this cell is written exactly once.
    static ENTRIES: OnceCell<Vec<EmojiData>> = const { OnceCell::new() };
}

// =========================================================
// Parse + sort emojibase data
// =========================================================

fn parse_emoji_data() -> Vec<EmojiData> {
    let raw_entries: Vec<EmojibaseEntry> =
        serde_json::from_str(EMOJI_DATA).expect("parse embedded emoji data");

    // Shortcode files are keyed by hexcode (e.g. "1F680", "00A9").
    let github: HashMap<String, ShortcodeValue> =
        serde_json::from_str(SHORTCODES_GITHUB).expect("parse embedded GitHub shortcodes");
    let emojibase: HashMap<String, ShortcodeValue> =
        serde_json::from_str(SHORTCODES_EMOJIBASE).expect("parse embedded emojibase shortcodes");

    // Merge: GitHub first, then emojibase entries that aren't
    // already present for this hexcode. The resulting
    // ordering matters — `shortcodes.first()` becomes the
    // display title.
    let mut shortcode_map: HashMap<String, Vec<String>> = HashMap::new();
    for (hexcode, value) in github {
        shortcode_map
            .entry(hexcode)
            .or_default()
            .extend(value.into_vec());
    }
    for (hexcode, value) in emojibase {
        let existing = shortcode_map.entry(hexcode).or_default();
        for sc in value.into_vec() {
            if !existing.contains(&sc) {
                existing.push(sc);
            }
        }
    }

    let mut data: Vec<EmojiData> = raw_entries
        .into_iter()
        .map(|entry| {
            // Shortcode files spell each codepoint with at least
            // four hex digits (`00A9-FE0F`). Emojibase shortcode
            // files use the variation-selector-stripped form for
            // many entries, so we check both the full hexcode and
            // the stripped version before giving up.
            let hexcode = entry
                .emoji
                .chars()
                .map(|c| format!("{:04X}", c as u32))
                .collect::<Vec<_>>()
                .join("-");
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

    // Sort by (group, order) so the browse view opens on
    // smileys. Entries without a group (regional indicator
    // letters, skin-tone modifiers, …) sort last — they
    // rarely make sense as a "here are the emoji" view.
    data.sort_by_key(|e| match (e.group, e.order) {
        (Some(g), Some(o)) => (0, g, o),
        _ => (1, 0, 0),
    });
    data
}

// =========================================================
// Char-offset → UTF-16-offset conversion
//
// nucleo's `pattern.indices()` returns positions into the
// `Utf32Str` representation — i.e. char indices. The WIT
// `scored-entry` record's `title-highlight-positions` and
// `subtitle-highlight-positions` fields must be UTF-16
// offsets, because the frontend's `highlightText` helper
// indexes into JavaScript strings (which are UTF-16). For
// ASCII shortcodes this is a pass-through; non-BMP chars
// in labels (rare but possible) expand to surrogate pairs
// (`len_utf16() == 2`).
// =========================================================

fn char_positions_to_utf16(s: &str, char_positions: &[u32]) -> Vec<u32> {
    // Precompute a cumulative offset table so a subsequent
    // sparse lookup is O(1) per position.
    let mut table: Vec<u32> = Vec::with_capacity(s.chars().count() + 1);
    let mut offset: u32 = 0;
    table.push(0);
    for c in s.chars() {
        offset += c.len_utf16() as u32;
        table.push(offset);
    }
    char_positions
        .iter()
        .map(|&p| table.get(p as usize).copied().unwrap_or(offset))
        .collect()
}

// =========================================================
// Empty query (just ":" typed)
//
// When the user has at least FRECENCY_MIN_ITEMS recorded
// selections, show those in frecency order. Otherwise fall
// back to the full emojibase browse order. Entries without
// shortcodes are filtered out — they have no displayable
// title.
//
// Scores are returned as 0: the host applies frecency
// bonuses on top of whatever the guest returns, so the
// final ordering naturally matches the frecency ranking
// without any double-counting on our side.
// =========================================================

fn empty_query_results(entries: &[EmojiData]) -> Vec<ScoredEntry> {
    if frecency::is_enabled() {
        let top = frecency::top_items(EMPTY_QUERY_LIMIT as u32);
        if top.len() >= FRECENCY_MIN_ITEMS {
            // Emoji character → catalogue row so we can look
            // up metadata for each recorded item_id.
            let by_emoji: HashMap<&str, &EmojiData> =
                entries.iter().map(|e| (e.emoji.as_str(), e)).collect();
            return top
                .iter()
                .filter_map(|item| {
                    let entry = by_emoji.get(item.item_id.as_str())?;
                    if entry.shortcodes.is_empty() {
                        return None;
                    }
                    Some(build_scored_entry(entry, 0, 0, &[], &[]))
                })
                .collect();
        }
    }

    entries
        .iter()
        .filter(|e| !e.shortcodes.is_empty())
        .take(EMPTY_QUERY_LIMIT)
        .map(|e| build_scored_entry(e, 0, 0, &[], &[]))
        .collect()
}

// =========================================================
// Two-pass nucleo scoring for non-empty queries
// =========================================================

/// Intermediate match state while we're still iterating
/// over pass 1 / pass 2 — converted into a `ScoredEntry`
/// once both passes have settled on a single hit per
/// entry.
struct EmojiMatch {
    score: u32,
    /// Char-indexed positions into the chosen shortcode
    /// (pass 1 hits) or empty (pass 2 hits).
    title_positions: Vec<u32>,
    /// Char-indexed positions into the label (pass 2 hits)
    /// or empty (pass 1 hits).
    subtitle_positions: Vec<u32>,
    /// Which shortcode was used for the match — pass 1
    /// picks the best-scoring one, pass 2 defaults to 0 so
    /// the default display title (first shortcode) is used.
    shortcode_idx: usize,
}

fn search_entries(query: &str, entries: &[EmojiData]) -> Vec<ScoredEntry> {
    // `prefer_prefix` gives shortcodes that start with the
    // query (`hug` → `hugs`) a distance-weighted bonus,
    // keeping prefix hits above mid-word matches like
    // `shrug`.
    let mut config = Config::DEFAULT;
    config.prefer_prefix = true;
    let mut matcher = Matcher::new(config);
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut char_buf = Vec::new();
    let mut indices_buf = Vec::new();

    // Track which entries matched in pass 1 so pass 2 can
    // skip them. Using `HashMap` keyed by entry index keeps
    // us from paying for duplicate pass-2 matches.
    let mut matched: HashMap<usize, EmojiMatch> = HashMap::new();

    // ---------------------------------------------------------
    // Pass 1: match against each entry's shortcodes. A single
    // entry can carry several shortcodes (e.g. both `grinning`
    // and `grinning_face`); we pick the one with the highest
    // boosted score as the display title.
    // ---------------------------------------------------------
    for (idx, entry) in entries.iter().enumerate() {
        let mut best_score: Option<u32> = None;
        let mut best_positions: Vec<u32> = Vec::new();
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
                    title_positions: best_positions,
                    subtitle_positions: Vec::new(),
                    shortcode_idx: best_shortcode_idx,
                },
            );
        }
    }

    // ---------------------------------------------------------
    // Pass 2: match against `label + " " + tags` for entries
    // not already matched. Positions that land in the tag
    // region are discarded — the frontend only renders the
    // label, so highlighting those would point into
    // whitespace the user never sees.
    // ---------------------------------------------------------
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

            matched.insert(
                idx,
                EmojiMatch {
                    score,
                    title_positions: Vec::new(),
                    subtitle_positions,
                    shortcode_idx: 0,
                },
            );
        }
    }

    // Build the outgoing WIT records. Entries without a
    // shortcode can't render a meaningful title, so they're
    // dropped. Host-side code sorts by score after apply_scores.
    matched
        .into_iter()
        .filter_map(|(idx, m)| {
            let entry = &entries[idx];
            if entry.shortcodes.is_empty() {
                return None;
            }

            // Adjust pass-1 title positions by +1 to account
            // for the leading ":" we prepend to the displayed
            // shortcode. Pass-2 matches don't produce title
            // positions, so this is a no-op for them.
            let adjusted_title: Vec<u32> = m.title_positions.iter().map(|p| p + 1).collect();

            Some(build_scored_entry(
                entry,
                m.shortcode_idx,
                m.score,
                &adjusted_title,
                &m.subtitle_positions,
            ))
        })
        .collect()
}

// =========================================================
// Render one emoji as a WIT ScoredEntry
//
// `shortcode_idx` picks which shortcode becomes the display
// title — pass-1 matches pass the matching index, everything
// else (pass-2, empty-query, fallback) passes 0. Position
// slices are char-indexed into the pre-format title and
// subtitle strings; this function handles the UTF-16
// conversion right before packing the record.
// =========================================================

fn build_scored_entry(
    entry: &EmojiData,
    shortcode_idx: usize,
    score: u32,
    title_char_positions: &[u32],
    subtitle_char_positions: &[u32],
) -> ScoredEntry {
    let display_shortcode = entry
        .shortcodes
        .get(shortcode_idx)
        .or_else(|| entry.shortcodes.first())
        .map(String::as_str)
        .unwrap_or("");
    let title = format!(":{display_shortcode}:");
    let subtitle = entry.label.clone();

    ScoredEntry {
        id: entry.emoji.clone(),
        title_highlight_positions: char_positions_to_utf16(&title, title_char_positions),
        subtitle_highlight_positions: char_positions_to_utf16(&subtitle, subtitle_char_positions),
        title,
        subtitle: Some(subtitle),
        icon: Some(EntryIcon::Emoji(entry.emoji.clone())),
        score,
        // Keybindings for well-known `ActionId`s are filled
        // in by the host, not the guest.
        actions: vec![Action {
            id: ActionId::Copy,
            label: "Copy to Clipboard".into(),
        }],
        data: None,
    }
}

// =========================================================
// Lifecycle
// =========================================================

impl LifecycleGuest for EmojiPickerPlugin {
    fn enable() -> Result<(), String> {
        // Parse the emojibase catalogue lazily — the data
        // never changes within a process lifetime, so a
        // re-enable after a disable cycle keeps reusing the
        // already-populated cell.
        ENTRIES.with(|cell| {
            if cell.get().is_none() {
                let data = parse_emoji_data();
                let _ = cell.set(data);
            }
        });

        logging::log(logging::LogLevel::Info, "Emoji picker enabled", &[], None);
        Ok(())
    }

    fn disable() {
        logging::log(logging::LogLevel::Info, "Emoji picker disabled", &[], None);
    }

    fn on_setting_changed(_key: String, _value: String) {
        // No `[settings]` declared — this callback never fires.
    }
}

// =========================================================
// Search
// =========================================================

impl SearchGuest for EmojiPickerPlugin {
    fn entries() -> Vec<CatalogEntry> {
        // Prefix-only gadget: it contributes nothing to the
        // always-on catalog, so typing a plain word like
        // "grinning" won't surface emoji.
        Vec::new()
    }

    fn search(query: String, matched_prefix: Option<String>) -> SearchResponse {
        // Outside prefix mode we have nothing to contribute;
        // returning `Nothing` lets the host route the query
        // to other always-on gadgets.
        if matched_prefix.is_none() {
            return SearchResponse::Nothing;
        }

        ENTRIES.with(|cell| {
            let Some(entries) = cell.get() else {
                // `enable()` hasn't populated the cell yet —
                // render an empty custom view so the frontend
                // still mounts the grid scaffolding.
                return SearchResponse::CustomUi(ViewResponse {
                    view: "picker".into(),
                    data: None,
                    results: Vec::new(),
                });
            };

            let results = if query.is_empty() {
                empty_query_results(entries)
            } else {
                search_entries(&query, entries)
            };

            SearchResponse::CustomUi(ViewResponse {
                view: "picker".into(),
                data: None,
                results,
            })
        })
    }

    fn execute(entry: ScoredEntry, _action_id: ActionId) -> Result<PostAction, String> {
        // `entry.id` is the emoji character itself — we set
        // `ScoredEntry.id = entry.emoji` when building the
        // response. The host has already recorded the
        // frecency selection by the time this runs.
        clipboard::write_text(&entry.id).map_err(|e| format!("write emoji to clipboard: {e}"))?;
        Ok(PostAction::Dismiss)
    }
}

// Emoji picker is prefix-only with no frontend↔backend RPC
// and no scheduled tasks. The SDK noop macros provide the
// mandatory WIT exports; misrouted calls still surface in
// host logs as the macro stubs return an error.
impl_noop_messaging!(EmojiPickerPlugin);
impl_noop_tasks!(EmojiPickerPlugin);

#[cfg(test)]
mod tests {
    use super::*;

    fn shortcodes_for(data: &[EmojiData], label: &str) -> Vec<String> {
        data.iter()
            .find(|e| e.label == label)
            .unwrap_or_else(|| panic!("embedded data has an emoji labelled {label:?}"))
            .shortcodes
            .clone()
    }

    // Shortcode files key codepoints below U+1000 with leading
    // zeros (`00A9-FE0F`), so these are the emoji a hexcode
    // without padding cannot find.
    #[test]
    fn emoji_with_low_codepoints_get_their_shortcodes() {
        let data = parse_emoji_data();
        for label in [
            "copyright",
            "registered",
            "keycap: #",
            "keycap: *",
            "keycap: 0",
        ] {
            assert!(
                !shortcodes_for(&data, label).is_empty(),
                "{label} should have shortcodes"
            );
        }
    }

    // Entries without shortcodes are dropped from every result
    // list, so a grouped emoji without them is unreachable.
    #[test]
    fn every_grouped_emoji_has_shortcodes() {
        let data = parse_emoji_data();
        let unreachable: Vec<&str> = data
            .iter()
            .filter(|e| e.group.is_some() && e.shortcodes.is_empty())
            .map(|e| e.label.as_str())
            .collect();
        assert!(unreachable.is_empty(), "no shortcodes for {unreachable:?}");
    }
}
