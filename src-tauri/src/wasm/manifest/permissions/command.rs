// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};

use crate::wasm::permission_vars::validate_variable_references;

/// `[[permissions.command]]` rule — a single binary +
/// argv-shape pattern the plugin is permitted to invoke
/// via `command::run`.
///
/// ```toml
/// [[permissions.command]]
/// binary = "mdfind"
/// argv = [
///     { kind = "literal", value = "kMDItemContentType == 'com.apple.application-bundle'" },
/// ]
/// ```
///
/// The `binary` field is either an absolute path
/// (`"/usr/bin/mdfind"`) or a `PATH`-resolved name
/// (`"mdfind"`). Opener-class binaries (`open`, `xdg-open`,
/// `start`, etc.) are rejected at manifest parse time —
/// plugins wanting "open with the registered application"
/// use `[permissions.opener] open-path = true` instead.
///
/// `argv` is a per-position constraint list. Each element
/// declares what kind of argv value is accepted at that
/// position. An empty `argv` list means the binary is
/// invoked with no arguments. The `rest` constraint kind
/// covers all remaining positions and may only appear at
/// the trailing position.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandPermissionDef {
    /// The binary the rule grants. Absolute path or
    /// PATH-resolved name. Validated at manifest parse
    /// time (no NUL bytes, not in opener-class denylist).
    pub binary: String,

    /// Per-position argv constraints. Empty means the
    /// rule grants `binary` with zero arguments.
    #[serde(default)]
    pub argv: Vec<ArgvConstraint>,

    /// Optional default working directory for invocations
    /// matching this rule. Plugin can override per-call;
    /// when omitted, the host falls back to the per-plugin
    /// scratch directory at `${gadget-data}/exec-cwd/`.
    pub cwd: Option<String>,

    /// Hard ceiling on `command-options.timeout-ms` for
    /// invocations matching this rule. Calls that request
    /// a longer timeout are clamped down. `None` defers to
    /// the host default.
    #[serde(default, rename = "timeout-ms-max")]
    pub timeout_ms_max: Option<u32>,

    /// Hard ceiling on `command-options.max-output-bytes`
    /// for invocations matching this rule. `None` defers
    /// to the host default.
    #[serde(default, rename = "max-output-bytes")]
    pub max_output_bytes: Option<u64>,

    /// Hard ceiling on `command-options.stdin` byte length
    /// for invocations matching this rule. `None` defers
    /// to the host default.
    #[serde(default, rename = "max-stdin-bytes")]
    pub max_stdin_bytes: Option<u64>,
}

impl CommandPermissionDef {
    /// Validate a single rule's argv shape and binary name.
    /// Errors include the rule index for actionable diagnostics.
    pub(super) fn validate_rule(&self, index: usize) -> anyhow::Result<()> {
        if self.binary.is_empty() {
            anyhow::bail!("`[[permissions.command]]` rule {index} has empty `binary`");
        }
        if self.binary.contains('\0') {
            anyhow::bail!("`[[permissions.command]]` rule {index} `binary` contains a NUL byte");
        }

        let mut saw_rest = false;
        for (argv_index, constraint) in self.argv.iter().enumerate() {
            if saw_rest {
                anyhow::bail!(
                    "`[[permissions.command]]` rule {index} declares an argv constraint at \
                     position {argv_index} after a `rest` constraint — `rest` must be the \
                     trailing position"
                );
            }
            validate_argv_constraint(constraint, index, argv_index, false)?;
            if matches!(constraint, ArgvConstraint::Rest { .. }) {
                saw_rest = true;
            }
        }

        if let Some(cwd) = &self.cwd {
            validate_variable_references(cwd, "cwd", index)?;
        }

        Ok(())
    }
}

/// Reject any pair of `[[permissions.command]]` rules whose
/// `(binary, argv-shape)` could both accept the same call.
/// Forces unambiguous manifests so a permission decision can
/// never silently depend on rule ordering at runtime.
///
/// Detection is conservative: when constraints are not
/// computable as a set (`glob`, `regex`, `path-under`,
/// pre-substitution variable references), the helper
/// assumes overlap. False positives force authors to
/// differentiate via positions that *are* computable
/// (`binary`, `literal`, `enum`, length, `rest` placement).
/// This is the safe direction — better to ask authors to
/// be more specific than to silently merge two rules at
/// runtime.
pub(super) fn validate_rules(rules: &[CommandPermissionDef]) -> anyhow::Result<()> {
    for (index, rule) in rules.iter().enumerate() {
        rule.validate_rule(index)?;
    }
    detect_command_rule_overlap(rules)
}

