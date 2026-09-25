// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Argv Matcher
//
// Security-critical core of the `command` host capability.
// Pure module — no wasmtime, no tokio, no I/O beyond what
// `path_safety::canonical_under_root` performs internally
// for `path-under` constraints.
//
// Two phases:
//
// 1. **Compile**: take the raw `[[permissions.command]]`
//    rules from the manifest plus a `PathContext` (resolved
//    paths for the substitution variables), substitute
//    variables, compile regex/glob patterns, and produce a
//    `Vec<CompiledCommandRule>` ready for runtime
//    evaluation.
//
// 2. **Match**: given an observed `(binary, argv)` from a
//    `command::run` call, walk the compiled rules and find
//    the (single) rule that accepts it. Manifest-time
//    overlap detection guarantees at most one rule
//    matches, so the matcher returns the first hit.
//
// The matcher is a pure function: no caching, no
// allocation beyond what `Result<&CompiledCommandRule, _>`
// requires.
// =========================================================

use std::path::{Path, PathBuf};

use globset::Glob;
use regex::Regex;
use thiserror::Error;

use super::manifest::{ArgvConstraint, CommandPermissionDef};
use super::path_safety::{self, PathError};
use crate::paths::{PathResolver, ResolveError};

// =========================================================
// Compiled rule representation
// =========================================================

/// One per-position constraint, in compiled form.
/// Variants mirror [`ArgvConstraint`] but carry compiled
/// forms (substituted strings, anchored `Regex`, compiled
/// `globset::GlobMatcher`, resolved `PathBuf` root) so the
/// matcher does no work beyond the actual comparison.
#[derive(Debug)]
pub enum CompiledArgvConstraint {
    Literal(String),
    Enum(Vec<String>),
    Glob(globset::GlobMatcher),
    Regex(Regex),
    PathUnder(PathBuf),
    AnyString,
    Rest(Box<CompiledArgvConstraint>),
}

/// One `[[permissions.command]]` rule, compiled.
#[derive(Debug)]
pub struct CompiledCommandRule {
    pub binary: String,
    pub argv: Vec<CompiledArgvConstraint>,
}

// =========================================================
// Errors
// =========================================================

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("rule {rule_index} {field}: {source}")]
    Resolve {
        rule_index: usize,
        field: String,
        #[source]
        source: ResolveError,
    },
    #[error("rule {rule_index} argv[{argv_index}] regex `{pattern}` failed to compile: {source}")]
    Regex {
        rule_index: usize,
        argv_index: usize,
        pattern: String,
        #[source]
        source: regex::Error,
    },
    #[error("rule {rule_index} argv[{argv_index}] glob `{pattern}` failed to compile: {source}")]
    Glob {
        rule_index: usize,
        argv_index: usize,
        pattern: String,
        #[source]
        source: globset::Error,
    },
}

#[derive(Debug, Error)]
pub enum MatchError {
    /// No declared rule accepts the observed `(binary, argv)`.
    /// The string is the binary name; the argv list is
    /// available on the host side for diagnostic logging if
    /// desired.
    #[error("no `[[permissions.command]]` rule accepts `{0}`")]
    NoMatchingRule(String),
}

// =========================================================
// Compile
// =========================================================

/// Compile a single raw `[[permissions.command]]` rule
/// against the provided [`PathResolver`]. Variables in
/// `literal`, `enum` value, and `path-under` root are
/// substituted; regex and glob patterns are compiled. The
/// resulting [`CompiledCommandRule`] is ready for runtime
/// evaluation.
pub fn compile_rule(
    raw: &CommandPermissionDef,
    rule_index: usize,
    resolver: &impl PathResolver,
) -> Result<CompiledCommandRule, CompileError> {
    let argv = raw
        .argv
        .iter()
        .enumerate()
        .map(|(argv_index, c)| compile_constraint(c, rule_index, argv_index, resolver))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(CompiledCommandRule {
        binary: raw.binary.clone(),
        argv,
    })
}

