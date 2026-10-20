// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::paths::{ParseTimeResolver, PathResolver};

/// `[[permissions.command]]` rule — a single binary +
/// argv-shape pattern the gadget is permitted to invoke
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
/// gadgets wanting "open with the registered application"
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
    /// The binary the rule grants: an absolute path, or a bare
    /// name looked up on the child's `PATH`. Validated at
    /// manifest parse time (non-empty, no NUL bytes, no relative
    /// path with a separator).
    pub binary: String,

    /// Per-position argv constraints. Empty means the
    /// rule grants `binary` with zero arguments.
    #[serde(default)]
    pub argv: Vec<ArgvConstraint>,

    /// Root of the working directories this rule allows. A call
    /// without a `cwd` runs in the root itself; a call with one
    /// must name the root or a directory below it. Without a
    /// rule `cwd`, a call cannot choose a working directory and
    /// the program runs in an empty directory the host creates
    /// for that one spawn.
    pub cwd: Option<String>,

    /// Environment variables a call may set, each with the
    /// constraint its value must meet (the argv kinds except
    /// `rest`). A call may set no other variable.
    ///
    /// ```toml
    /// [permissions.command.env]
    /// GIT_AUTHOR_NAME = { kind = "regex", pattern = "[A-Za-z ]{1,64}" }
    /// ```
    #[serde(default)]
    pub env: BTreeMap<String, ArgvConstraint>,

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

// ─── Domain conversions ──────────────────────────────────

impl From<Vec<CommandPermissionDef>> for crate::caps::CommandPermissions {
    fn from(rules: Vec<CommandPermissionDef>) -> Self {
        Self { rules }
    }
}

impl From<Vec<CommandPermissionDef>> for crate::caps::CapRequest {
    fn from(rules: Vec<CommandPermissionDef>) -> Self {
        Self::Command {
            permissions: rules.into(),
        }
    }
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
        // A relative path such as `bin/tool` would be resolved against
        // the working directory, which the guest may choose, so it could
        // pick which program runs. macOS-specific: only `/` starts an
        // absolute path; other platforms need their own rule (todo
        // `01M5DN71VBN0H0AVSQT2GMDK49-decide-how-command-confinement-works-on-linux-and-windows.md`).
        let is_bare_name =
            !self.binary.contains(['/', '\\']) && self.binary != "." && self.binary != "..";
        if !(self.binary.starts_with('/') || is_bare_name) {
            anyhow::bail!(
                "Command rule {index}: the binary \"{}\" must be an absolute path or a \
                 bare program name",
                self.binary
            );
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
            validate_argv_constraint(
                constraint,
                &format!("rule {index} argv[{argv_index}]"),
                false,
            )?;
            validate_argv_location(constraint, index, argv_index)?;
            if matches!(constraint, ArgvConstraint::Rest { .. }) {
                saw_rest = true;
            }
        }

        if let Some(cwd) = &self.cwd {
            validate_rule_root(cwd, index, "the working directory", ValueUse::Configuration)?;
        }

        for (key, constraint) in &self.env {
            validate_env_declaration(key, constraint, index)?;
        }