/// One per-position argv constraint. Internally tagged via
/// the `kind` discriminator.
///
/// Constraint kinds:
///
/// - `literal`     — exact byte match against `value`.
/// - `enum`        — argv element must equal one of `values`.
/// - `glob`        — argv element must match `pattern` as a glob.
/// - `regex`       — argv element must match `pattern` (anchored).
/// - `path-under`  — argv element parses as an absolute path that
///                   canonicalizes under `root`. `root` may use the
///                   substitution variables `${gadget-data}`,
///                   `${gadget-archive}`, `${home}`, `${xdg-config}`,
///                   `${xdg-data}`.
/// - `any-string`  — argv element accepted unconditionally.
/// - `rest`        — applies `constraint` to every remaining argv
///                   element. May only appear at the trailing position.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ArgvConstraint {
    Literal {
        value: String,
    },
    Enum {
        values: Vec<String>,
    },
    Glob {
        pattern: String,
    },
    Regex {
        pattern: String,
    },
    #[serde(rename = "path-under")]
    PathUnder {
        root: String,
    },
    AnyString,
    Rest {
        constraint: Box<ArgvConstraint>,
    },
}

// Substitution variables recognized in `literal`, `enum`,
// `path-under`, and per-rule `cwd` fields are defined in
// `crate::wasm::permission_vars` and shared with the runtime
// `paths::resolve` host import. See that module for the
// list and the parser/substituter implementations.

// =========================================================
// Internal validators
// =========================================================

/// Reject any pair of rules for the same binary whose argv
/// shapes could both accept the same call.
fn detect_command_rule_overlap(rules: &[CommandPermissionDef]) -> anyhow::Result<()> {
    for i in 0..rules.len() {
        for j in (i + 1)..rules.len() {
            if rules[i].binary != rules[j].binary {
                continue;
            }
            if argv_shapes_can_overlap(&rules[i].argv, &rules[j].argv) {
                anyhow::bail!(
                    "`[[permissions.command]]` rules {i} and {j} both grant `{bin}` \
                     and accept potentially-overlapping argv shapes — split or merge \
                     them so each invocation matches exactly one rule",
                    bin = rules[i].binary
                );
            }
        }
    }
    Ok(())
}

/// Can two argv-shape vectors both accept the same argv
/// tuple? Used by [`detect_command_rule_overlap`].
///
/// Both shapes may end in a `Rest` constraint, which acts
/// as "every remaining position must match this inner
/// constraint". The function unwraps `Rest`s and walks the
/// fixed prefixes against each other; tail handling differs
/// by whether one or both shapes carry a `Rest`.
fn argv_shapes_can_overlap(a: &[ArgvConstraint], b: &[ArgvConstraint]) -> bool {
    let a_has_rest = matches!(a.last(), Some(ArgvConstraint::Rest { .. }));
    let b_has_rest = matches!(b.last(), Some(ArgvConstraint::Rest { .. }));

    let a_prefix_len = if a_has_rest { a.len() - 1 } else { a.len() };
    let b_prefix_len = if b_has_rest { b.len() - 1 } else { b.len() };

    match (a_has_rest, b_has_rest) {
        (false, false) => {
            // Both fixed length. Lengths must match exactly,
            // then per-position constraints must overlap.
            if a.len() != b.len() {
                return false;
            }
            a.iter()
                .zip(b.iter())
                .all(|(ca, cb)| position_overlaps(ca, cb))
        }
        (true, false) => fixed_compatible_with_rest_rule(b, a),
        (false, true) => fixed_compatible_with_rest_rule(a, b),
        (true, true) => {
            // Both shapes carry a `Rest`. The fixed prefixes
            // must overlap up to the shorter prefix length;
            // beyond that, each rule's `Rest` constraint
            // must accept the other rule's still-fixed
            // positions. The empty-tail case (no further
            // args) is trivially compatible.
            let min_prefix = a_prefix_len.min(b_prefix_len);
            for i in 0..min_prefix {
                if !position_overlaps(&a[i], &b[i]) {
                    return false;
                }
            }
            if a_prefix_len > min_prefix {
                let b_rest_inner = unwrap_rest(&b[b_prefix_len]);
                for c in &a[min_prefix..a_prefix_len] {
                    if !position_overlaps(c, b_rest_inner) {
                        return false;
                    }
                }
            }
            if b_prefix_len > min_prefix {
                let a_rest_inner = unwrap_rest(&a[a_prefix_len]);
                for c in &b[min_prefix..b_prefix_len] {
                    if !position_overlaps(c, a_rest_inner) {
                        return false;
                    }
                }
            }
            true
        }
    }
}

