// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Permission Variable Substitution
//
// Single source of truth for the `${...}` substitution
// variables that appear in `[[permissions.command]]` rules
// and in `paths::resolve` calls.
//
// Three consumers share this module:
//
// 1. Manifest validation (`validate_variable_references`)
//    runs at parse time. It does not have a `PathContext`
//    yet — the actual paths are not known until gadget
//    enable. It only checks that every `${...}` token uses
//    a recognized variable name.
//
// 2. Argv-matcher rule compilation (`substitute_variables`
//    via `compile_rule`) runs at gadget enable. By then
//    the host has constructed a `PathContext` and can
//    resolve every variable to a real absolute path.
//
// 3. The `paths::resolve` host import lets gadgets resolve
//    the same template syntax at runtime, so a manifest
//    declaration like `path-under = "${gadget-archive}/foo"`
//    has a 1:1 gadget-side equivalent
//    `paths::resolve("${gadget-archive}/foo")`.
//
// Keeping the recognized list, the parser, and the
// substituter in one module means manifest validation and
// runtime resolution cannot drift apart.
// =========================================================

use std::path::PathBuf;

use thiserror::Error;

/// Substitution variables recognized in
/// `[[permissions.command]]` rule fields and in
/// `paths::resolve` calls. Any `${...}` token whose name
/// does not appear here is rejected at the appropriate
/// stage (manifest parse or runtime resolve).
pub const RECOGNIZED_PERMISSION_VARIABLES: &[&str] = &[
    "gadget-data",
    "gadget-archive",
    "home",
    "xdg-config",
    "xdg-data",
];

/// Resolved values for every recognized substitution
/// variable. Constructed once per gadget instance at
/// bridge-construction time.
#[derive(Debug, Clone)]
pub struct PathContext {
    pub gadget_data: PathBuf,
    pub gadget_archive: PathBuf,
    pub home: PathBuf,
    pub xdg_config: PathBuf,
    pub xdg_data: PathBuf,
}

impl PathContext {
    /// Look up the resolved absolute path for a recognized
    /// variable name. Returns `None` for any name not in
    /// [`RECOGNIZED_PERMISSION_VARIABLES`].
    pub fn lookup(&self, name: &str) -> Option<&std::path::Path> {
        match name {
            "gadget-data" => Some(self.gadget_data.as_path()),
            "gadget-archive" => Some(self.gadget_archive.as_path()),
            "home" => Some(self.home.as_path()),
            "xdg-config" => Some(self.xdg_config.as_path()),
            "xdg-data" => Some(self.xdg_data.as_path()),
            _ => None,
        }
    }
}

/// Outcome of a runtime `paths::resolve` call. Distinct
/// from the parse-time `validate_variable_references`
/// errors (which surface as `anyhow::Error` strings) — the
/// runtime variant is a typed result so the WIT host
/// import can produce a structured error variant for the
/// guest to switch on.
#[derive(Debug, Error)]
pub enum ResolveError {
    /// A `${name}` token's `name` is not in
    /// [`RECOGNIZED_PERMISSION_VARIABLES`]. The string is
    /// the offending name.
    #[error("unknown variable `${{{0}}}`")]
    UnknownVariable(String),

    /// A `${` was not closed by `}` before the end of the
    /// template. The string is the partial token (everything
    /// after the `${`) so callers can surface it in a
    /// diagnostic.
    #[error("unterminated `${{...}}` reference at `{0}`")]
    Unterminated(String),
}

