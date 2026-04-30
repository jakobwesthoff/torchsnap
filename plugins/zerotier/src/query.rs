// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Query intent dispatch and fuzzy matching.
//!
//! The launcher invokes `search()` per keystroke. This module
//! covers two concerns:
//!
//! * **Intent detection.** `intent_for` classifies the query
//!   into `Match` (fuzzy match against known networks) or
//!   `JoinById` (the input is exactly 16 hex characters and
//!   maps to either an existing entry or a synthetic
//!   "Connect to network <id>" entry).
//!
//! * **Match scoring.** `match_networks` runs the launcher's
//!   nucleo matcher against names + ID prefixes and assembles
//!   `ScoredEntry`s with state-tier-aware base scores so
//!   connected networks rank above stored-only candidates.
//!
//! Score-tier rationale (referenced from `ranges:` comments
//! around the codebase): bangs sit at a fixed 1000, the
//! built-in URL plugin at 500. Connected ZeroTier entries
//! slot between (750), joined-offline near URL (450),
//! known-only below (250), synthetic Intent B at the floor
//! (125). Frecency adds up to ~3000 on top, so frequently
//! used entries rise to the top regardless of base tier.

use crate::api::{Network, NetworkStatus};

/// 16 hex characters, case-insensitive, no separators.
fn is_zt_network_id(s: &str) -> bool {
    s.len() == 16 && s.chars().all(|c| c.is_ascii_hexdigit())
}

#[derive(Debug, PartialEq, Eq)]
pub enum Intent {
    /// Empty query — produce no entries.
    None,
    /// Bare 16-hex-char input. Matched against history first;
    /// if absent, the entry-builder produces a synthetic
    /// "Connect to network <id>" entry.
    JoinById(String),
    /// Fuzzy-match against known networks by name and id
    /// prefix.
    Match(String),
}

pub fn intent_for(query: &str) -> Intent {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Intent::None;
    }
    if is_zt_network_id(trimmed) {
        return Intent::JoinById(trimmed.to_ascii_lowercase());
    }
    Intent::Match(trimmed.to_string())
}

/// Unified per-network row used by the launcher path. Built
/// from the join of live `GET /network` results and the
/// `networks` history table.
#[derive(Debug, Clone)]
pub struct NetworkRow {
    pub id: String,
    pub name: String,
    pub state: NetworkState,
    pub assigned_addresses: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkState {
    /// Live with `status == OK` — traffic flowing.
    Connected,
    /// Live with any other status. Carries the status so the
    /// launcher can pick a precise badge.
    JoinedOffline(NetworkStatus),
    /// In history only; not currently joined per the daemon.
    KnownOnly,
}

impl NetworkRow {
    pub fn from_live(net: &Network) -> Self {
        let state = match net.status {
            NetworkStatus::Ok => NetworkState::Connected,
            other => NetworkState::JoinedOffline(other),
        };
        Self {
            id: net.id.clone(),
            name: net.name.clone(),
            state,
            assigned_addresses: net.assigned_addresses.clone(),
        }
    }
}

/// Base score per state tier. Combined additively with the
/// per-match nucleo score.
pub fn base_score_for(state: NetworkState) -> u32 {
    match state {
        NetworkState::Connected => 750,
        NetworkState::JoinedOffline(_) => 450,
        NetworkState::KnownOnly => 250,
    }
}

/// Synthetic Intent B base score — slot below every state
/// tier so a real-network match for an exact-id input
/// outranks the synthetic.
pub const SYNTHETIC_CONNECT_SCORE: u32 = 125;

/// Stable entry id format: `network:<lowercase-id>`. Used by
/// the host's frecency layer to recognize repeated
/// selections of the same network across queries.
pub fn entry_id(network_id: &str) -> String {
    format!("network:{}", network_id.to_ascii_lowercase())
}

/// Parse a `network:<id>` entry id back into the bare network
/// id. Returns `None` for entry ids the action handler
/// shouldn't act on (synthetic, malformed).
pub fn parse_entry_id(entry_id: &str) -> Option<String> {
    entry_id.strip_prefix("network:").map(|s| s.to_string())
}

/// Match a query against the supplied rows and return one
/// scored entry per match. Rows that don't match drop out;
/// matches keep their network state in the score so callers
/// can sort by total without re-grouping.
///
/// Matching uses `nucleo_matcher::Pattern` with `Smart` case
/// and normalization — same configuration the host catalog
/// path uses, kept in lockstep so plugin and host scoring
/// behave identically.
pub fn match_networks(query: &str, rows: &[NetworkRow]) -> Vec<ScoredMatch> {
    use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
    use nucleo_matcher::{Config, Matcher, Utf32Str};

    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);

    let mut out = Vec::new();
    let mut name_buf: Vec<char> = Vec::new();
    let mut id_buf: Vec<char> = Vec::new();
    let mut indices_buf: Vec<u32> = Vec::new();

    for row in rows {
        name_buf.clear();
        name_buf.extend(row.name.chars());
        id_buf.clear();
        id_buf.extend(row.id.chars());
        indices_buf.clear();

        let name_haystack = Utf32Str::new(&row.name, &mut name_buf);
        let name_score = pattern.indices(name_haystack, &mut matcher, &mut indices_buf);

        let (nucleo_score, highlight_positions) = match name_score {
            Some(score) => (score, std::mem::take(&mut indices_buf)),
            None => {
                // Fall back to matching against the id (so
                // typing a partial id finds the network).
                indices_buf.clear();
                let id_haystack = Utf32Str::new(&row.id, &mut id_buf);
                match pattern.score(id_haystack, &mut matcher) {
                    Some(score) => (score, Vec::new()),
                    None => continue,
                }
            }
        };

        let total = base_score_for(row.state).saturating_add(nucleo_score);
        out.push(ScoredMatch {
            row: row.clone(),
            score: total,
            title_highlight_positions: highlight_positions,
        });
    }

