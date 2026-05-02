// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Permissions
//
// Plugins opt into host capabilities by declaring the
// relevant sub-table or array under `[permissions]`.
// Omitting a sub-table or array means the capability is
// denied. This mirrors the Android/iOS permission model:
// no capability is implicitly granted.
//
// Origins in `[permissions.http]` are normalized to
// `url::Origin::ascii_serialization()` form at parse time
// (see `validate_permissions`) so runtime checks are plain
// string equality — no re-parsing at call time — and
// variant spellings like `"https://example.com/"`,
// `"HTTPS://example.com"`, and `"https://example.com:443"`
// all map to the same canonical form `"https://example.com"`.
//
// `[[permissions.command]]` rules carry a binary name plus
// a per-position argv constraint vocabulary (literal, enum,
// glob, regex, path-under, any-string, rest). Argv
// constraints validate the *shape* of invocations; they do
// not constrain what a binary does once invoked. See ADR
// 0040 for the full trust-model rationale.
// =========================================================

use serde::{Deserialize, Serialize};

pub(super) mod opener;
pub use opener::OpenerPermissionsDef;

pub(super) mod http;
pub use http::HttpPermissionsDef;

pub(super) mod fs;
pub use fs::FsPermissionsDef;

use super::{ArgvConstraint, CommandPermissionDef};
use crate::wasm::permission_vars::validate_variable_references;

/// `[permissions]` block.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PermissionsDef {
    /// `[permissions.opener]` — URL-opening + path-handling
    /// capabilities.
    pub opener: Option<OpenerPermissionsDef>,
    /// `[permissions.http]` — HTTP fetch capability.
    pub http: Option<HttpPermissionsDef>,
    /// `[permissions.fs]` — read-only filesystem access via
    /// the `fs::read-file` / `file-exists` / `metadata` host
    /// imports.
    pub fs: Option<FsPermissionsDef>,
    /// `[[permissions.command]]` rules — process-execution
    /// capability with per-rule argv constraints. Empty
    /// vector when no rules are declared (deny by default).
    #[serde(default)]
    pub command: Vec<CommandPermissionDef>,
    /// `permissions.website-metadata` — read/write access to the
    /// host-managed website metadata cache. Boolean rather than a
    /// struct because there is no per-domain allowlist; the cache
    /// is shared host-wide and rate-limiting / disk-pressure
    /// concerns are absorbed by the cache layer itself.
    #[serde(default, rename = "website-metadata")]
    pub website_metadata: bool,
}

// =========================================================
// Dispatcher
// =========================================================

/// Validate and normalize `[permissions]` at manifest parse time.
///
/// HTTP origins are normalized to `ascii_serialization()` form so
/// runtime checks can use plain string equality — no re-parsing.
///
/// Rejects:
/// - `[permissions.opener]` declared without granting any
///   capability (no schemes, both booleans `false`).
/// - `[permissions.http]` with an empty `origins` list.
/// - Any `origins` entry that is not a parseable URL (and not `"*"`).
/// - `[[permissions.command]]` rules whose binary is empty,
///   contains a NUL byte, or whose argv constraints fail
///   shape validation (bad regex, empty enum/glob/path-under,
///   nested `rest`, unknown variable references).
pub(super) fn validate_permissions(permissions: PermissionsDef) -> anyhow::Result<PermissionsDef> {
    let opener = permissions.opener.map(|o| o.validate()).transpose()?;

    let http = permissions.http.map(|h| h.validate()).transpose()?;

    let fs = permissions.fs.map(|f| f.validate()).transpose()?;

    for (index, rule) in permissions.command.iter().enumerate() {
        validate_command_rule(rule, index)?;
    }

    detect_command_rule_overlap(&permissions.command)?;

    Ok(PermissionsDef {
        opener,
        http,
        fs,
        command: permissions.command,
        website_metadata: permissions.website_metadata,
    })
}

// =========================================================
// Command rule validators (private to this module until
// moved to command.rs in commit 7)
// =========================================================

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
fn validate_command_rule(rule: &CommandPermissionDef, index: usize) -> anyhow::Result<()> {
    if rule.binary.is_empty() {
        anyhow::bail!("`[[permissions.command]]` rule {index} has empty `binary`");
    }
    if rule.binary.contains('\0') {
        anyhow::bail!("`[[permissions.command]]` rule {index} `binary` contains a NUL byte");
    }

    let mut saw_rest = false;
    for (argv_index, constraint) in rule.argv.iter().enumerate() {
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

    if let Some(cwd) = &rule.cwd {
        validate_variable_references(cwd, "cwd", index)?;
    }

    Ok(())
}

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