/// Parse-time check: every `${name}` token in `s` must use
/// a name from [`RECOGNIZED_PERMISSION_VARIABLES`]. The
/// recognized list is closed at parse time, so a typo in a
/// variable name fails gadget load rather than silently
/// matching nothing at runtime.
///
/// `field` and `rule_index` are diagnostic context for the
/// returned error.
pub fn validate_variable_references(s: &str, field: &str, rule_index: usize) -> anyhow::Result<()> {
    for token in iter_variable_tokens(s) {
        match token {
            Ok(Token::Variable(name)) => {
                if !RECOGNIZED_PERMISSION_VARIABLES.contains(&name) {
                    anyhow::bail!(
                        "rule {rule_index} {field}: unknown variable `${{{name}}}` in `{s}`; \
                         recognized variables are {RECOGNIZED_PERMISSION_VARIABLES:?}"
                    );
                }
            }
            Ok(Token::Literal(_)) => {}
            Err(TokenizeError::Unterminated(rest)) => {
                anyhow::bail!(
                    "rule {rule_index} {field}: unterminated `${{...}}` reference at `{rest}` in `{s}`"
                );
            }
        }
    }
    Ok(())
}

/// Runtime substitution: replace every `${name}` token in
/// `template` with the corresponding resolved path from
/// `ctx`. Unknown variable names produce
/// [`ResolveError::UnknownVariable`]; unterminated `${`
/// references produce [`ResolveError::Unterminated`].
///
/// The output is a plain `String`. Callers that need a
/// `Path` parse the result on their end — keeping the
/// API string-based avoids an asymmetric "this returns
/// `PathBuf` but mid-template substitution would have
/// produced bytes that aren't a path" footgun.
pub fn substitute_variables(template: &str, ctx: &PathContext) -> Result<String, ResolveError> {
    let mut result = String::with_capacity(template.len());
    for token in iter_variable_tokens(template) {
        match token.map_err(|e| match e {
            TokenizeError::Unterminated(rest) => ResolveError::Unterminated(rest),
        })? {
            Token::Literal(s) => result.push_str(s),
            Token::Variable(name) => {
                let Some(path) = ctx.lookup(name) else {
                    return Err(ResolveError::UnknownVariable(name.to_string()));
                };
                let path_str = path.to_string_lossy();
                result.push_str(&path_str);
            }
        }
    }
    Ok(result)
}

// =========================================================
// Tokenizer (private)
//
// Single tokenization pass shared by `validate_variable_
// references` (which only inspects variable names) and
// `substitute_variables` (which interleaves literals and
// resolved values into the output).
//
// Handles only `${name}` tokens; `$` followed by anything
// other than `{` is a plain literal. There is no escape
// syntax — manifests rarely need a literal `${...}` and
// adding escapes would invite the usual quoting bugs.
// =========================================================