        // A zero ceiling clamps every call to nothing: no time to run,
        // no output kept, no stdin accepted. Leave the key out to use
        // the host default instead.
        let ceilings = [
            ("timeout-ms-max", self.timeout_ms_max.map(u64::from)),
            ("max-output-bytes", self.max_output_bytes),
            ("max-stdin-bytes", self.max_stdin_bytes),
        ];
        for (field, value) in ceilings {
            if value == Some(0) {
                anyhow::bail!(
                    "`[[permissions.command]]` rule {index} `{field}` is 0, which allows \
                     no invocation to succeed; use a positive limit or remove the key"
                );
            }
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
///   canonicalizes under `root`. `root` may use the
///   substitution variables `${gadget-data}`,
///   `${gadget-archive}`, `${home}`, `${xdg-config}`,
///   `${xdg-data}`.
/// - `any-string`  — argv element accepted unconditionally.
/// - `rest`        — applies `constraint` to every remaining argv
///   element. May only appear at the trailing position.
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
// `path-under`, and per-rule `cwd` fields are validated via
// `crate::paths::ParseTimeResolver` and resolved at runtime
// through `crate::paths::PathResolver` implementations.

// =========================================================
// Internal validators
// =========================================================

/// What a rule value is for, which decides the variables naming areas
/// the gadget can write that it may not contain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueUse {
    /// A working directory or an environment value: neither
    /// `${gadget-data}` nor the gadget's package `${gadget-archive}`.
    Configuration,
    /// A program argument: not `${gadget-data}`. Arguments may name
    /// files in the package, such as a script a granted interpreter
    /// runs; the review shows them as written.
    Argument,
}

/// Reject a value that names a directory the gadget can write through
/// a variable. Parsing cannot know where the other variables resolve;
/// the host checks the resolved values when it loads the gadget.
fn reject_writable_variables(
    value: &str,
    rule_index: usize,
    field: &str,
    use_: ValueUse,
) -> anyhow::Result<()> {
    let variables: &[&str] = match use_ {
        ValueUse::Configuration => &["${gadget-data}", "${gadget-archive}"],
        ValueUse::Argument => &["${gadget-data}"],
    };
    if let Some(variable) = variables.iter().find(|variable| value.contains(**variable)) {
        anyhow::bail!(
            "Command rule {rule_index}: {field} \"{value}\" uses {variable}, an area the \
             gadget can write"
        );
    }
    Ok(())
}

/// Parse-time checks of a fixed (`literal` or `enum`) value: no
/// variable naming an area the gadget can write, and no `..` in a path
/// inside it, so the path means the same before and after symlinks
/// are resolved.
fn validate_fixed_value_location(
    value: &str,
    rule_index: usize,
    field: &str,
    use_: ValueUse,
) -> anyhow::Result<()> {
    reject_writable_variables(value, rule_index, field, use_)?;
    for token in path_tokens(value) {
        if token.split('/').any(|segment| segment == "..") {
            anyhow::bail!(
                "Command rule {rule_index}: {field} \"{value}\" contains a path with \"..\""
            );
        }
    }
    Ok(())
}

/// The location checks of one argv constraint, `rest` unwrapped.
fn validate_argv_location(
    constraint: &ArgvConstraint,
    rule_index: usize,
    argv_index: usize,
) -> anyhow::Result<()> {
    let field = format!("argument {argv_index}");
    match constraint {
        ArgvConstraint::Rest { constraint } => {
            validate_argv_location(constraint, rule_index, argv_index)
        }
        ArgvConstraint::PathUnder { root } => validate_rule_root(
            root,
            rule_index,
            &format!("the path-under root of {field}"),
            ValueUse::Argument,
        ),
        ArgvConstraint::Literal { value } => {
            validate_fixed_value_location(value, rule_index, &field, ValueUse::Argument)
        }
        ArgvConstraint::Enum { values } => values.iter().try_for_each(|value| {
            validate_fixed_value_location(value, rule_index, &field, ValueUse::Argument)
        }),
        ArgvConstraint::Glob { .. } | ArgvConstraint::Regex { .. } | ArgvConstraint::AnyString => {
            Ok(())
        }
    }
}

/// Parse-time checks of a directory a rule names as a root: it must
/// be non-empty, start with `/` or a `${...}` variable (every variable
/// resolves to an absolute directory), and contain no `..` segment, so
/// the root means the same before and after symlinks are resolved.
///
/// macOS-specific: only `/`-anchored paths with `/` separators are
/// accepted. Other platforms need their own choice (todo
/// `01M5DN71VBN0H0AVSQT2GMDK49-decide-how-command-confinement-works-on-linux-and-windows.md`).
fn validate_rule_root(
    root: &str,
    rule_index: usize,
    field: &str,
    use_: ValueUse,
) -> anyhow::Result<()> {
    reject_writable_variables(root, rule_index, field, use_)?;
    if root.is_empty() {
        anyhow::bail!("Command rule {rule_index}: {field} is empty");
    }
    if !(root.starts_with('/') || root.starts_with("${")) {
        anyhow::bail!(
            "Command rule {rule_index}: {field} \"{root}\" is not absolute; start it \
             with / or a ${{...}} variable"
        );
    }
    if root.split('/').any(|segment| segment == "..") {
        anyhow::bail!("Command rule {rule_index}: {field} \"{root}\" contains \"..\"");
    }
    ParseTimeResolver
        .validate_variable_references(root)
        .map_err(|e| anyhow::anyhow!("rule {rule_index} {field}: {e}"))
}

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
/// references are recognized but not substituted; regex and
/// glob patterns are compile-tested but the compiled form is
/// discarded (the matcher recompiles when it builds its
/// compiled rule set).
fn validate_argv_constraint(
    constraint: &ArgvConstraint,
    where_: &str,
    nested_in_rest: bool,
) -> anyhow::Result<()> {
    match constraint {
        ArgvConstraint::Literal { value } => {
            ParseTimeResolver
                .validate_variable_references(value)
                .map_err(|e| anyhow::anyhow!("{where_}.value: {e}"))?;
        }
        ArgvConstraint::Enum { values } => {
            if values.is_empty() {
                anyhow::bail!("{where_} `enum` constraint has empty `values` list");
            }
            for v in values {
                ParseTimeResolver
                    .validate_variable_references(v)
                    .map_err(|e| anyhow::anyhow!("{where_}.values: {e}"))?;
            }
        }
        ArgvConstraint::Glob { pattern } => {
            if pattern.is_empty() {
                anyhow::bail!("{where_} `glob` constraint has empty `pattern`");
            }
            // Same builder the matcher uses, so a pattern that parses
            // here also compiles there.
            globset::Glob::new(pattern).map_err(|e| {
                anyhow::anyhow!("{where_} `glob` pattern `{pattern}` does not compile: {e}")
            })?;
        }
        ArgvConstraint::Regex { pattern } => {
            // Anchor before compile-checking so gadget authors
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
            ParseTimeResolver
                .validate_variable_references(root)
                .map_err(|e| anyhow::anyhow!("{where_}.root: {e}"))?;
        }
        ArgvConstraint::AnyString => {}
        ArgvConstraint::Rest { constraint } => {
            if nested_in_rest {
                anyhow::bail!(
                    "{where_} declares a `rest` constraint nested inside another `rest` — \
                     `rest` may only appear at the top level"
                );
            }
            validate_argv_constraint(constraint, where_, true)?;
        }
    }
    Ok(())
}

// =========================================================
// Declared environment
// =========================================================

/// Keys a rule can never declare, compared case-insensitively. They
/// make a program load or run other code: the dynamic loader, `PATH`
/// lookups, interpreter startup hooks, and programs a tool starts on
/// its own (pagers, editors, ssh, askpass helpers, git config from the
/// environment). `HOME` and `TMPDIR` are the spawn's own empty
/// directories; declaring them would undo that. The list is defense in
/// depth on top of declared, reviewed keys, not the guarantee.
const REJECTED_ENV_KEYS: &[&str] = &[
    "PATH",
    "HOME",
    "TMPDIR",
    "NODE_OPTIONS",
    "NODE_PATH",
    "PYTHONPATH",
    "PYTHONSTARTUP",
    "PYTHONHOME",
    "PERL5LIB",
    "PERL5OPT",
    "RUBYOPT",
    "RUBYLIB",
    "JAVA_TOOL_OPTIONS",
    "BASH_ENV",
    "ENV",
    "IFS",
    "ZDOTDIR",
    "PROMPT_COMMAND",
    "SHELL",
    "PAGER",
    "LESSOPEN",
    "EDITOR",
    "VISUAL",
    "BROWSER",
    "SSH_ASKPASS",
    "GIT_SSH",
    "GIT_SSH_COMMAND",
    "GIT_EXTERNAL_DIFF",
    "GIT_PAGER",
    "GIT_EDITOR",
    "GIT_SEQUENCE_EDITOR",
    "GIT_ASKPASS",
    "GIT_PROXY_COMMAND",
    "GIT_EXEC_PATH",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_PARAMETERS",
];

/// Key prefixes a rule can never declare, compared case-insensitively.
/// `DYLD_` is the macOS dynamic loader and `LD_` its Linux twin; other
/// platforms need their own entries (todo
/// `01M5DN71VBN0H0AVSQT2GMDK49-decide-how-command-confinement-works-on-linux-and-windows.md`).
/// `GIT_CONFIG_KEY_<n>` and `GIT_CONFIG_VALUE_<n>` set git configuration
/// such as `core.pager`, and `npm_config_*` sets npm's.
const REJECTED_ENV_KEY_PREFIXES: &[&str] = &[
    "LD_",
    "DYLD_",
    "GIT_CONFIG_KEY_",
    "GIT_CONFIG_VALUE_",
    "NPM_CONFIG_",
];

/// Parse-time checks of one `[permissions.command.env]` entry.
fn validate_env_declaration(
    key: &str,
    constraint: &ArgvConstraint,
    rule_index: usize,
) -> anyhow::Result<()> {
    let where_ = format!("Command rule {rule_index}: environment variable {key}");
    if key.is_empty() {
        anyhow::bail!("Command rule {rule_index}: an environment variable has an empty name");
    }
    if key.contains('\0') {
        anyhow::bail!(
            "Command rule {rule_index}: the environment variable name \"{key}\" contains a NUL byte"
        );
    }
    if key.contains('=') {
        anyhow::bail!(
            "Command rule {rule_index}: the environment variable name \"{key}\" contains \"=\""
        );
    }
    let upper = key.to_ascii_uppercase();
    if REJECTED_ENV_KEYS.contains(&upper.as_str())
        || REJECTED_ENV_KEY_PREFIXES
            .iter()
            .any(|prefix| upper.starts_with(prefix))
    {
        anyhow::bail!(
            "Command rule {rule_index}: the environment variable {key} cannot be declared, \
             since it makes the program load or run other code"
        );
    }

    match constraint {
        ArgvConstraint::Rest { .. } => {
            anyhow::bail!(
                "Command rule {rule_index}: rest is not a value constraint, as used for {key}"
            );
        }
        ArgvConstraint::PathUnder { root } => {
            validate_rule_root(
                root,
                rule_index,
                &format!("the path-under root of {key}"),
                ValueUse::Configuration,
            )?;
        }
        ArgvConstraint::Literal { value } => {
            validate_fixed_env_value(value, &where_)?;
            validate_fixed_value_location(
                value,
                rule_index,
                &format!("the value of {key}"),
                ValueUse::Configuration,
            )?;
        }
        ArgvConstraint::Enum { values } => {
            for value in values {
                validate_fixed_env_value(value, &where_)?;
                validate_fixed_value_location(
                    value,
                    rule_index,
                    &format!("the value of {key}"),
                    ValueUse::Configuration,
                )?;
            }
        }
        ArgvConstraint::Glob { .. } | ArgvConstraint::Regex { .. } | ArgvConstraint::AnyString => {}
    }
    validate_argv_constraint(constraint, &where_, false)
}

/// A fixed env value names a path only absolutely: a relative path
/// would be resolved against whatever directory the program runs in.
/// The value is checked token by token, so a path inside a list or
/// after `--option=` counts too. A URL with a scheme other than `file`
/// names no file.
fn validate_fixed_env_value(value: &str, where_: &str) -> anyhow::Result<()> {
    for token in path_tokens(value) {
        if token.contains('/') && !token.starts_with('/') && !token.starts_with("${") {
            anyhow::bail!(
                "{where_}: the value \"{value}\" contains the relative path \"{token}\"; use an \
                 absolute path or a ${{...}} variable"
            );
        }
    }
    Ok(())
}

/// The parts of `value` that may be paths: the value split at the
/// separators lists and options use (`:`, `;`, `,`, `=`, whitespace).
/// A `file://` URL contributes its path; a URL of any other scheme
/// contributes nothing.
///
/// macOS-specific: splitting at `:` fits Unix path lists but would cut
/// a Windows drive letter off its path. Other platforms need their own
/// separators (todo
/// `01M5DN71VBN0H0AVSQT2GMDK49-decide-how-command-confinement-works-on-linux-and-windows.md`).
pub(crate) fn path_tokens(value: &str) -> Vec<&str> {
    if let Some((scheme, rest)) = value.split_once("://")
        && !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return if scheme.eq_ignore_ascii_case("file") {
            vec![rest]
        } else {
            Vec::new()
        };
    }
    value
        .split(|c: char| matches!(c, ':' | ';' | ',' | '=') || c.is_ascii_whitespace())
        .filter(|token| !token.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::{CapRequest, CommandPermissions};
    use crate::wasm::manifest::Manifest;
    use crate::wasm::manifest::test_helpers::minimal;

    // ─── Domain conversions ─────────────────────────────────

    #[test]
    fn into_command_permissions_maps_rules() {
        let rules = vec![CommandPermissionDef {
            binary: "/usr/bin/echo".into(),
            argv: vec![],
            cwd: None,
            timeout_ms_max: None,
            max_output_bytes: None,
            max_stdin_bytes: None,
            env: Default::default(),
        }];
        let perms: CommandPermissions = rules.into();
        assert_eq!(perms.rules.len(), 1);
        assert_eq!(perms.rules[0].binary, "/usr/bin/echo");
    }

    #[test]
    fn into_command_permissions_empty_rules() {
        let rules: Vec<CommandPermissionDef> = vec![];
        let perms: CommandPermissions = rules.into();
        assert!(perms.rules.is_empty());
    }

    #[test]
    fn into_cap_request_produces_command_variant() {
        let rules = vec![CommandPermissionDef {
            binary: "ls".into(),
            argv: vec![ArgvConstraint::AnyString],
            cwd: Some("/tmp".into()),
            timeout_ms_max: Some(5000),
            max_output_bytes: Some(1024),
            max_stdin_bytes: None,
            env: Default::default(),
        }];
        let req: CapRequest = rules.into();
        match req {
            CapRequest::Command { permissions } => {
                assert_eq!(permissions.rules.len(), 1);
                assert_eq!(permissions.rules[0].binary, "ls");
                assert_eq!(permissions.rules[0].timeout_ms_max, Some(5000));
            }
            _ => panic!("expected Command variant"),
        }
    }

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
                   { kind = "path-under", root = "${home}/repos" },
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
    fn reject_relative_binary_with_separator() {
        for binary in [
            "bin/tool",
            "./run",
            "node_modules/.bin/eslint",
            "..",
            ".",
            "bin\\\\tool",
        ] {
            let err = Manifest::parse(&minimal(&format!(
                "[[permissions.command]]\nbinary = \"{binary}\""
            )))
            .expect_err(binary);
            assert!(
                err.to_string()
                    .contains("absolute path or a bare program name"),
                "{binary}: {err}"
            );
        }
    }

    #[test]
    fn accept_absolute_and_bare_binaries() {
        for binary in ["/usr/bin/git", "git", "osascript", "zerotier-cli"] {
            Manifest::parse(&minimal(&format!(
                "[[permissions.command]]\nbinary = \"{binary}\""
            )))
            .unwrap_or_else(|e| panic!("{binary}: {e}"));
        }
    }

    #[test]
    fn reject_empty_rule_cwd() {
        let err = Manifest::parse(&minimal(
            "[[permissions.command]]\nbinary = \"git\"\ncwd = \"\"",
        ))
        .unwrap_err();
        assert!(
            err.to_string().contains("the working directory is empty"),
            "{err}"
        );
    }

    #[test]
    fn reject_relative_rule_cwd() {
        for cwd in ["projects", "./projects", "home/${home}"] {
            let err = Manifest::parse(&minimal(&format!(
                "[[permissions.command]]\nbinary = \"git\"\ncwd = \"{cwd}\""
            )))
            .expect_err(cwd);
            assert!(err.to_string().contains("is not absolute"), "{cwd}: {err}");
        }
    }

    #[test]
    fn reject_rule_cwd_with_dotdot() {
        for cwd in ["${home}/projects/..", "/Users/me/../other", "${home}/../x"] {
            let err = Manifest::parse(&minimal(&format!(
                "[[permissions.command]]\nbinary = \"git\"\ncwd = \"{cwd}\""
            )))
            .expect_err(cwd);
            assert!(err.to_string().contains("contains \"..\""), "{cwd}: {err}");
        }
    }

    #[test]
    fn accept_anchored_rule_cwd() {
        for cwd in ["${home}/projects", "/Volumes/Work", "${xdg-data}/notes"] {
            Manifest::parse(&minimal(&format!(
                "[[permissions.command]]\nbinary = \"git\"\ncwd = \"{cwd}\""
            )))
            .unwrap_or_else(|e| panic!("{cwd}: {e}"));
        }
    }

    // ─── declared environment ───────────────────────────────

    fn parse_env(declaration: &str) -> anyhow::Result<Manifest> {
        Manifest::parse(&minimal(&format!(
            "[[permissions.command]]\nbinary = \"git\"\n[permissions.command.env]\n{declaration}\n"
        )))
    }

    /// Every key of `keys` that the parser accepts as a declaration.
    fn accepted_keys<'a>(keys: &[&'a str]) -> Vec<&'a str> {
        keys.iter()
            .copied()
            .filter(|key| parse_env(&format!("\"{key}\" = {{ kind = \"any-string\" }}")).is_ok())
            .collect()
    }

    #[test]
    fn accept_declared_env() {
        let m = parse_env(
            r#"GIT_AUTHOR_NAME = { kind = "regex", pattern = "[A-Za-z ]{1,64}" }
               LC_ALL = { kind = "enum", values = ["C", "en_US.UTF-8"] }"#,
        )
        .expect("should parse");
        let env = &m.permissions.unwrap().command[0].env;
        assert_eq!(
            env.keys().collect::<Vec<_>>(),
            vec!["GIT_AUTHOR_NAME", "LC_ALL"]
        );
    }

    #[test]
    fn reject_env_key_with_loader_prefix() {
        let keys = [
            "LD_PRELOAD",
            "LD_LIBRARY_PATH",
            "LD_AUDIT",
            "DYLD_INSERT_LIBRARIES",
            "DYLD_FALLBACK_LIBRARY_PATH",
            "ld_preload",
            "Dyld_Insert_Libraries",
        ];
        assert_eq!(accepted_keys(&keys), Vec::<&str>::new());
    }

    #[test]
    fn reject_env_key_path_home_and_tmpdir() {
        let keys = ["PATH", "Path", "HOME", "home", "TMPDIR"];
        assert_eq!(accepted_keys(&keys), Vec::<&str>::new());
    }

    #[test]
    fn reject_interpreter_and_tool_hijack_keys() {
        let keys = [
            "NODE_OPTIONS",
            "PYTHONPATH",
            "PYTHONSTARTUP",
            "PERL5LIB",
            "PERL5OPT",
            "RUBYOPT",
            "RUBYLIB",
            "BASH_ENV",
            "ENV",
            "IFS",
            "GIT_SSH",
            "GIT_SSH_COMMAND",
            "GIT_EXTERNAL_DIFF",
            "GIT_PAGER",
            "PAGER",
            "LESSOPEN",
            "EDITOR",
            "VISUAL",
            "BROWSER",
            "SHELL",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_KEY_0",
            "GIT_CONFIG_VALUE_12",
            "GIT_CONFIG_PARAMETERS",
            "GIT_ASKPASS",
            "SSH_ASKPASS",
            "GIT_EDITOR",
            "GIT_SEQUENCE_EDITOR",
            "GIT_PROXY_COMMAND",
            "GIT_EXEC_PATH",
            "PYTHONHOME",
            "NODE_PATH",
            "JAVA_TOOL_OPTIONS",
            "ZDOTDIR",
            "PROMPT_COMMAND",
            "npm_config_script_shell",
            "NPM_CONFIG_NODE_OPTIONS",
        ];
        assert_eq!(accepted_keys(&keys), Vec::<&str>::new());
    }

    #[test]
    fn reject_empty_env_key() {
        let err = parse_env(r#""" = { kind = "any-string" }"#).unwrap_err();
        assert!(err.to_string().contains("has an empty name"), "{err}");
    }

    #[test]
    fn reject_env_key_with_equals() {
        let err = parse_env(r#""A=B" = { kind = "any-string" }"#).unwrap_err();
        assert!(err.to_string().contains("contains \"=\""), "{err}");
    }

    #[test]
    fn reject_env_key_with_nul() {
        let err = parse_env("\"A\\u0000B\" = { kind = \"any-string\" }").unwrap_err();
        assert!(err.to_string().contains("NUL"), "{err}");
    }

    #[test]
    fn reject_rest_env_constraint() {
        let err = parse_env(r#"FOO = { kind = "rest", constraint = { kind = "any-string" } }"#)
            .unwrap_err();
        assert!(
            err.to_string().contains("rest is not a value constraint"),
            "{err}"
        );
    }

    #[test]
    fn reject_env_constraint_with_bad_regex() {
        let err = parse_env(r#"FOO = { kind = "regex", pattern = "[unclosed" }"#).unwrap_err();
        assert!(err.to_string().contains("does not compile"), "{err}");
    }

    #[test]
    fn reject_relative_literal_env_path() {
        for declaration in [
            r#"FOO = { kind = "literal", value = "conf/app.toml" }"#,
            r#"FOO = { kind = "enum", values = ["/etc/app.toml", "conf/app.toml"] }"#,
            r#"FOO = { kind = "literal", value = "/etc/a.toml:conf/b.toml" }"#,
            r#"FOO = { kind = "literal", value = "--config=conf/app.toml" }"#,
            r#"FOO = { kind = "literal", value = "file://conf/app.toml" }"#,
        ] {
            let err = parse_env(declaration).expect_err(declaration);
            assert!(
                err.to_string().contains("relative path"),
                "{declaration}: {err}"
            );
        }
    }

    #[test]
    fn reject_env_path_under_with_a_relative_root() {
        let err = parse_env(r#"FOO = { kind = "path-under", root = "projects" }"#).unwrap_err();
        assert!(err.to_string().contains("not absolute"), "{err}");
    }

    #[test]
    fn accept_absolute_literal_env_paths_and_plain_values() {
        parse_env(
            r#"A = { kind = "literal", value = "/etc/app.toml" }
               B = { kind = "literal", value = "${home}/app.toml" }
               C = { kind = "literal", value = "https://proxy.example.com:8080/" }
               E = { kind = "literal", value = "file:///etc/app.toml" }
               D = { kind = "enum", values = ["C", "en_US.UTF-8"] }"#,
        )
        .expect("should parse");
    }

    // ─── nothing in the gadget's writable areas ─────────────

    fn parse_rule(body: &str) -> anyhow::Result<Manifest> {
        Manifest::parse(&minimal(&format!(
            "[[permissions.command]]\nbinary = \"git\"\n{body}\n"
        )))
    }

    /// Every rule body of `bodies` that the parser accepts.
    fn accepted_rules<'a>(bodies: &[&'a str]) -> Vec<&'a str> {
        bodies
            .iter()
            .copied()
            .filter(|body| parse_rule(body).is_ok())
            .collect()
    }

    #[test]
    fn reject_rule_cwd_in_gadget_data_or_package() {
        let bodies = [
            r#"cwd = "${gadget-data}""#,
            r#"cwd = "${gadget-data}/repos""#,
            r#"cwd = "${gadget-archive}""#,
            r#"cwd = "${gadget-archive}/scripts""#,
        ];
        assert_eq!(accepted_rules(&bodies), Vec::<&str>::new());
    }

    #[test]
    fn reject_path_under_root_in_gadget_data() {
        let bodies = [
            r#"argv = [{ kind = "path-under", root = "${gadget-data}" }]"#,
            r#"argv = [{ kind = "rest", constraint = { kind = "path-under", root = "${gadget-data}/x" } }]"#,
            "[permissions.command.env]\nGIT_DIR = { kind = \"path-under\", root = \"${gadget-data}\" }",
            "[permissions.command.env]\nGIT_DIR = { kind = \"path-under\", root = \"${gadget-archive}\" }",
        ];
        assert_eq!(accepted_rules(&bodies), Vec::<&str>::new());
    }

    #[test]
    fn reject_argv_path_under_root_that_is_relative_or_has_dotdot() {
        let bodies = [
            r#"argv = [{ kind = "path-under", root = "repos" }]"#,
            r#"argv = [{ kind = "path-under", root = "${home}/repos/.." }]"#,
            r#"argv = [{ kind = "rest", constraint = { kind = "path-under", root = "/a/../b" } }]"#,
        ];
        assert_eq!(accepted_rules(&bodies), Vec::<&str>::new());
    }

    #[test]
    fn reject_literal_values_in_gadget_data() {
        let bodies = [
            r#"argv = [{ kind = "literal", value = "${gadget-data}/Makefile" }]"#,
            r#"argv = [{ kind = "literal", value = "--file=${gadget-data}/x" }]"#,
            r#"argv = [{ kind = "enum", values = ["-C", "${gadget-data}"] }]"#,
            r#"argv = [{ kind = "rest", constraint = { kind = "literal", value = "${gadget-data}" } }]"#,
            "[permissions.command.env]\nGIT_CONFIG_GLOBAL = { kind = \"literal\", value = \"${gadget-data}/gitconfig\" }",
            "[permissions.command.env]\nGIT_CONFIG_GLOBAL = { kind = \"literal\", value = \"${gadget-archive}/gitconfig\" }",
            "[permissions.command.env]\nX = { kind = \"enum\", values = [\"a\", \"/etc:${gadget-data}\"] }",
        ];
        assert_eq!(accepted_rules(&bodies), Vec::<&str>::new());
    }

    #[test]
    fn reject_literal_values_with_a_dotdot_path() {
        let bodies = [
            r#"argv = [{ kind = "literal", value = "/etc/../Users/x" }]"#,
            r#"argv = [{ kind = "literal", value = "--file=${home}/a/../b" }]"#,
            "[permissions.command.env]\nX = { kind = \"literal\", value = \"/usr/share/../lib\" }",
        ];
        assert_eq!(accepted_rules(&bodies), Vec::<&str>::new());
    }

    /// The package may hold scripts a granted interpreter runs, and the
    /// review shows such an argument as it is written.
    #[test]
    fn accept_argv_values_in_the_gadget_package() {
        let bodies = [
            r#"argv = [{ kind = "literal", value = "${gadget-archive}/scripts/run.py" }]"#,
            r#"argv = [{ kind = "path-under", root = "${gadget-archive}" }]"#,
        ];
        assert_eq!(accepted_rules(&bodies), bodies.to_vec());
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
    fn reject_command_rule_with_bad_glob() {
        for argv in [
            r#"[{ kind = "glob", pattern = "[unclosed" }]"#,
            r#"[{ kind = "rest", constraint = { kind = "glob", pattern = "{a,b" } }]"#,
        ] {
            let err = Manifest::parse(&minimal(&format!(
                "[[permissions.command]]\nbinary = \"git\"\nargv = {argv}"
            )))
            .expect_err(argv);
            let message = err.to_string();
            assert!(message.contains("rule 0 argv[0]"), "{argv}: {message}");
            assert!(message.contains("does not compile"), "{argv}: {message}");
        }
    }

    #[test]
    fn accept_command_rule_with_valid_glob() {
        Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "glob", pattern = "refs/heads/*" }]"#,
        ))
        .expect("should parse");
    }

    #[test]
    fn reject_command_rule_with_zero_ceiling() {
        for field in ["timeout-ms-max", "max-output-bytes", "max-stdin-bytes"] {
            let err = Manifest::parse(&minimal(&format!(
                "[[permissions.command]]\nbinary = \"git\"\n{field} = 0"
            )))
            .expect_err(field);
            let message = err.to_string();
            assert!(message.contains("rule 0"), "{field}: {message}");
            assert!(
                message.contains(&format!("`{field}` is 0")),
                "{field}: {message}"
            );
        }
    }

    #[test]
    fn accept_command_rule_with_minimal_ceilings() {
        Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               timeout-ms-max = 1
               max-output-bytes = 1
               max-stdin-bytes = 1"#,
        ))
        .expect("should parse");
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
               argv = [{ kind = "literal", value = "${gadget-typo}" }]"#,
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

    /// `${gadget-data}` is rejected in values; see
    /// `reject_literal_values_in_gadget_data`.
    #[test]
    fn accept_recognized_variables_in_literal() {
        for var in &["gadget-archive", "home", "xdg-config", "xdg-data"] {
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