/// Check whether a fixed-length rule's argv could be
/// accepted by a rule that ends in `Rest`. The fixed rule
/// must be at least as long as the rest-rule's prefix, the
/// shared prefix must overlap pairwise, and every
/// remaining fixed position must overlap with the
/// rest-rule's inner constraint.
fn fixed_compatible_with_rest_rule(fixed: &[ArgvConstraint], rest_rule: &[ArgvConstraint]) -> bool {
    let rest_prefix_len = rest_rule.len() - 1;
    if fixed.len() < rest_prefix_len {
        return false;
    }
    for i in 0..rest_prefix_len {
        if !position_overlaps(&fixed[i], &rest_rule[i]) {
            return false;
        }
    }
    let rest_inner = unwrap_rest(&rest_rule[rest_prefix_len]);
    for c in &fixed[rest_prefix_len..] {
        if !position_overlaps(c, rest_inner) {
            return false;
        }
    }
    true
}

fn unwrap_rest(constraint: &ArgvConstraint) -> &ArgvConstraint {
    match constraint {
        ArgvConstraint::Rest { constraint } => constraint.as_ref(),
        _ => unreachable!("caller must pass a Rest constraint"),
    }
}

/// Per-position constraint compatibility — does there
/// exist any string accepted by both `a` and `b`?
///
/// Computable cases (`literal`, `enum`, `any-string`)
/// resolve precisely. Pattern-style constraints (`glob`,
/// `regex`, `path-under`) cannot be compared as
/// computable string sets without compilation and a string
/// solver — the helper conservatively returns `true` for
/// any pair involving them. This produces false positives,
/// but that is the safe direction: authors are forced to
/// differentiate via the *computable* positions in their
/// rule, which is exactly the kind of clarity overlap
/// detection is meant to enforce.
fn position_overlaps(a: &ArgvConstraint, b: &ArgvConstraint) -> bool {
    use ArgvConstraint::*;
    match (a, b) {
        (AnyString, _) | (_, AnyString) => true,
        (Literal { value: va }, Literal { value: vb }) => va == vb,
        (Literal { value: v }, Enum { values }) | (Enum { values }, Literal { value: v }) => {
            values.contains(v)
        }
        (Enum { values: va }, Enum { values: vb }) => va.iter().any(|v| vb.contains(v)),
        // Pattern-style constraints — conservatively true.
        (Glob { .. } | Regex { .. } | PathUnder { .. }, _)
        | (_, Glob { .. } | Regex { .. } | PathUnder { .. }) => true,
        // `Rest` lives only at the trailing position and is
        // unwrapped by callers before reaching here.
        (Rest { .. }, _) | (_, Rest { .. }) => {
            unreachable!("Rest constraints are unwrapped before per-position comparison")
        }
    }
}

