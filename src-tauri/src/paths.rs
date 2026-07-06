// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Path Resolution
//
// Provides variable substitution for `${...}` tokens in
// permission patterns and runtime path resolution.
//
// Two levels of resolution:
//
// - `PlatformPaths`: platform-level directories (home,
//   config, data). Resolved once at startup, shared across
//   all gadgets.
//
// - `GadgetPaths`: extends `PlatformPaths` with per-gadget
//   directories (gadget-data, gadget-archive). Built per
//   gadget during capability provisioning.
//
// Both implement the `PathResolver` trait, which provides
// variable recognition, resolution, validation, and template
// substitution as a unified interface. The tokenizer is a
// private implementation detail of the default trait methods.
// =========================================================

use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;

// =========================================================
// PathResolver Trait
// =========================================================

/// Unified interface for `${...}` variable resolution.
///
/// Implementors provide `is_recognized` and `resolve`. The
/// default methods `validate_variable_references` and
/// `substitute_variables` build on those two primitives.
pub trait PathResolver {
    /// Whether this resolver handles the given variable name.
    fn is_recognized(&self, name: &str) -> bool;

    /// Resolve a single variable name to a path. Returns
    /// `None` for unrecognized names.
    fn resolve(&self, name: &str) -> Option<&Path>;

    /// Parse-time check: every `${name}` token in `s` must
    /// use a name recognized by this resolver.
    fn validate_variable_references(&self, s: &str) -> anyhow::Result<()> {
        for token in iter_variable_tokens(s) {
            match token {
                Ok(Token::Variable(name)) => {
                    if !self.is_recognized(name) {
                        anyhow::bail!("unknown variable `${{{name}}}` in `{s}`");
                    }
                }
                Ok(Token::Literal(_)) => {}
                Err(TokenizeError::Unterminated(rest)) => {
                    anyhow::bail!("unterminated `${{...}}` reference at `{rest}` in `{s}`");
                }
            }
        }
        Ok(())
    }

    /// Runtime substitution: replace every `${name}` token
    /// in `template` with the corresponding resolved path.
    fn substitute_variables(&self, template: &str) -> Result<String, ResolveError> {
        let mut result = String::with_capacity(template.len());
        for token in iter_variable_tokens(template) {
            match token.map_err(|e| match e {
                TokenizeError::Unterminated(rest) => ResolveError::Unterminated(rest),
            })? {
                Token::Literal(s) => result.push_str(s),
                Token::Variable(name) => {
                    let Some(path) = self.resolve(name) else {
                        return Err(ResolveError::UnknownVariable(name.to_string()));
                    };
                    let path_str = path.to_string_lossy();
                    result.push_str(&path_str);
                }
            }
        }
        Ok(result)
    }
}

// =========================================================
// ResolveError
// =========================================================

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("unknown variable `${{{0}}}`")]
    UnknownVariable(String),

    #[error("unterminated `${{...}}` reference at `{0}`")]
    Unterminated(String),
}

// =========================================================
// PlatformPaths
// =========================================================

/// Platform-level directories shared across all gadgets.
/// Resolved once from the application handle at startup.
#[derive(Debug, Clone)]
pub struct PlatformPaths {
    pub home: PathBuf,
    pub xdg_config: PathBuf,
    pub xdg_data: PathBuf,
}

impl PathResolver for PlatformPaths {
    fn is_recognized(&self, name: &str) -> bool {
        matches!(name, "home" | "xdg-config" | "xdg-data")
    }

    fn resolve(&self, name: &str) -> Option<&Path> {
        match name {
            "home" => Some(&self.home),
            "xdg-config" => Some(&self.xdg_config),
            "xdg-data" => Some(&self.xdg_data),
            _ => None,
        }
    }
}

// =========================================================
// GadgetPaths
// =========================================================

/// Per-gadget path context extending `PlatformPaths` with
/// gadget-specific directories. Built by the gadget host
/// during capability provisioning.
#[derive(Debug, Clone)]
pub struct GadgetPaths {
    pub platform: Arc<PlatformPaths>,
    pub gadget_data: PathBuf,
    pub gadget_archive: PathBuf,
}

impl PathResolver for GadgetPaths {
    fn is_recognized(&self, name: &str) -> bool {
        matches!(name, "gadget-data" | "gadget-archive") || self.platform.is_recognized(name)
    }