fn compile_constraint(
    constraint: &ArgvConstraint,
    rule_index: usize,
    argv_index: usize,
    resolver: &impl PathResolver,
) -> Result<CompiledArgvConstraint, CompileError> {
    match constraint {
        ArgvConstraint::Literal { value } => {
            let resolved =
                resolver
                    .substitute_variables(value)
                    .map_err(|e| CompileError::Resolve {
                        rule_index,
                        field: format!("argv[{argv_index}].value"),
                        source: e,
                    })?;
            Ok(CompiledArgvConstraint::Literal(resolved))
        }
        ArgvConstraint::Enum { values } => {
            let resolved = values
                .iter()
                .map(|v| {
                    resolver
                        .substitute_variables(v)
                        .map_err(|e| CompileError::Resolve {
                            rule_index,
                            field: format!("argv[{argv_index}].values"),
                            source: e,
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(CompiledArgvConstraint::Enum(resolved))
        }
        ArgvConstraint::Glob { pattern } => {
            let glob = Glob::new(pattern).map_err(|e| CompileError::Glob {
                rule_index,
                argv_index,
                pattern: pattern.clone(),
                source: e,
            })?;
            Ok(CompiledArgvConstraint::Glob(glob.compile_matcher()))
        }
        ArgvConstraint::Regex { pattern } => {
            // Anchor the user pattern. The manifest validator
            // already compile-checks the same anchored form,
            // so this should always succeed — but if globset
            // and regex disagree on what is valid we surface
            // the runtime error rather than panicking.
            let anchored = format!("^(?:{pattern})$");
            let compiled = Regex::new(&anchored).map_err(|e| CompileError::Regex {
                rule_index,
                argv_index,
                pattern: pattern.clone(),
                source: e,
            })?;
            Ok(CompiledArgvConstraint::Regex(compiled))
        }
        ArgvConstraint::PathUnder { root } => {
            let resolved =
                resolver
                    .substitute_variables(root)
                    .map_err(|e| CompileError::Resolve {
                        rule_index,
                        field: format!("argv[{argv_index}].root"),
                        source: e,
                    })?;
            Ok(CompiledArgvConstraint::PathUnder(PathBuf::from(resolved)))
        }
        ArgvConstraint::AnyString => Ok(CompiledArgvConstraint::AnyString),
        ArgvConstraint::Rest { constraint } => {
            // The manifest validator rejects nested `Rest`,
            // so the inner constraint here is never another
            // `Rest`. Compile recursively to keep the type
            // shape consistent.
            let inner = compile_constraint(constraint, rule_index, argv_index, resolver)?;
            Ok(CompiledArgvConstraint::Rest(Box::new(inner)))
        }
    }
}

// =========================================================
// Match
// =========================================================

/// Given a set of compiled rules and the observed
/// `(binary, argv)`, return the first rule that accepts
/// the call. Manifest-time overlap detection guarantees at
/// most one rule will match; this function does not check
/// that invariant at runtime.
///
/// Returns `Err(NoMatchingRule)` when no rule accepts the
/// call. The rule set may be empty (deny by default).
pub fn matches<'r>(
    rules: &'r [CompiledCommandRule],
    binary: &str,
    argv: &[String],
) -> Result<&'r CompiledCommandRule, MatchError> {
    for rule in rules {
        if rule.binary == binary && argv_satisfies_rule(&rule.argv, argv) {
            return Ok(rule);
        }
    }
    Err(MatchError::NoMatchingRule(binary.to_string()))
}

fn argv_satisfies_rule(rule_argv: &[CompiledArgvConstraint], argv: &[String]) -> bool {
    let has_rest = matches!(rule_argv.last(), Some(CompiledArgvConstraint::Rest(_)));
    let prefix_len = if has_rest {
        rule_argv.len() - 1
    } else {
        rule_argv.len()
    };

    if has_rest {
        if argv.len() < prefix_len {
            return false;
        }
    } else if argv.len() != prefix_len {
        return false;
    }

    for (i, c) in rule_argv[..prefix_len].iter().enumerate() {
        if !position_matches(c, &argv[i]) {
            return false;
        }
    }

    if has_rest {
        let CompiledArgvConstraint::Rest(inner) = &rule_argv[prefix_len] else {
            unreachable!("guard above");
        };
        for arg in &argv[prefix_len..] {
            if !position_matches(inner, arg) {
                return false;
            }
        }
    }

    true
}

fn position_matches(constraint: &CompiledArgvConstraint, arg: &str) -> bool {
    match constraint {
        CompiledArgvConstraint::Literal(v) => v == arg,
        CompiledArgvConstraint::Enum(values) => values.iter().any(|v| v == arg),
        CompiledArgvConstraint::Glob(matcher) => matcher.is_match(arg),
        CompiledArgvConstraint::Regex(re) => re.is_match(arg),
        CompiledArgvConstraint::PathUnder(root) => {
            // Plugins must pass absolute paths for
            // path-under to evaluate. A relative path is a
            // mismatch (not an error) — a different rule
            // could still accept the call, so the matcher
            // simply returns false here.
            let candidate = Path::new(arg);
            if !candidate.is_absolute() {
                return false;
            }
            match path_safety::canonical_under_root(candidate, root) {
                Ok(_) => true,
                Err(PathError::EscapesRoot { .. })
                | Err(PathError::DanglingSymlink(_))
                | Err(PathError::CandidateCanonicalize(_))
                | Err(PathError::CandidateNotAbsolute(_))
                | Err(PathError::RootNotAbsolute(_))
                | Err(PathError::RootCanonicalize { .. }) => false,
            }
        }
        CompiledArgvConstraint::AnyString => true,
        CompiledArgvConstraint::Rest(_) => {
            unreachable!("Rest constraints are handled by argv_satisfies_rule directly")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    use crate::paths::{GadgetPaths, PlatformPaths};
    use std::sync::Arc;

    fn ctx_with(gadget_archive: PathBuf, gadget_data: PathBuf) -> GadgetPaths {
        GadgetPaths {
            platform: Arc::new(PlatformPaths {
                home: PathBuf::from("/tmp"),
                xdg_config: PathBuf::from("/tmp"),
                xdg_data: PathBuf::from("/tmp"),
            }),
            gadget_data,
            gadget_archive,
        }
    }

    fn default_ctx() -> GadgetPaths {
        GadgetPaths {
            platform: Arc::new(PlatformPaths {
                home: PathBuf::from("/tmp/home"),
                xdg_config: PathBuf::from("/tmp/config"),
                xdg_data: PathBuf::from("/tmp/data"),
            }),
            gadget_data: PathBuf::from("/tmp/plug-data"),
            gadget_archive: PathBuf::from("/tmp/plug-archive"),
        }
    }

    fn rule(binary: &str, argv: Vec<ArgvConstraint>) -> CommandPermissionDef {
        CommandPermissionDef {
            binary: binary.to_string(),
            argv,
            cwd: None,
            timeout_ms_max: None,
            max_output_bytes: None,
            max_stdin_bytes: None,
        }
    }

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    // ----- Compile -----

    #[test]
    fn compile_substitutes_variables_in_literal() {
        let raw = rule(
            "git",
            vec![ArgvConstraint::Literal {
                value: "${gadget-archive}/repos".to_string(),
            }],
        );
        let compiled = compile_rule(&raw, 0, &default_ctx()).expect("compile");
        match &compiled.argv[0] {
            CompiledArgvConstraint::Literal(v) => {
                assert_eq!(v, "/tmp/plug-archive/repos");
            }
            other => panic!("expected Literal, got {other:?}"),
        }
    }

    #[test]
    fn compile_substitutes_variables_in_path_under() {
        let raw = rule(
            "git",
            vec![ArgvConstraint::PathUnder {
                root: "${gadget-data}/repos".to_string(),
            }],
        );
        let compiled = compile_rule(&raw, 0, &default_ctx()).expect("compile");
        match &compiled.argv[0] {
            CompiledArgvConstraint::PathUnder(root) => {
                assert_eq!(root, &PathBuf::from("/tmp/plug-data/repos"));
            }
            other => panic!("expected PathUnder, got {other:?}"),
        }
    }

    #[test]
    fn compile_anchors_regex() {
        let raw = rule(
            "grep",
            vec![ArgvConstraint::Regex {
                pattern: "[a-z]+".to_string(),
            }],
        );
        let compiled = compile_rule(&raw, 0, &default_ctx()).expect("compile");
        match &compiled.argv[0] {
            CompiledArgvConstraint::Regex(re) => {
                assert!(re.is_match("hello"));
                // Anchored — cannot match a substring with non-letters.
                assert!(!re.is_match("hello123"));
            }
            other => panic!("expected Regex, got {other:?}"),
        }
    }

    #[test]
    fn compile_compiles_glob() {
        let raw = rule(
            "git",
            vec![ArgvConstraint::Glob {
                pattern: "refs/heads/*".to_string(),
            }],
        );
        let compiled = compile_rule(&raw, 0, &default_ctx()).expect("compile");
        match &compiled.argv[0] {
            CompiledArgvConstraint::Glob(matcher) => {
                assert!(matcher.is_match("refs/heads/main"));
                assert!(!matcher.is_match("refs/tags/v1"));
            }
            other => panic!("expected Glob, got {other:?}"),
        }
    }

    // ----- Match: exact / no-rest cases -----

    #[test]
    fn matches_finds_literal_rule() {
        let raw = rule(
            "git",
            vec![ArgvConstraint::Literal {
                value: "rev-parse".to_string(),
            }],
        );
        let compiled = vec![compile_rule(&raw, 0, &default_ctx()).unwrap()];

        matches(&compiled, "git", &args(&["rev-parse"])).expect("should match");
        assert!(matches(&compiled, "git", &args(&["log"])).is_err());
        assert!(matches(&compiled, "other", &args(&["rev-parse"])).is_err());
    }

    #[test]
    fn matches_rejects_wrong_argv_length_without_rest() {
        let raw = rule(
            "git",
            vec![ArgvConstraint::Literal {
                value: "log".to_string(),
            }],
        );
        let compiled = vec![compile_rule(&raw, 0, &default_ctx()).unwrap()];

        // Too many args without rest — mismatch.
        assert!(matches(&compiled, "git", &args(&["log", "HEAD"])).is_err());
        // Too few args — mismatch.
        assert!(matches(&compiled, "git", &args(&[])).is_err());
    }

    #[test]
    fn matches_enum() {
        let raw = rule(
            "git",
            vec![ArgvConstraint::Enum {
                values: vec!["log".to_string(), "show".to_string()],
            }],
        );
        let compiled = vec![compile_rule(&raw, 0, &default_ctx()).unwrap()];

        matches(&compiled, "git", &args(&["log"])).expect("log accepted");
        matches(&compiled, "git", &args(&["show"])).expect("show accepted");
        assert!(matches(&compiled, "git", &args(&["push"])).is_err());
    }

    #[test]
    fn matches_glob_and_regex() {
        let raw = rule(
            "git",
            vec![
                ArgvConstraint::Glob {
                    pattern: "refs/heads/*".to_string(),
                },
                ArgvConstraint::Regex {
                    pattern: "[0-9a-f]{7,40}".to_string(),
                },
            ],
        );
        let compiled = vec![compile_rule(&raw, 0, &default_ctx()).unwrap()];

        matches(&compiled, "git", &args(&["refs/heads/main", "deadbeef"]))
            .expect("both positions match");
        assert!(matches(&compiled, "git", &args(&["refs/tags/v1", "deadbeef"])).is_err());
        assert!(matches(&compiled, "git", &args(&["refs/heads/main", "ghijklm"])).is_err());
    }

    #[test]
    fn matches_any_string() {
        let raw = rule("echo", vec![ArgvConstraint::AnyString]);
        let compiled = vec![compile_rule(&raw, 0, &default_ctx()).unwrap()];

        matches(&compiled, "echo", &args(&["literally anything"])).expect("any-string accepts all");
        matches(&compiled, "echo", &args(&[""])).expect("empty string is a string");
    }

    // ----- Match: Rest -----

    #[test]
    fn matches_rest_accepts_zero_extra_args() {
        let raw = rule(
            "git",
            vec![
                ArgvConstraint::Literal {
                    value: "log".to_string(),
                },
                ArgvConstraint::Rest {
                    constraint: Box::new(ArgvConstraint::AnyString),
                },
            ],
        );
        let compiled = vec![compile_rule(&raw, 0, &default_ctx()).unwrap()];

        matches(&compiled, "git", &args(&["log"])).expect("rest may be empty");
        matches(&compiled, "git", &args(&["log", "HEAD"])).expect("rest of length 1");
        matches(
            &compiled,
            "git",
            &args(&["log", "HEAD", "main", "feature/x"]),
        )
        .expect("rest of length 3");
    }

    #[test]
    fn matches_rest_rejects_when_inner_constraint_fails() {
        let raw = rule(
            "git",
            vec![
                ArgvConstraint::Literal {
                    value: "log".to_string(),
                },
                ArgvConstraint::Rest {
                    constraint: Box::new(ArgvConstraint::Enum {
                        values: vec!["HEAD".to_string(), "main".to_string()],
                    }),
                },
            ],
        );
        let compiled = vec![compile_rule(&raw, 0, &default_ctx()).unwrap()];

        matches(&compiled, "git", &args(&["log", "HEAD", "main"]))
            .expect("all rest values are in enum");
        assert!(
            matches(&compiled, "git", &args(&["log", "HEAD", "feature/x"])).is_err(),
            "trailing value not in enum should fail"
        );
    }

    #[test]
    fn matches_rest_rejects_when_prefix_too_short() {
        let raw = rule(
            "git",
            vec![
                ArgvConstraint::Literal {
                    value: "log".to_string(),
                },
                ArgvConstraint::Literal {
                    value: "HEAD".to_string(),
                },
                ArgvConstraint::Rest {
                    constraint: Box::new(ArgvConstraint::AnyString),
                },
            ],
        );
        let compiled = vec![compile_rule(&raw, 0, &default_ctx()).unwrap()];

        // The rule requires a 2-element prefix before rest.
        // A call with only one arg cannot match.
        assert!(matches(&compiled, "git", &args(&["log"])).is_err());
    }

    // ----- Match: PathUnder -----

    #[test]
    fn matches_path_under_accepts_path_inside_root() {
        let root_dir = TempDir::new().unwrap();
        let inside = root_dir.path().join("subdir");
        fs::create_dir(&inside).unwrap();

        let ctx = ctx_with(PathBuf::from("/tmp/unused"), root_dir.path().to_path_buf());

        let raw = rule(
            "ls",
            vec![ArgvConstraint::PathUnder {
                root: "${gadget-data}".to_string(),
            }],
        );
        let compiled = vec![compile_rule(&raw, 0, &ctx).unwrap()];

        matches(&compiled, "ls", &args(&[inside.to_str().unwrap()]))
            .expect("path inside root should match");
    }

    #[test]
    fn matches_path_under_rejects_path_outside_root() {
        let root_dir = TempDir::new().unwrap();
        let outside_dir = TempDir::new().unwrap();

        let ctx = ctx_with(PathBuf::from("/tmp/unused"), root_dir.path().to_path_buf());

        let raw = rule(
            "ls",
            vec![ArgvConstraint::PathUnder {
                root: "${gadget-data}".to_string(),
            }],
        );
        let compiled = vec![compile_rule(&raw, 0, &ctx).unwrap()];

        assert!(
            matches(
                &compiled,
                "ls",
                &args(&[outside_dir.path().to_str().unwrap()])
            )
            .is_err(),
            "path outside root should not match"
        );
    }

    #[test]
    fn matches_path_under_rejects_relative_path() {
        let root_dir = TempDir::new().unwrap();
        let ctx = ctx_with(PathBuf::from("/tmp/unused"), root_dir.path().to_path_buf());

        let raw = rule(
            "ls",
            vec![ArgvConstraint::PathUnder {
                root: "${gadget-data}".to_string(),
            }],
        );
        let compiled = vec![compile_rule(&raw, 0, &ctx).unwrap()];

        assert!(
            matches(&compiled, "ls", &args(&["relative/subpath"])).is_err(),
            "relative paths are not absolute, must mismatch"
        );
    }

    #[cfg(unix)]
    #[test]
    fn matches_path_under_rejects_path_through_dangling_symlink() {
        let root_dir = TempDir::new().unwrap();
        let outside_dir = TempDir::new().unwrap();
        let link = root_dir.path().join("trapdoor");
        std::os::unix::fs::symlink(outside_dir.path().join("not-yet-there"), &link).unwrap();

        let ctx = ctx_with(PathBuf::from("/tmp/unused"), root_dir.path().to_path_buf());

        let raw = rule(
            "touch",
            vec![ArgvConstraint::PathUnder {
                root: "${gadget-data}".to_string(),
            }],
        );
        let compiled = vec![compile_rule(&raw, 0, &ctx).unwrap()];

        assert!(
            matches(&compiled, "touch", &args(&[link.to_str().unwrap()])).is_err(),
            "a dangling symlink must not match path-under"
        );
    }

    // ----- Multi-rule -----

    #[test]
    fn matches_finds_correct_rule_among_many() {
        let r1 = compile_rule(
            &rule(
                "git",
                vec![ArgvConstraint::Literal {
                    value: "log".to_string(),
                }],
            ),
            0,
            &default_ctx(),
        )
        .unwrap();
        let r2 = compile_rule(
            &rule(
                "git",
                vec![ArgvConstraint::Literal {
                    value: "rev-parse".to_string(),
                }],
            ),
            1,
            &default_ctx(),
        )
        .unwrap();
        let r3 = compile_rule(
            &rule("mdfind", vec![ArgvConstraint::AnyString]),
            2,
            &default_ctx(),
        )
        .unwrap();
        let rules = vec![r1, r2, r3];

        let m = matches(&rules, "git", &args(&["rev-parse"])).expect("matches r2");
        assert_eq!(m.binary, "git");
        let m = matches(&rules, "mdfind", &args(&["query"])).expect("matches r3");
        assert_eq!(m.binary, "mdfind");
        assert!(matches(&rules, "git", &args(&["push"])).is_err());
    }
}
