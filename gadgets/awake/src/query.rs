// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Query grammar for the awake gadget.
//!
//! The gadget is keyword-matched in the launcher's global
//! search, so `search()` receives the raw query on every
//! keystroke and this module decides whether that query
//! addresses the awake gadget and, if so, what it requests.
//!
//! # Grammar
//!
//! The trimmed query is tokenized on ASCII whitespace and every
//! token is compared case-insensitively (lowercased).
//!
//! * **Keyword** — the first token. It must be a prefix of
//!   `awake` or `amphetamine` with a minimum length of three
//!   characters. So `awa`, `awak`, `awake` and `amp`…
//!   `amphetamine` address the gadget, while `aw`, `a`,
//!   `awakening` and `awoke` do not (the *token* must be a
//!   prefix of the keyword, never the other way around).
//!   Without a keyword the gadget produces no result at all.
//!
//! * **Arguments** — the remaining tokens in any order, each
//!   consumed at most once:
//!   - A duration, given at most once, in one of two forms. A
//!     compact token combines an optional `<digits>h` with an
//!     optional `<digits>m` (`30m`, `2h`, `1h30m`); at least one
//!     part is present and nothing else may appear in the token.
//!     Alternatively a bare number token is followed by a unit
//!     word (`45 min`, `2 hours`). Minute units are `m`, `min`,
//!     `mins`, `minute`, `minutes`; hour units are `h`, `hr`,
//!     `hrs`, `hour`, `hours`.
//!   - The `display` flag, the exact token `display`, given at
//!     most once. It permits the display to sleep while the
//!     system stays awake.
//!
//! Durations resolve to whole minutes (`u32`). A zero duration
//! (`0m`, `0 min`, `0h0m`) is invalid, and every numeric step
//! uses checked arithmetic so overflow is invalid too. A second
//! duration, a repeated `display`, or any leftover token that
//! fits none of the above makes the whole argument list invalid.
//!
//! # Result
//!
//! Parsing distinguishes four outcomes, captured by [`Query`]:
//! no keyword (the gadget stays out of the results), a bare
//! keyword (status/toggle surface), a valid start request, and a
//! keyword paired with arguments that do not parse (a hint
//! surface). Once the keyword matches, the gadget never falls
//! back to "no match": malformed arguments always land on the
//! hint outcome.

/// Outcome of classifying a launcher query against the awake
/// grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Query {
    /// The first token is not a keyword prefix. The gadget
    /// contributes nothing to the launcher results.
    NoMatch,
    /// A bare keyword with no arguments. Drives the status /
    /// toggle surface.
    Status,
    /// A keyword followed by a well-formed argument list.
    Start(StartRequest),
    /// A keyword followed by arguments that do not parse. The
    /// caller shows a syntax hint rather than dropping the
    /// gadget from the results.
    Hint,
}

/// A validated request to begin a keep-awake session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartRequest {
    /// Session length in whole minutes, or `None` for an
    /// indefinite session (for example `awake display`).
    pub(crate) minutes: Option<u32>,
    /// Whether the display may sleep while the system stays
    /// awake. Defaults to `false`; set by the `display` flag.
    pub(crate) display_sleep_allowed: bool,
}

// =========================================================
// Keyword recognition
// =========================================================

/// Keywords the gadget answers to. Any prefix of these (subject
/// to [`MIN_KEYWORD_LEN`]) addresses the gadget.
const KEYWORDS: [&str; 2] = ["awake", "amphetamine"];

/// Shortest keyword prefix that still addresses the gadget.
/// Prefixes below this length match too many unrelated queries
/// in a global keystroke search.
const MIN_KEYWORD_LEN: usize = 3;

/// Whether `token` addresses the gadget: at least
/// [`MIN_KEYWORD_LEN`] characters and a prefix of some keyword.
/// `token` is expected already lowercased.
fn is_keyword(token: &str) -> bool {
    token.len() >= MIN_KEYWORD_LEN && KEYWORDS.iter().any(|kw| kw.starts_with(token))
}

// =========================================================
// Duration tokens
// =========================================================

/// Unit half of the two-token duration form (`45 min`).
enum Unit {
    Minutes,
    Hours,
}