    // Sort descending by total score so the launcher can
    // stable-sort across plugins later without re-ordering
    // the within-plugin order.
    out.sort_by(|a, b| b.score.cmp(&a.score));
    out
}

/// One result of [`match_networks`]. Carries the matched row,
/// the combined score, and the highlight positions on the
/// title (empty if the match was on the id fallback).
#[derive(Debug, Clone)]
pub struct ScoredMatch {
    pub row: NetworkRow,
    pub score: u32,
    pub title_highlight_positions: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live_row(id: &str, name: &str, state: NetworkState) -> NetworkRow {
        NetworkRow {
            id: id.into(),
            name: name.into(),
            state,
            assigned_addresses: vec![],
        }
    }

    // ---- intent_for -----------------------------------------

    #[test]
    fn empty_query_returns_none_intent() {
        assert_eq!(intent_for(""), Intent::None);
        assert_eq!(intent_for("   "), Intent::None);
    }

    #[test]
    fn bare_16_hex_chars_returns_join_by_id() {
        let intent = intent_for("AbCdEf0123456789");
        assert_eq!(intent, Intent::JoinById("abcdef0123456789".into()));
    }

    #[test]
    fn fifteen_hex_chars_falls_back_to_match() {
        // One char short — not a valid ZT network id.
        let intent = intent_for("abcdef012345678");
        assert_eq!(intent, Intent::Match("abcdef012345678".into()));
    }

    #[test]
    fn seventeen_hex_chars_falls_back_to_match() {
        let intent = intent_for("abcdef01234567890");
        assert_eq!(intent, Intent::Match("abcdef01234567890".into()));
    }

    #[test]
    fn non_hex_chars_fall_back_to_match() {
        let intent = intent_for("homenet");
        assert_eq!(intent, Intent::Match("homenet".into()));
    }

    #[test]
    fn whitespace_is_trimmed_before_intent_detection() {
        let intent = intent_for("  abcdef0123456789  ");
        assert_eq!(intent, Intent::JoinById("abcdef0123456789".into()));
    }

    // ---- entry_id round-trip --------------------------------

    #[test]
    fn entry_id_is_lowercase_prefixed() {
        assert_eq!(entry_id("ABCDEF0123456789"), "network:abcdef0123456789");
    }

    #[test]
    fn parse_entry_id_extracts_network_id() {
        assert_eq!(
            parse_entry_id("network:abcdef0123456789"),
            Some("abcdef0123456789".into())
        );
    }

    #[test]
    fn parse_entry_id_rejects_malformed() {
        assert_eq!(parse_entry_id("synthetic:foo"), None);
        assert_eq!(parse_entry_id("abcdef0123456789"), None);
    }

    // ---- match_networks -------------------------------------

    #[test]
    fn match_returns_empty_when_no_rows_match() {
        let rows = vec![live_row("1111111111111111", "homenet", NetworkState::Connected)];
        let matches = match_networks("zzz-no-overlap-zzz", &rows);
        assert!(matches.is_empty());
    }

    #[test]
    fn connected_ranks_above_joined_offline_for_same_query() {
        let rows = vec![
            live_row(
                "1111111111111111",
                "homenet",
                NetworkState::JoinedOffline(NetworkStatus::RequestingConfiguration),
            ),
            live_row("2222222222222222", "homenet", NetworkState::Connected),
        ];
        let matches = match_networks("homenet", &rows);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].row.id, "2222222222222222"); // Connected first
        assert!(matches[0].score > matches[1].score);
    }

    #[test]
    fn joined_offline_ranks_above_known_only() {
        let rows = vec![
            live_row("1111111111111111", "homenet", NetworkState::KnownOnly),
            live_row(
                "2222222222222222",
                "homenet",
                NetworkState::JoinedOffline(NetworkStatus::AccessDenied),
            ),
        ];
        let matches = match_networks("homenet", &rows);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].row.id, "2222222222222222");
    }

    #[test]
    fn id_prefix_match_falls_back_when_name_does_not_match() {
        let rows = vec![live_row(
            "abcdef0123456789",
            "homenet",
            NetworkState::Connected,
        )];
        // Query doesn't appear in the name, but is a prefix of the id.
        let matches = match_networks("abcdef", &rows);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].row.id, "abcdef0123456789");
        // Title highlights are empty when match falls back to id.
        assert!(matches[0].title_highlight_positions.is_empty());
    }

    #[test]
    fn name_match_carries_highlight_positions() {
        let rows = vec![live_row(
            "1111111111111111",
            "homenet",
            NetworkState::Connected,
        )];
        let matches = match_networks("hom", &rows);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].title_highlight_positions,
            vec![0, 1, 2],
            "expected highlight on the prefix",
        );
    }

    #[test]
    fn case_insensitive_name_match() {
        let rows = vec![live_row(
            "1111111111111111",
            "HomeNet",
            NetworkState::Connected,
        )];
        let matches = match_networks("homenet", &rows);
        assert_eq!(matches.len(), 1);
    }

    // ---- base_score tiers -----------------------------------

    #[test]
    fn base_scores_form_strict_descending_tiers() {
        let connected = base_score_for(NetworkState::Connected);
        let joined = base_score_for(NetworkState::JoinedOffline(NetworkStatus::Ok));
        let known = base_score_for(NetworkState::KnownOnly);
        assert!(connected > joined && joined > known);
        assert!(known > SYNTHETIC_CONNECT_SCORE);
    }
}
