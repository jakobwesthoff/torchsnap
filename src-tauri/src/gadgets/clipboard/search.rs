// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Clipboard Search Query Construction
//
// Translates a raw user search string into an FTS5 MATCH
// expression. User input must never reach FTS5 unescaped: the
// MATCH argument is a query *language*, not a literal, so bare
// input containing `-`, `.`, `/`, `(`, `"` or `:` is either a
// syntax error or silently means something other than what the
// user typed. Clipboard histories are full of URLs, file paths,
// UUIDs and code, so unescaped input fails on a large share of
// realistic searches.
// =========================================================

/// Build an FTS5 MATCH expression for a user-typed search string.
///
/// The input is split into alphanumeric tokens, each quoted as an
/// FTS5 string so no character in it can be read as query syntax.
/// Tokens are implicitly ANDed, and the final token carries a `*`
/// so that results appear while the word is still being typed.
///
/// Returns `None` when the input holds no alphanumeric characters
/// at all (`"---"`, `" "`), because FTS5 has no expression for
/// "match everything" — callers treat this as an unfiltered query
/// rather than as a search for nothing.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(build_fts_query("claude --resume").as_deref(), Some(r#""claude" "resume"*"#));
/// assert_eq!(build_fts_query("---"), None);
/// ```
pub fn build_fts_query(term: &str) -> Option<String> {
    let tokens: Vec<&str> = term
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();

    let (last, leading) = tokens.split_last()?;

    // Quoting is what neutralizes the query language. It also keeps a
    // token that happens to spell an FTS5 keyword (AND, OR, NOT, NEAR)
    // from being parsed as one, so "and" searches for the word "and".
    let mut query = String::with_capacity(term.len() + 3 * tokens.len());
    for token in leading {
        push_quoted(&mut query, token);
        query.push(' ');
    }
    push_quoted(&mut query, last);
    query.push('*');

    Some(query)
}

/// Append a token as an FTS5 quoted string.
///
/// Tokens are alphanumeric by construction, so they cannot contain
/// the `"` that would need doubling to escape.
fn push_quoted(query: &mut String, token: &str) {
    query.push('"');
    query.push_str(token);
    query.push('"');
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------
    // Token splitting
    // -----------------------------------------------------

    #[test]
    fn single_word_is_quoted_and_prefixed() {
        assert_eq!(build_fts_query("resume").as_deref(), Some(r#""resume"*"#));
    }

    #[test]
    fn words_are_anded_with_only_the_last_prefixed() {
        assert_eq!(
            build_fts_query("hello world").as_deref(),
            Some(r#""hello" "world"*"#)
        );
    }

    #[test]
    fn runs_of_separators_collapse() {
        assert_eq!(
            build_fts_query("a   ---   b").as_deref(),
            Some(r#""a" "b"*"#)
        );
    }

    #[test]
    fn leading_and_trailing_separators_are_dropped() {
        assert_eq!(
            build_fts_query("  --resume  ").as_deref(),
            Some(r#""resume"*"#)
        );
    }

    // -----------------------------------------------------
    // Inputs that previously produced FTS5 syntax errors
    // -----------------------------------------------------

    #[test]
    fn hyphenated_flag_becomes_plain_tokens() {
        assert_eq!(
            build_fts_query("claude --resume").as_deref(),
            Some(r#""claude" "resume"*"#)
        );
    }

    #[test]
    fn partially_typed_flag_drops_the_dangling_separator() {
        // Typed character by character, "claude -" must keep matching
        // rather than collapsing to an error the moment the dash lands.
        assert_eq!(build_fts_query("claude -").as_deref(), Some(r#""claude"*"#));
    }

    #[test]
    fn domain_splits_on_the_dot() {
        assert_eq!(
            build_fts_query("example.com").as_deref(),
            Some(r#""example" "com"*"#)
        );
    }

    #[test]
    fn url_path_splits_on_slashes_and_colon() {
        assert_eq!(
            build_fts_query("https://example.com/foo/bar").as_deref(),
            Some(r#""https" "example" "com" "foo" "bar"*"#)
        );
    }

    #[test]
    fn uuid_fragment_splits_on_hyphens() {
        assert_eq!(
            build_fts_query("979713a6-1365").as_deref(),
            Some(r#""979713a6" "1365"*"#)
        );
    }

    #[test]
    fn code_punctuation_is_stripped() {
        assert_eq!(
            build_fts_query("fn main() { let x = 1; }").as_deref(),
            Some(r#""fn" "main" "let" "x" "1"*"#)
        );
    }

    #[test]
    fn unbalanced_quote_is_stripped() {
        assert_eq!(
            build_fts_query(r#""quoted"#).as_deref(),
            Some(r#""quoted"*"#)
        );
    }

    #[test]
    fn embedded_quotes_are_stripped_as_separators() {
        assert_eq!(
            build_fts_query(r#"say "hi" now"#).as_deref(),
            Some(r#""say" "hi" "now"*"#)
        );
    }

    #[test]
    fn column_filter_syntax_is_neutralized() {
        // Bare `display_text:foo` would be an FTS5 column filter.
        assert_eq!(
            build_fts_query("display_text:foo").as_deref(),
            Some(r#""display" "text" "foo"*"#)
        );
    }

    // -----------------------------------------------------
    // FTS5 keywords
    // -----------------------------------------------------

    #[test]
    fn keywords_are_quoted_into_literal_terms() {
        for keyword in ["AND", "OR", "NOT", "NEAR"] {
            let query = build_fts_query(keyword).expect("keyword yields a token");
            assert_eq!(query, format!(r#""{keyword}"*"#));
        }
    }

    // -----------------------------------------------------
    // Empty and separator-only input
    // -----------------------------------------------------

    #[test]
    fn empty_input_has_no_query() {
        assert_eq!(build_fts_query(""), None);
    }

    #[test]
    fn whitespace_only_input_has_no_query() {
        assert_eq!(build_fts_query("   \t\n "), None);
    }

    #[test]
    fn separator_only_input_has_no_query() {
        assert_eq!(build_fts_query("---"), None);
        assert_eq!(build_fts_query("://"), None);
        assert_eq!(build_fts_query(r#""""#), None);
    }

    // -----------------------------------------------------
    // Unicode
    // -----------------------------------------------------

    #[test]
    fn diacritics_are_kept_in_the_token() {
        // unicode61 folds diacritics on both sides, so the token is
        // passed through verbatim and matching stays accent-insensitive.
        assert_eq!(build_fts_query("München").as_deref(), Some(r#""München"*"#));
    }

    #[test]
    fn cjk_runs_stay_a_single_token() {
        assert_eq!(build_fts_query("日本語").as_deref(), Some(r#""日本語"*"#));
    }

    #[test]
    fn emoji_are_separators() {
        assert_eq!(
            build_fts_query("hello 👋 world").as_deref(),
            Some(r#""hello" "world"*"#)
        );
    }
}