/// The unit word for the two-token duration form, or `None` when
/// the token is not a recognized unit. `token` is expected
/// already lowercased.
fn parse_unit(token: &str) -> Option<Unit> {
    match token {
        "m" | "min" | "mins" | "minute" | "minutes" => Some(Unit::Minutes),
        "h" | "hr" | "hrs" | "hour" | "hours" => Some(Unit::Hours),
        _ => None,
    }
}

/// A run of ASCII digits as a `u32`. Rejects empty input and
/// anything the standard parser would otherwise tolerate, such
/// as a leading `+`, so numeric fragments stay strictly numeric.
/// Values beyond `u32::MAX` return `None`.
fn parse_digits(token: &str) -> Option<u32> {
    if token.is_empty() || !token.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    token.parse::<u32>().ok()
}

/// Total minutes for a compact duration token, or `None` when
/// the token does not fit the `[<digits>h][<digits>m]` shape.
///
/// The token is scanned as an optional hours part terminated by
/// `h`, followed by an optional minutes part terminated by `m`,
/// with nothing left over. At least one part must be present.
/// Zero is a legal parse result here; the caller rejects it,
/// keeping the "is this a duration token" and "is this duration
/// usable" questions apart. Overflow of the hours-to-minutes
/// conversion returns `None`.
fn parse_compact_duration(token: &str) -> Option<u32> {
    let mut rest = token;
    let mut hours: Option<u32> = None;
    let mut minutes: Option<u32> = None;

    // Hours part: everything up to the first `h`, if any.
    if let Some(pos) = rest.find('h') {
        let (digits, after) = rest.split_at(pos);
        hours = Some(parse_digits(digits)?);
        rest = &after[1..];
    }

    // Minutes part: the remainder must end in `m` with digits
    // ahead of it, and `m` may appear only as that terminator.
    if !rest.is_empty() {
        let pos = rest.find('m')?;
        if pos != rest.len() - 1 {
            return None;
        }
        minutes = Some(parse_digits(&rest[..pos])?);
    }

    // Reject a token that carried neither part (for example a
    // bare `h`, which slips past both branches above).
    if hours.is_none() && minutes.is_none() {
        return None;
    }

    let total = hours
        .unwrap_or(0)
        .checked_mul(60)?
        .checked_add(minutes.unwrap_or(0))?;
    Some(total)
}

// =========================================================
// Top-level parse
// =========================================================

