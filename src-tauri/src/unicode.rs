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
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    /// Convert grapheme-cluster indices to UTF-16 code unit offsets
    /// for correct highlighting on the JavaScript side.
    ///
    /// For ASCII-only text the indices are identical so we skip the
    /// conversion entirely.
    pub fn to_utf16(self, text: &str) -> Utf16Positions {
        let mut indices = self.0;
        if indices.is_empty() || text.is_ascii() {
            return Utf16Positions(indices);
        }

        // Build a lookup table: grapheme cluster index → starting
        // UTF-16 code unit offset. We walk the string's grapheme
        // clusters and accumulate the UTF-16 length of each one.
        let cluster_offsets: Vec<u32> = text
            .graphemes(true)
            .scan(0u32, |offset, grapheme| {
                let start = *offset;
                *offset += grapheme.encode_utf16().count() as u32;
                Some(start)
            })
            .collect();

        for pos in indices.iter_mut() {
            if let Some(&utf16_offset) = cluster_offsets.get(*pos as usize) {
                *pos = utf16_offset;
            }
        }

        Utf16Positions(indices)
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
        GraphemePositions(positions).to_utf16(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_indices_unchanged() {
        let positions = GraphemePositions(vec![0, 2, 4]);
        let result = positions.to_utf16("hello");
        assert_eq!(result.0, vec![0, 2, 4]);
    }

    #[test]
    fn single_emoji_prefix_shifts_indices() {
        // "🎨 Palette" — 🎨 is one grapheme but 2 UTF-16 code units
        let text = "🎨 Palette";
        // Nucleo would report P=2, a=3, l=4 (grapheme indices)
        let positions = GraphemePositions(vec![2, 3, 4]);
        let result = positions.to_utf16(text);
        // In UTF-16: 🎨=0,1  ' '=2  P=3  a=4  l=5
        assert_eq!(result.0, vec![3, 4, 5]);
    }

    #[test]
    fn flag_emoji_prefix() {
        // "🇺🇸 US" — flag is one grapheme but 4 UTF-16 code units
        let text = "🇺🇸 US";
        // Nucleo: ' '=1, U=2, S=3
        let positions = GraphemePositions(vec![2, 3]);
        let result = positions.to_utf16(text);
        // UTF-16: 🇺🇸=0,1,2,3  ' '=4  U=5  S=6
        assert_eq!(result.0, vec![5, 6]);
    }

    #[test]
    fn empty_positions_unchanged() {
        let positions = GraphemePositions(vec![]);
        let result = positions.to_utf16("🎨 test");
        assert_eq!(result.0, Vec::<u32>::new());
    }

    #[test]
    fn mixed_emoji_and_ascii() {
        // "a🎨b" — a=1 grapheme/1 utf16, 🎨=1 grapheme/2 utf16, b=1 grapheme/1 utf16
        let text = "a🎨b";
        // Nucleo: a=0, 🎨=1, b=2
        let positions = GraphemePositions(vec![0, 2]);
        let result = positions.to_utf16(text);
        // UTF-16: a=0, 🎨=1,2, b=3
        assert_eq!(result.0, vec![0, 3]);
    }
}
