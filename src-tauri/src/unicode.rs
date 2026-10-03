// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Grapheme ↔ UTF-16 Position Types
//
// Nucleo produces match indices in grapheme-cluster space (one
// index per user-perceived character). JavaScript indexes
// strings by UTF-16 code units (`text[i]`, `.length`). For
// ASCII the two are identical, but multi-codepoint grapheme
// clusters (emoji, flag sequences, combining marks) cause them
// to diverge.
//
// These newtypes make the distinction explicit at the type
// level: `ScoredEntry` only accepts `Utf16Positions`, forcing
// every call site to go through the conversion.
// =========================================================

use serde::Serialize;
use unicode_segmentation::UnicodeSegmentation;

/// Match positions in grapheme-cluster index space, as produced
/// by nucleo's `Pattern::indices`. Not serializable — must be
/// converted to [`Utf16Positions`] before use in `ScoredEntry`.
#[derive(Debug, Clone)]
pub struct GraphemePositions(pub Vec<u32>);

impl GraphemePositions {
    /// Idiomatic empty-value constructor, mirroring `Vec::new` /
    /// `String::new`. Kept as public API surface even without internal
    /// callers.
    #[allow(dead_code)]
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    /// Convert grapheme-cluster indices to UTF-16 code unit offsets
    /// for correct highlighting on the JavaScript side.
    ///
    /// Each matched grapheme expands to every code unit it spans, so
    /// the frontend never splits a surrogate pair or detaches a
    /// combining mark. Indices past the last grapheme are dropped.
    ///
    /// For ASCII-only text the indices are identical so we skip the
    /// conversion entirely.
    pub fn into_utf16(self, text: &str) -> Utf16Positions {
        let indices = self.0;
        if indices.is_empty() || text.is_ascii() {
            return Utf16Positions(indices);
        }

        // Build a lookup table: grapheme cluster index → UTF-16 code
        // unit range. We walk the string's grapheme clusters and
        // accumulate the UTF-16 length of each one.
        let cluster_ranges: Vec<std::ops::Range<u32>> = text
            .graphemes(true)
            .scan(0u32, |offset, grapheme| {
                let start = *offset;
                *offset += grapheme.encode_utf16().count() as u32;
                Some(start..*offset)
            })
            .collect();

        let utf16_positions = indices
            .iter()
            .filter_map(|&pos| cluster_ranges.get(pos as usize).cloned())
            .flatten()
            .collect();

        Utf16Positions(utf16_positions)
    }
}

/// Match positions in UTF-16 code unit space, matching JavaScript's
/// string indexing model. This is the only position type that
/// `ScoredEntry` accepts and that gets serialized across the Tauri
/// bridge.
#[derive(Debug, Clone, Serialize)]
pub struct Utf16Positions(pub Vec<u32>);

impl Utf16Positions {
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    /// Convenience constructor that converts grapheme-cluster indices
    /// to UTF-16 offsets in one step, without requiring an intermediate
    /// `GraphemePositions` binding at the call site.
    pub fn from_graphemes(positions: Vec<u32>, text: &str) -> Self {
        GraphemePositions(positions).into_utf16(text)
    }