/// Classify a raw launcher query against the awake grammar.
pub(crate) fn parse(query: &str) -> Query {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|t| t.to_ascii_lowercase())
        .collect();

    // No first token means an empty or whitespace-only query,
    // which cannot carry a keyword.
    let Some(keyword) = tokens.first() else {
        return Query::NoMatch;
    };
    if !is_keyword(keyword) {
        return Query::NoMatch;
    }

    // A keyword on its own is the status / toggle surface.
    let args = &tokens[1..];
    if args.is_empty() {
        return Query::Status;
    }

    // From here the keyword has matched, so every failure below
    // resolves to `Hint`, never `NoMatch`.
    let mut minutes: Option<u32> = None;
    let mut display = false;

    let mut i = 0;
    while i < args.len() {
        let token = &args[i];

        // The `display` flag stands alone and appears once.
        if token == "display" {
            if display {
                return Query::Hint;
            }
            display = true;
            i += 1;
            continue;
        }

        // Compact duration (`30m`, `1h30m`), consuming one token.
        if let Some(total) = parse_compact_duration(token) {
            if minutes.is_some() || total == 0 {
                return Query::Hint;
            }
            minutes = Some(total);
            i += 1;
            continue;
        }

        // Two-token duration: a bare number followed by a unit
        // word (`45 min`). A number with no following unit, or a
        // following non-unit token, is invalid.
        if let Some(count) = parse_digits(token) {
            let unit = args.get(i + 1).and_then(|t| parse_unit(t));
            let Some(unit) = unit else {
                return Query::Hint;
            };
            let total = match unit {
                Unit::Minutes => Some(count),
                Unit::Hours => count.checked_mul(60),
            };
            let Some(total) = total else {
                return Query::Hint;
            };
            if minutes.is_some() || total == 0 {
                return Query::Hint;
            }
            minutes = Some(total);
            i += 2;
            continue;
        }

        // Anything else (a stray unit word, garbage, overflowing
        // numeric literal) invalidates the argument list.
        return Query::Hint;
    }

    Query::Start(StartRequest {
        minutes,
        display_sleep_allowed: display,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shorthand for asserting a valid start request.
    fn start(minutes: Option<u32>, display: bool) -> Query {
        Query::Start(StartRequest {
            minutes,
            display_sleep_allowed: display,
        })
    }

    // ---- keyword recognition --------------------------------

    #[test]
    fn keyword_prefixes_of_awake_match_from_length_three() {
        assert_eq!(parse("awa"), Query::Status);
        assert_eq!(parse("awak"), Query::Status);
        assert_eq!(parse("awake"), Query::Status);
    }

    #[test]
    fn keyword_prefixes_of_amphetamine_match_from_length_three() {
        assert_eq!(parse("amp"), Query::Status);
        assert_eq!(parse("amphet"), Query::Status);
        assert_eq!(parse("amphetamine"), Query::Status);
    }

    #[test]
    fn too_short_keyword_prefixes_do_not_match() {
        assert_eq!(parse("aw"), Query::NoMatch);
        assert_eq!(parse("a"), Query::NoMatch);
        assert_eq!(parse("am"), Query::NoMatch);
    }

    #[test]
    fn overshoot_tokens_are_not_prefixes_and_do_not_match() {
        // Longer than the keyword: the keyword is not a prefix of
        // the token, and the token is not a prefix of the keyword.
        assert_eq!(parse("awakening"), Query::NoMatch);
        assert_eq!(parse("awoke"), Query::NoMatch);
        assert_eq!(parse("amphetamines"), Query::NoMatch);
    }

    #[test]
    fn keyword_as_substring_of_first_token_does_not_match() {
        assert_eq!(parse("myawake"), Query::NoMatch);
        assert_eq!(parse("awakeish"), Query::NoMatch);
        assert_eq!(parse("stayawake"), Query::NoMatch);
    }

    #[test]
    fn keyword_matching_is_case_insensitive() {
        assert_eq!(parse("AWAKE"), Query::Status);
        assert_eq!(parse("Awake"), Query::Status);
        assert_eq!(parse("AmP"), Query::Status);
    }

    #[test]
    fn empty_and_whitespace_only_queries_do_not_match() {
        assert_eq!(parse(""), Query::NoMatch);
        assert_eq!(parse("   "), Query::NoMatch);
        assert_eq!(parse("\t \n"), Query::NoMatch);
    }

    // ---- bare keyword ---------------------------------------

    #[test]
    fn bare_keyword_with_surrounding_whitespace_is_status() {
        assert_eq!(parse("  awake  "), Query::Status);
    }

    // ---- compact durations ----------------------------------

    #[test]
    fn compact_minutes_duration() {
        assert_eq!(parse("awake 30m"), start(Some(30), false));
    }

    #[test]
    fn compact_hours_duration_converts_to_minutes() {
        assert_eq!(parse("awake 2h"), start(Some(120), false));
    }

    #[test]
    fn compact_combined_duration_sums_hours_and_minutes() {
        assert_eq!(parse("awake 1h30m"), start(Some(90), false));
    }

    #[test]
    fn compact_combined_duration_with_larger_values() {
        assert_eq!(parse("awake 3h45m"), start(Some(225), false));
    }

    // ---- two-token durations --------------------------------

    #[test]
    fn number_with_minute_unit_words() {
        assert_eq!(parse("awake 45 m"), start(Some(45), false));
        assert_eq!(parse("awake 45 min"), start(Some(45), false));
        assert_eq!(parse("awake 45 mins"), start(Some(45), false));
        assert_eq!(parse("awake 45 minute"), start(Some(45), false));
        assert_eq!(parse("awake 45 minutes"), start(Some(45), false));
    }

    #[test]
    fn number_with_hour_unit_words_converts_to_minutes() {
        assert_eq!(parse("awake 2 h"), start(Some(120), false));
        assert_eq!(parse("awake 2 hr"), start(Some(120), false));
        assert_eq!(parse("awake 2 hrs"), start(Some(120), false));
        assert_eq!(parse("awake 2 hour"), start(Some(120), false));
        assert_eq!(parse("awake 2 hours"), start(Some(120), false));
    }

    // ---- display flag ---------------------------------------

    #[test]
    fn display_flag_alone_is_indefinite_session() {
        assert_eq!(parse("awake display"), start(None, true));
    }

    #[test]
    fn display_flag_before_duration() {
        assert_eq!(parse("awake display 30m"), start(Some(30), true));
    }

    #[test]
    fn display_flag_after_duration() {
        assert_eq!(parse("awake 30m display"), start(Some(30), true));
    }

    #[test]
    fn display_flag_with_two_token_duration_in_either_order() {
        assert_eq!(parse("awake 2 hours display"), start(Some(120), true));
        assert_eq!(parse("awake display 2 hours"), start(Some(120), true));
    }

    // ---- zero durations -------------------------------------

    #[test]
    fn zero_compact_minutes_is_invalid() {
        assert_eq!(parse("awake 0m"), Query::Hint);
    }

    #[test]
    fn zero_compact_hours_is_invalid() {
        assert_eq!(parse("awake 0h"), Query::Hint);
    }

    #[test]
    fn zero_compact_combined_is_invalid() {
        assert_eq!(parse("awake 0h0m"), Query::Hint);
    }

    #[test]
    fn zero_two_token_duration_is_invalid() {
        assert_eq!(parse("awake 0 min"), Query::Hint);
        assert_eq!(parse("awake 0 hours"), Query::Hint);
    }

    // ---- overflow -------------------------------------------

    #[test]
    fn compact_minutes_beyond_u32_is_invalid() {
        // 4294967296 == u32::MAX + 1.
        assert_eq!(parse("awake 4294967296m"), Query::Hint);
    }

    #[test]
    fn compact_hours_overflowing_conversion_is_invalid() {
        assert_eq!(parse("awake 99999999999h"), Query::Hint);
    }

    #[test]
    fn two_token_hours_overflowing_conversion_is_invalid() {
        // Parses as u32 but overflows when multiplied by 60.
        assert_eq!(parse("awake 999999999 hours"), Query::Hint);
    }

    // ---- duplicate arguments --------------------------------

    #[test]
    fn duplicate_compact_duration_is_invalid() {
        assert_eq!(parse("awake 30m 2h"), Query::Hint);
    }

    #[test]
    fn duplicate_two_token_duration_is_invalid() {
        assert_eq!(parse("awake 30 min 2 hours"), Query::Hint);
    }

    #[test]
    fn mixed_duplicate_duration_forms_are_invalid() {
        assert_eq!(parse("awake 30m 2 hours"), Query::Hint);
    }

    #[test]
    fn duplicate_display_flag_is_invalid() {
        assert_eq!(parse("awake display display"), Query::Hint);
    }

    // ---- malformed argument lists ---------------------------

    #[test]
    fn number_without_unit_is_invalid() {
        assert_eq!(parse("awake 3"), Query::Hint);
    }

    #[test]
    fn number_followed_by_non_unit_token_is_invalid() {
        assert_eq!(parse("awake 3 display"), Query::Hint);
        assert_eq!(parse("awake 3 xyz"), Query::Hint);
    }

    #[test]
    fn unit_word_without_number_is_invalid() {
        assert_eq!(parse("awake min"), Query::Hint);
        assert_eq!(parse("awake h"), Query::Hint);
    }

    #[test]
    fn unrecognized_token_is_invalid() {
        assert_eq!(parse("awake xyz"), Query::Hint);
    }

    #[test]
    fn malformed_compact_token_is_invalid() {
        assert_eq!(parse("awake 1h30"), Query::Hint);
        assert_eq!(parse("awake 30mm"), Query::Hint);
        assert_eq!(parse("awake h30m"), Query::Hint);
        assert_eq!(parse("awake 1m30m"), Query::Hint);
    }

    #[test]
    fn signed_numeric_tokens_are_invalid() {
        // A leading sign is not accepted for durations.
        assert_eq!(parse("awake +5m"), Query::Hint);
        assert_eq!(parse("awake +5 min"), Query::Hint);
    }

    // ---- whitespace handling --------------------------------

    #[test]
    fn extra_whitespace_between_tokens_is_ignored() {
        assert_eq!(parse("awake   30m"), start(Some(30), false));
        assert_eq!(
            parse("  awake   45   min   display  "),
            start(Some(45), true)
        );
        assert_eq!(parse("awake\t2\thours"), start(Some(120), false));
    }

    // ---- hint never regresses to no-match -------------------

    #[test]
    fn matched_keyword_with_bad_args_never_returns_no_match() {
        // Every invalid-argument case stays on the keyword-matched
        // side of the fence.
        for query in ["awake 3", "awake xyz", "awake 30m 2h", "awake min"] {
            assert_eq!(parse(query), Query::Hint, "query: {query}");
        }
    }
}