/// Validate a single `[[permissions.command]]` rule's
/// shape. Errors are reported with the rule index so a
/// manifest with multiple rules can pinpoint the offender.
///
/// Validation only — no resolution or compilation. Variable
/// references are recognized but not substituted; regex
/// patterns are anchored and compile-tested but the
/// compiled form is discarded (the matcher recompiles when
/// it builds its compiled rule set). Glob patterns are
/// checked for non-emptiness only; full glob compilation
/// happens in the matcher.
fn validate_argv_constraint(
    constraint: &ArgvConstraint,
    rule_index: usize,
    argv_index: usize,
    nested_in_rest: bool,
) -> anyhow::Result<()> {
    let where_ = format!("rule {rule_index} argv[{argv_index}]");
    match constraint {
        ArgvConstraint::Literal { value } => {
            validate_variable_references(value, &format!("{where_}.value"), rule_index)?;
        }
        ArgvConstraint::Enum { values } => {
            if values.is_empty() {
                anyhow::bail!("{where_} `enum` constraint has empty `values` list");
            }
            for v in values {
                validate_variable_references(v, &format!("{where_}.values"), rule_index)?;
            }
        }
        ArgvConstraint::Glob { pattern } => {
            if pattern.is_empty() {
                anyhow::bail!("{where_} `glob` constraint has empty `pattern`");
            }
        }
        ArgvConstraint::Regex { pattern } => {
            // Anchor before compile-checking so plugin authors
            // get the exact same syntax behaviour the matcher
            // will use at runtime.
            let anchored = format!("^(?:{pattern})$");
            regex::Regex::new(&anchored).map_err(|e| {
                anyhow::anyhow!("{where_} `regex` pattern `{pattern}` does not compile: {e}")
            })?;
        }
        ArgvConstraint::PathUnder { root } => {
            if root.is_empty() {
                anyhow::bail!("{where_} `path-under` constraint has empty `root`");
            }
            validate_variable_references(root, &format!("{where_}.root"), rule_index)?;
        }
        ArgvConstraint::AnyString => {}
        ArgvConstraint::Rest { constraint } => {
            if nested_in_rest {
                anyhow::bail!(
                    "{where_} declares a `rest` constraint nested inside another `rest` — \
                     `rest` may only appear at the top level"
                );
            }
            validate_argv_constraint(constraint, rule_index, argv_index, true)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::wasm::manifest::Manifest;
    use crate::wasm::manifest::test_helpers::minimal;

    // =====================================================
    // Permissions: command rules
    // =====================================================

    #[test]
    fn accept_minimal_command_rule() {
        let m = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "mdfind""#,
        ))
        .expect("should parse");
        let command = m.permissions.unwrap().command;
        assert_eq!(command.len(), 1);
        assert_eq!(command[0].binary, "mdfind");
        assert!(command[0].argv.is_empty());
    }

    #[test]
    fn accept_command_rule_with_full_argv_vocabulary() {
        let m = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "literal", value = "log" },
                   { kind = "enum", values = ["HEAD", "main"] },
                   { kind = "glob", pattern = "refs/heads/*" },
                   { kind = "regex", pattern = "[0-9a-f]{40}" },
                   { kind = "path-under", root = "${gadget-data}/repos" },
                   { kind = "any-string" },
                   { kind = "rest", constraint = { kind = "any-string" } },
               ]"#,
        ))
        .expect("should parse");
        let command = m.permissions.unwrap().command;
        assert_eq!(command[0].argv.len(), 7);
    }

    #[test]
    fn reject_command_rule_with_empty_binary() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = """#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("empty `binary`"));
    }

    #[test]
    fn reject_command_rule_with_nul_binary() {
        let err = Manifest::parse(&minimal(
            "[[permissions.command]]\nbinary = \"foo\\u0000bar\"\n",
        ))
        .unwrap_err();
        assert!(err.to_string().contains("NUL byte"));
    }

    #[test]
    fn reject_command_rule_with_bad_regex() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "grep"
               argv = [{ kind = "regex", pattern = "[unclosed" }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("does not compile"));
    }

    #[test]
    fn reject_command_rule_with_empty_enum() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "enum", values = [] }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("empty `values`"));
    }

    #[test]
    fn reject_command_rule_with_empty_glob() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "glob", pattern = "" }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("empty `pattern`"));
    }

    #[test]
    fn reject_command_rule_with_empty_path_under_root() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "path-under", root = "" }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("empty `root`"));
    }

    #[test]
    fn reject_command_rule_with_unknown_variable() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "${plugin-typo}" }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("unknown variable"));
    }

    #[test]
    fn reject_command_rule_with_argv_after_rest() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "rest", constraint = { kind = "any-string" } },
                   { kind = "literal", value = "trailing" },
               ]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("after a `rest` constraint"));
    }

    #[test]
    fn reject_command_rule_with_nested_rest() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "rest", constraint = { kind = "rest", constraint = { kind = "any-string" } } },
               ]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("nested inside another `rest`"));
    }

    #[test]
    fn accept_recognized_variables_in_literal() {
        for var in &[
            "gadget-data",
            "gadget-archive",
            "home",
            "xdg-config",
            "xdg-data",
        ] {
            let toml_text = format!(
                r#"[[permissions.command]]
                   binary = "echo"
                   argv = [{{ kind = "literal", value = "${{{var}}}/foo" }}]"#
            );
            Manifest::parse(&minimal(&toml_text))
                .unwrap_or_else(|e| panic!("variable `{var}` should be accepted: {e}"));
        }
    }

    #[test]
    fn accept_multiple_command_rules() {
        let m = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "mdfind"

               [[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "rev-parse" }]"#,
        ))
        .expect("should parse");
        let command = m.permissions.unwrap().command;
        assert_eq!(command.len(), 2);
        assert_eq!(command[0].binary, "mdfind");
        assert_eq!(command[1].binary, "git");
    }

    #[test]
    fn manifest_without_command_section_has_empty_command_vec() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["*"]"#,
        ))
        .expect("should parse");
        assert!(m.permissions.unwrap().command.is_empty());
    }

    // =====================================================
    // Permissions: command rule overlap
    // =====================================================

    #[test]
    fn accept_rules_with_different_binaries() {
        // Even though argv shapes are identical, distinct
        // binaries means the rules cannot match the same call.
        Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "mdfind"

               [[permissions.command]]
               binary = "git""#,
        ))
        .expect("different binaries do not overlap");
    }

    #[test]
    fn accept_rules_distinguished_by_literal_position() {
        Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "log" }]

               [[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "rev-parse" }]"#,
        ))
        .expect("different first-position literals do not overlap");
    }

    #[test]
    fn reject_identical_rules() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "log" }]

               [[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "log" }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("potentially-overlapping"));
    }

    #[test]
    fn reject_literal_subsumed_by_enum() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "log" }]

               [[permissions.command]]
               binary = "git"
               argv = [{ kind = "enum", values = ["log", "show"] }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("potentially-overlapping"));
    }

    #[test]
    fn reject_enums_with_intersection() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "enum", values = ["log", "show"] }]

               [[permissions.command]]
               binary = "git"
               argv = [{ kind = "enum", values = ["show", "diff"] }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("potentially-overlapping"));
    }

    #[test]
    fn accept_enums_without_intersection() {
        Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "enum", values = ["log", "show"] }]

               [[permissions.command]]
               binary = "git"
               argv = [{ kind = "enum", values = ["push", "pull"] }]"#,
        ))
        .expect("disjoint enums do not overlap");
    }

    #[test]
    fn accept_rules_of_different_length_without_rest() {
        Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "log" }]

               [[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "literal", value = "log" },
                   { kind = "literal", value = "HEAD" },
               ]"#,
        ))
        .expect("different fixed lengths cannot match the same argv");
    }

    #[test]
    fn reject_pattern_constraint_against_literal_at_same_length() {
        // `glob` / `regex` / `path-under` are conservatively
        // treated as overlapping with anything at the same
        // position — authors must differentiate elsewhere.
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "log" }]

               [[permissions.command]]
               binary = "git"
               argv = [{ kind = "regex", pattern = "[a-z]+" }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("potentially-overlapping"));
    }

    #[test]
    fn reject_rest_swallowing_fixed_rule() {
        // Rule 1 accepts ["log", X*]; rule 2 accepts ["log", "HEAD"].
        // Rule 1's rest-of-any-string trivially overlaps rule 2.
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "literal", value = "log" },
                   { kind = "rest", constraint = { kind = "any-string" } },
               ]

               [[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "literal", value = "log" },
                   { kind = "literal", value = "HEAD" },
               ]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("potentially-overlapping"));
    }

    #[test]
    fn accept_rest_with_disjoint_prefix() {
        // Rule 1's prefix is `log`; rule 2 starts with `show`.
        // The prefixes don't overlap, so no argv tuple matches both.
        Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "literal", value = "log" },
                   { kind = "rest", constraint = { kind = "any-string" } },
               ]

               [[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "literal", value = "show" },
                   { kind = "rest", constraint = { kind = "any-string" } },
               ]"#,
        ))
        .expect("disjoint prefixes do not overlap even with rest on both");
    }

    #[test]
    fn accept_fixed_rule_shorter_than_rest_prefix() {
        // Rule 1 needs 2 fixed args + rest; rule 2 has only 1 fixed arg.
        // Rule 2's call cannot satisfy rule 1's required-prefix length,
        // so they do not overlap.
        Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "literal", value = "log" },
                   { kind = "literal", value = "HEAD" },
                   { kind = "rest", constraint = { kind = "any-string" } },
               ]

               [[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "log" }]"#,
        ))
        .expect("fixed rule shorter than rest-rule prefix does not overlap");
    }
}