    /// Compute UTF-16 highlight positions for occurrences of a
    /// substring within a text.
    ///
    /// When `all` is `false`, only the first occurrence is
    /// highlighted. When `true`, every non-overlapping occurrence
    /// is highlighted.
    ///
    /// Returns an empty `Utf16Positions` if the substring is not
    /// found.
    // Sibling constructor to `empty` and `from_graphemes` above, both of
    // which gadget_host and entry_store use for grapheme-index-based
    // highlighting; this substring-based form is exercised only by this
    // module's tests.
    #[allow(dead_code)]
    pub fn from_substring(text: &str, substring: &str, all: bool) -> Self {
        if substring.is_empty() {
            return Self::empty();
        }

        // Pre-compute the UTF-16 offset of every byte position that
        // falls on a char boundary. This lets us map any byte-based
        // match index to its UTF-16 offset with a single lookup.
        let byte_into_utf16: Vec<u32> = {
            let mut table = Vec::with_capacity(text.len() + 1);
            let mut utf16_offset = 0u32;
            for ch in text.chars() {
                // Every byte of this char maps to the same UTF-16 start.
                for _ in 0..ch.len_utf8() {
                    table.push(utf16_offset);
                }
                utf16_offset += ch.len_utf16() as u32;
            }
            // Sentinel for end-of-string.
            table.push(utf16_offset);
            table
        };

        let mut positions = Vec::new();
        let mut search_start = 0;

        while let Some(byte_start) = text[search_start..].find(substring) {
            let byte_start = search_start + byte_start;
            let byte_end = byte_start + substring.len();

            let utf16_start = byte_into_utf16[byte_start];
            let utf16_end = byte_into_utf16[byte_end];
            for pos in utf16_start..utf16_end {
                positions.push(pos);
            }

            if !all {
                break;
            }
            search_start = byte_end;
        }

        Self(positions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_indices_unchanged() {
        let positions = GraphemePositions(vec![0, 2, 4]);
        let result = positions.into_utf16("hello");
        assert_eq!(result.0, vec![0, 2, 4]);
    }

    #[test]
    fn single_emoji_prefix_shifts_indices() {
        // "🎨 Palette" — 🎨 is one grapheme but 2 UTF-16 code units
        let text = "🎨 Palette";
        // Nucleo would report P=2, a=3, l=4 (grapheme indices)
        let positions = GraphemePositions(vec![2, 3, 4]);
        let result = positions.into_utf16(text);
        // In UTF-16: 🎨=0,1  ' '=2  P=3  a=4  l=5
        assert_eq!(result.0, vec![3, 4, 5]);
    }

    #[test]
    fn flag_emoji_prefix() {
        // "🇺🇸 US" — flag is one grapheme but 4 UTF-16 code units
        let text = "🇺🇸 US";
        // Nucleo: ' '=1, U=2, S=3
        let positions = GraphemePositions(vec![2, 3]);
        let result = positions.into_utf16(text);
        // UTF-16: 🇺🇸=0,1,2,3  ' '=4  U=5  S=6
        assert_eq!(result.0, vec![5, 6]);
    }

    #[test]
    fn empty_positions_unchanged() {
        let positions = GraphemePositions(vec![]);
        let result = positions.into_utf16("🎨 test");
        assert_eq!(result.0, Vec::<u32>::new());
    }

    #[test]
    fn substring_first_occurrence() {
        let text = "Open 'rust async' in Google";
        let positions = Utf16Positions::from_substring(text, "Google", false);
        // "Google" starts at char/UTF-16 index 21
        assert_eq!(positions.0, vec![21, 22, 23, 24, 25, 26]);
    }

    #[test]
    fn substring_all_occurrences() {
        let text = "foo bar foo baz foo";
        let positions = Utf16Positions::from_substring(text, "foo", true);
        // "foo" at positions 0-2, 8-10, 16-18
        assert_eq!(positions.0, vec![0, 1, 2, 8, 9, 10, 16, 17, 18]);
    }

    #[test]
    fn substring_not_found() {
        let text = "Open Google";
        let positions = Utf16Positions::from_substring(text, "Yahoo", false);
        assert!(positions.0.is_empty());
    }

    #[test]
    fn substring_empty_needle() {
        let text = "Open Google";
        let positions = Utf16Positions::from_substring(text, "", false);
        assert!(positions.0.is_empty());
    }

    #[test]
    fn substring_with_emoji() {
        // 🎨 is 1 grapheme, 2 UTF-16 code units
        let text = "Open 🎨 Google";
        let positions = Utf16Positions::from_substring(text, "Google", false);
        // "Open " = 5 UTF-16 units, "🎨" = 2, " " = 1 → "Google" at 8
        assert_eq!(positions.0, vec![8, 9, 10, 11, 12, 13]);
    }

    #[test]
    fn substring_first_only_with_multiple() {
        let text = "foo bar foo";
        let positions = Utf16Positions::from_substring(text, "foo", false);
        assert_eq!(positions.0, vec![0, 1, 2]);
    }

    #[test]
    fn mixed_emoji_and_ascii() {
        // "a🎨b" — a=1 grapheme/1 utf16, 🎨=1 grapheme/2 utf16, b=1 grapheme/1 utf16
        let text = "a🎨b";
        // Nucleo: a=0, 🎨=1, b=2
        let positions = GraphemePositions(vec![0, 2]);
        let result = positions.into_utf16(text);
        // UTF-16: a=0, 🎨=1,2, b=3
        assert_eq!(result.0, vec![0, 3]);
    }

    // `highlightText` walks the title one UTF-16 code unit at a time, so
    // a matched grapheme must cover all its units. Otherwise the span
    // boundary falls between the halves of a surrogate pair and each half
    // renders as U+FFFD.

    #[test]
    fn matched_emoji_covers_both_surrogates() {
        let text = "a🎨b";
        let positions = GraphemePositions(vec![1]);
        let result = positions.into_utf16(text);
        assert_eq!(result.0, vec![1, 2]);
    }

    #[test]
    fn matched_flag_covers_all_four_code_units() {
        let text = "🇺🇸 US";
        let positions = GraphemePositions(vec![0, 2]);
        let result = positions.into_utf16(text);
        assert_eq!(result.0, vec![0, 1, 2, 3, 5]);
    }

    #[test]
    fn matched_combining_sequence_covers_the_mark() {
        // "e" followed by U+0301 COMBINING ACUTE ACCENT is one grapheme of
        // two BMP code points.
        let text = "cafe\u{0301}!";
        let positions = GraphemePositions(vec![3]);
        let result = positions.into_utf16(text);
        assert_eq!(result.0, vec![3, 4]);
    }

    #[test]
    fn out_of_range_grapheme_index_is_dropped() {
        let text = "a🎨b";
        let positions = GraphemePositions(vec![0, 3, 42]);
        let result = positions.into_utf16(text);
        assert_eq!(result.0, vec![0]);
    }
}