#[derive(Debug)]
enum Token<'a> {
    Literal(&'a str),
    Variable(&'a str),
}

#[derive(Debug)]
enum TokenizeError {
    Unterminated(String),
}

fn iter_variable_tokens(s: &str) -> impl Iterator<Item = Result<Token<'_>, TokenizeError>> + '_ {
    let bytes = s.as_bytes();
    let mut cursor = 0;

    std::iter::from_fn(move || {
        if cursor >= bytes.len() {
            return None;
        }

        // Look for the next `${` opening; emit anything
        // before it as a literal token.
        let mut scan = cursor;
        while scan < bytes.len() {
            if bytes[scan] == b'$' && scan + 1 < bytes.len() && bytes[scan + 1] == b'{' {
                if scan > cursor {
                    let literal = &s[cursor..scan];
                    cursor = scan;
                    return Some(Ok(Token::Literal(literal)));
                }
                // Sitting on `${` — parse the variable.
                let name_start = scan + 2;
                let Some(end_offset) = bytes[name_start..].iter().position(|&b| b == b'}') else {
                    let rest = s[scan..].to_string();
                    cursor = bytes.len();
                    return Some(Err(TokenizeError::Unterminated(rest)));
                };
                let name_end = name_start + end_offset;
                let name = &s[name_start..name_end];
                cursor = name_end + 1;
                return Some(Ok(Token::Variable(name)));
            }
            scan += 1;
        }

        // Trailing literal (no more `${...}` ahead).
        let literal = &s[cursor..];
        cursor = bytes.len();
        Some(Ok(Token::Literal(literal)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ctx() -> PathContext {
        PathContext {
            gadget_data: PathBuf::from("/data/plug"),
            gadget_archive: PathBuf::from("/archive/plug"),
            home: PathBuf::from("/home/jake"),
            xdg_config: PathBuf::from("/home/jake/.config"),
            xdg_data: PathBuf::from("/home/jake/.local/share"),
        }
    }

    // ----- validate_variable_references -----

    #[test]
    fn validate_accepts_recognized_names() {
        for name in RECOGNIZED_PERMISSION_VARIABLES {
            let s = format!("prefix-${{{name}}}-suffix");
            validate_variable_references(&s, "test", 0)
                .unwrap_or_else(|e| panic!("`{name}` should be accepted: {e}"));
        }
    }

    #[test]
    fn validate_accepts_no_variables() {
        validate_variable_references("plain string with no template", "test", 0)
            .expect("plain text is fine");
    }

    #[test]
    fn validate_accepts_dollar_without_brace() {
        // `$5` and `$NAME` (no braces) are plain text.
        validate_variable_references("price $5 USD", "test", 0).expect("plain text");
        validate_variable_references("ENV=$NAME", "test", 0).expect("plain text");
    }

    #[test]
    fn validate_rejects_unknown_variable() {
        let err = validate_variable_references("${gadget-typo}/foo", "test", 0).unwrap_err();
        assert!(err.to_string().contains("gadget-typo"));
    }

    #[test]
    fn validate_rejects_first_unknown_variable_in_chain() {
        // First unrecognized name should be the one named in
        // the error — confirms we're tokenizing left-to-right.
        let err = validate_variable_references("${gadget-data}/${unknown}/${home}", "test", 0)
            .unwrap_err();
        assert!(err.to_string().contains("unknown"));
    }

    // ----- substitute_variables -----

    #[test]
    fn substitute_replaces_single_variable() {
        let resolved = substitute_variables("${gadget-data}", &ctx()).expect("should substitute");
        assert_eq!(resolved, "/data/plug");
    }

    #[test]
    fn substitute_preserves_literal_segments() {
        let resolved =
            substitute_variables("${gadget-data}/repos/local", &ctx()).expect("should substitute");
        assert_eq!(resolved, "/data/plug/repos/local");
    }

    #[test]
    fn substitute_handles_multiple_variables() {
        let resolved = substitute_variables("PRE_${gadget-data}_MID_${home}_END", &ctx())
            .expect("should substitute");
        assert_eq!(resolved, "PRE_/data/plug_MID_/home/jake_END");
    }

    #[test]
    fn substitute_passes_through_plain_text() {
        let resolved =
            substitute_variables("plain string no templates", &ctx()).expect("plain text");
        assert_eq!(resolved, "plain string no templates");
    }

    #[test]
    fn substitute_passes_through_dollar_without_brace() {
        let resolved = substitute_variables("price $5 USD", &ctx()).expect("plain text");
        assert_eq!(resolved, "price $5 USD");
    }

    #[test]
    fn substitute_returns_unknown_variable_error() {
        let err = substitute_variables("${gadget-typo}", &ctx()).unwrap_err();
        match err {
            ResolveError::UnknownVariable(name) => assert_eq!(name, "gadget-typo"),
            other => panic!("expected UnknownVariable, got {other:?}"),
        }
    }

    #[test]
    fn substitute_returns_unterminated_error() {
        let err = substitute_variables("${gadget-data", &ctx()).unwrap_err();
        match err {
            ResolveError::Unterminated(rest) => assert!(rest.starts_with("${gadget-data")),
            other => panic!("expected Unterminated, got {other:?}"),
        }
    }

    #[test]
    fn substitute_handles_empty_template() {
        let resolved = substitute_variables("", &ctx()).expect("empty");
        assert_eq!(resolved, "");
    }

    #[test]
    fn substitute_resolves_all_recognized_variables() {
        for name in RECOGNIZED_PERMISSION_VARIABLES {
            let template = format!("${{{name}}}");
            let resolved = substitute_variables(&template, &ctx())
                .unwrap_or_else(|e| panic!("`{name}` should resolve: {e}"));
            assert!(!resolved.is_empty(), "`{name}` resolved to empty string");
        }
    }
}