    fn resolve(&self, name: &str) -> Option<&Path> {
        match name {
            "gadget-data" => Some(&self.gadget_data),
            "gadget-archive" => Some(&self.gadget_archive),
            _ => self.platform.resolve(name),
        }
    }
}

// =========================================================
// ParseTimeResolver
// =========================================================

/// Parse-time-only resolver that recognizes all valid
/// variable names but cannot resolve them to paths. Used by
/// manifest validators that need to check variable name
/// validity before a `GadgetPaths` is available.
pub struct ParseTimeResolver;

impl PathResolver for ParseTimeResolver {
    fn is_recognized(&self, name: &str) -> bool {
        matches!(
            name,
            "home" | "xdg-config" | "xdg-data" | "gadget-data" | "gadget-archive"
        )
    }

    fn resolve(&self, _name: &str) -> Option<&Path> {
        None
    }
}

// =========================================================
// Tokenizer (private)
//
// Single tokenization pass shared by the default
// `validate_variable_references` and `substitute_variables`
// trait methods.
//
// Handles only `${name}` tokens; `$` followed by anything
// other than `{` is a plain literal. There is no escape
// syntax.
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

        let mut scan = cursor;
        while scan < bytes.len() {
            if bytes[scan] == b'$' && scan + 1 < bytes.len() && bytes[scan + 1] == b'{' {
                if scan > cursor {
                    let literal = &s[cursor..scan];
                    cursor = scan;
                    return Some(Ok(Token::Literal(literal)));
                }
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

        let literal = &s[cursor..];
        cursor = bytes.len();
        Some(Ok(Token::Literal(literal)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platform() -> PlatformPaths {
        PlatformPaths {
            home: PathBuf::from("/home/jake"),
            xdg_config: PathBuf::from("/home/jake/.config"),
            xdg_data: PathBuf::from("/home/jake/.local/share"),
        }
    }

    fn gadget_paths() -> GadgetPaths {
        GadgetPaths {
            platform: Arc::new(platform()),
            gadget_data: PathBuf::from("/data/plug"),
            gadget_archive: PathBuf::from("/archive/plug"),
        }
    }

    // ----- PlatformPaths -----

    #[test]
    fn platform_recognizes_own_variables() {
        let p = platform();
        assert!(p.is_recognized("home"));
        assert!(p.is_recognized("xdg-config"));
        assert!(p.is_recognized("xdg-data"));
    }

    #[test]
    fn platform_does_not_recognize_gadget_variables() {
        let p = platform();
        assert!(!p.is_recognized("gadget-data"));
        assert!(!p.is_recognized("gadget-archive"));
    }

    #[test]
    fn platform_resolves_known_variables() {
        let p = platform();
        assert_eq!(p.resolve("home"), Some(Path::new("/home/jake")));
        assert_eq!(
            p.resolve("xdg-config"),
            Some(Path::new("/home/jake/.config"))
        );
        assert_eq!(
            p.resolve("xdg-data"),
            Some(Path::new("/home/jake/.local/share"))
        );
    }

    #[test]
    fn platform_resolve_returns_none_for_unknown() {
        assert_eq!(platform().resolve("gadget-data"), None);
    }

    // ----- GadgetPaths -----

    #[test]
    fn gadget_recognizes_all_variables() {
        let g = gadget_paths();
        assert!(g.is_recognized("home"));
        assert!(g.is_recognized("xdg-config"));
        assert!(g.is_recognized("xdg-data"));
        assert!(g.is_recognized("gadget-data"));
        assert!(g.is_recognized("gadget-archive"));
    }

    #[test]
    fn gadget_does_not_recognize_unknown() {
        assert!(!gadget_paths().is_recognized("unknown-var"));
    }

    #[test]
    fn gadget_resolves_own_variables() {
        let g = gadget_paths();
        assert_eq!(g.resolve("gadget-data"), Some(Path::new("/data/plug")));
        assert_eq!(
            g.resolve("gadget-archive"),
            Some(Path::new("/archive/plug"))
        );
    }

    #[test]
    fn gadget_delegates_platform_resolution() {
        let g = gadget_paths();
        assert_eq!(g.resolve("home"), Some(Path::new("/home/jake")));
        assert_eq!(
            g.resolve("xdg-config"),
            Some(Path::new("/home/jake/.config"))
        );
    }

    // ----- validate_variable_references (via trait) -----

    #[test]
    fn validate_accepts_recognized_names() {
        let g = gadget_paths();
        for name in [
            "gadget-data",
            "gadget-archive",
            "home",
            "xdg-config",
            "xdg-data",
        ] {
            let s = format!("prefix-${{{name}}}-suffix");
            g.validate_variable_references(&s)
                .unwrap_or_else(|e| panic!("`{name}` should be accepted: {e}"));
        }
    }

    #[test]
    fn validate_accepts_no_variables() {
        gadget_paths()
            .validate_variable_references("plain string with no template")
            .expect("plain text is fine");
    }

    #[test]
    fn validate_accepts_dollar_without_brace() {
        let g = gadget_paths();
        g.validate_variable_references("price $5 USD")
            .expect("plain text");
        g.validate_variable_references("ENV=$NAME")
            .expect("plain text");
    }

    #[test]
    fn validate_rejects_unknown_variable() {
        let err = gadget_paths()
            .validate_variable_references("${gadget-typo}/foo")
            .unwrap_err();
        assert!(err.to_string().contains("gadget-typo"));
    }

    #[test]
    fn validate_rejects_first_unknown_in_chain() {
        let err = gadget_paths()
            .validate_variable_references("${gadget-data}/${unknown}/${home}")
            .unwrap_err();
        assert!(err.to_string().contains("unknown"));
    }

    #[test]
    fn platform_validate_rejects_gadget_variables() {
        let err = platform()
            .validate_variable_references("${gadget-data}/foo")
            .unwrap_err();
        assert!(err.to_string().contains("gadget-data"));
    }

    // ----- substitute_variables (via trait) -----

    #[test]
    fn substitute_replaces_single_variable() {
        let resolved = gadget_paths()
            .substitute_variables("${gadget-data}")
            .expect("should substitute");
        assert_eq!(resolved, "/data/plug");
    }

    #[test]
    fn substitute_preserves_literal_segments() {
        let resolved = gadget_paths()
            .substitute_variables("${gadget-data}/repos/local")
            .expect("should substitute");
        assert_eq!(resolved, "/data/plug/repos/local");
    }

    #[test]
    fn substitute_handles_multiple_variables() {
        let resolved = gadget_paths()
            .substitute_variables("PRE_${gadget-data}_MID_${home}_END")
            .expect("should substitute");
        assert_eq!(resolved, "PRE_/data/plug_MID_/home/jake_END");
    }

    #[test]
    fn substitute_passes_through_plain_text() {
        let resolved = gadget_paths()
            .substitute_variables("plain string no templates")
            .expect("plain text");
        assert_eq!(resolved, "plain string no templates");
    }

    #[test]
    fn substitute_passes_through_dollar_without_brace() {
        let resolved = gadget_paths()
            .substitute_variables("price $5 USD")
            .expect("plain text");
        assert_eq!(resolved, "price $5 USD");
    }

    #[test]
    fn substitute_returns_unknown_variable_error() {
        let err = gadget_paths()
            .substitute_variables("${gadget-typo}")
            .unwrap_err();
        match err {
            ResolveError::UnknownVariable(name) => assert_eq!(name, "gadget-typo"),
            other => panic!("expected UnknownVariable, got {other:?}"),
        }
    }

    #[test]
    fn substitute_returns_unterminated_error() {
        let err = gadget_paths()
            .substitute_variables("${gadget-data")
            .unwrap_err();
        match err {
            ResolveError::Unterminated(rest) => assert!(rest.starts_with("${gadget-data")),
            other => panic!("expected Unterminated, got {other:?}"),
        }
    }

    #[test]
    fn substitute_handles_empty_template() {
        let resolved = gadget_paths().substitute_variables("").expect("empty");
        assert_eq!(resolved, "");
    }

    #[test]
    fn substitute_resolves_all_gadget_variables() {
        let g = gadget_paths();
        for name in [
            "gadget-data",
            "gadget-archive",
            "home",
            "xdg-config",
            "xdg-data",
        ] {
            let template = format!("${{{name}}}");
            let resolved = g
                .substitute_variables(&template)
                .unwrap_or_else(|e| panic!("`{name}` should resolve: {e}"));
            assert!(!resolved.is_empty(), "`{name}` resolved to empty string");
        }
    }

    #[test]
    fn platform_substitute_rejects_gadget_variables() {
        let err = platform()
            .substitute_variables("${gadget-data}")
            .unwrap_err();
        match err {
            ResolveError::UnknownVariable(name) => assert_eq!(name, "gadget-data"),
            other => panic!("expected UnknownVariable, got {other:?}"),
        }
    }
}
