// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Calculator Plugin
//
// Evaluates math expressions inline using the `evalexpr` crate.
// Operates in two modes:
//
// - Prefix mode (`=`): exclusive routing, full custom UI with
//   history list below the result.
// - Heuristic mode (no prefix): detects math expressions in
//   the query and shows an inline result above the regular
//   result list.
//
// The evaluator wraps `evalexpr` behind a thin API so the
// crate is swappable if needed. Integer-to-float promotion
// via AST rewriting prevents integer division truncation.
// =========================================================

use std::sync::atomic::{AtomicBool, Ordering};

use evalexpr::{Node, Operator, Value, build_operator_tree};
use regex::Regex;
use serde_json::json;

use super::{PluginContext, QueryPlugin};
use crate::search::types::{ActionId, PostAction, SearchResponse};
use crate::settings::SettingsInit;

// =========================================================
// Evaluation Result
// =========================================================

/// The result of evaluating a math expression, ready for
/// serialization to the frontend.
struct EvalResult {
    /// Formatted display value (e.g., "0.5", "true", "42").
    value: String,
    /// Type tag for frontend styling: "number" or "boolean".
    result_type: &'static str,
}

// =========================================================
// Expression Evaluator
// =========================================================

/// Math functions that evalexpr requires a `math::` prefix for.
/// Functions like `floor`, `ceil`, `round`, `min`, `max` work
/// without the prefix, but trig, log, sqrt, etc. need it.
const MATH_PREFIX_FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "sinh", "cosh", "tanh", "asinh",
    "acosh", "atanh", "sqrt", "cbrt", "ln", "log", "log2", "log10", "exp", "exp2", "pow", "abs",
    "hypot",
];

/// Preprocess an expression to add `math::` prefix to known math
/// function names. This lets users write `sqrt(144)` instead of
/// `math::sqrt(144)`.
fn preprocess_math_functions(expr: &str) -> String {
    use std::sync::LazyLock;

    static MATH_FN_RE: LazyLock<Regex> = LazyLock::new(|| {
        let pattern = MATH_PREFIX_FUNCTIONS.join("|");
        Regex::new(&format!(r"(?i)\b({pattern})\s*\(")).expect("math function preprocess regex")
    });

    MATH_FN_RE
        .replace_all(expr, |caps: &regex::Captures| {
            let name = caps[1].to_lowercase();
            let full_match = &caps[0];
            // Preserve the opening paren and any whitespace.
            let paren_part = &full_match[caps[1].len()..];
            format!("math::{name}{paren_part}")
        })
        .into_owned()
}

/// Evaluate a math expression string, returning a formatted
/// result or `None` for parse errors, incomplete expressions,
/// or unsupported result types.
///
/// All integer literals are promoted to floats via AST rewriting
/// before evaluation. This ensures `1/2` returns `0.5` instead
/// of `0` (evalexpr's default integer division behavior).
/// Boolean results from comparison operators are preserved since
/// only `Const` nodes are rewritten.
fn evaluate(expr: &str) -> Option<EvalResult> {
    let preprocessed = preprocess_math_functions(expr);
    let mut tree = build_operator_tree(&preprocessed).ok()?;
    promote_ints_to_floats(&mut tree);

    // Each query gets a fresh context — no state persists between
    // queries, but intra-expression variables work (e.g., `a=5; a+1`).
    let mut context = evalexpr::HashMapContext::new();
    let value = tree.eval_with_context_mut(&mut context).ok()?;

    match value {
        Value::Float(f) => Some(EvalResult {
            value: format_float(f),
            result_type: "number",
        }),
        Value::Int(i) => Some(EvalResult {
            value: i.to_string(),
            result_type: "number",
        }),
        Value::Boolean(b) => Some(EvalResult {
            value: b.to_string(),
            result_type: "boolean",
        }),
        // String, Tuple, Empty — unsupported for display.
        _ => None,
    }
}

/// Walk the parsed AST and replace all integer constants with
/// float equivalents. This forces float arithmetic throughout
/// the expression, preventing integer division truncation.
///
/// Only `Const` nodes containing `Value::Int` are modified —
/// operators and function names are untouched, so boolean
/// results from comparisons are preserved.
fn promote_ints_to_floats(node: &mut Node) {
    for op in node.iter_operators_mut() {
        if let Operator::Const { value: Value::Int(i) } = op {
            *op = Operator::Const {
                value: Value::Float(*i as f64),
            };
        }
    }
}

/// Format a float for display:
/// - Integer values (no fractional part): display without decimal point.
/// - Scientific notation for very large (>10^15) or very small (<10^-6).
/// - Otherwise: decimal with trailing zeros stripped.
fn format_float(f: f64) -> String {
    if !f.is_finite() {
        return if f.is_nan() {
            "NaN".to_string()
        } else if f.is_sign_positive() {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        };
    }

    let abs = f.abs();

    // Scientific notation for extremes.
    if abs != 0.0 && (abs > 1e15 || abs < 1e-6) {
        return format!("{:.6e}", f)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string();
    }

    // Integer display when there's no fractional part.
    if f.fract() == 0.0 && abs < 1e15 {
        return format!("{:.0}", f);
    }

    // General decimal: up to 10 significant digits, strip trailing zeros.
    let s = format!("{:.10}", f);
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    trimmed.to_string()
}

// =========================================================
// Heuristic Detection
// =========================================================

/// Check whether a query string looks like a math expression
/// that should trigger the calculator in heuristic (prefix-free)
/// mode.
///
/// Returns the expression string if matched (the full query),
/// or `None` if the query doesn't look like math.
fn try_extract_math(query: &str) -> Option<&str> {
    // Lazy-initialized regexes. These are compiled once and
    // reused across calls.
    use std::sync::LazyLock;

    // Two numeric operands with an arithmetic operator between them.
    // Matches: `20+24`, `3 * 4`, `2^10`
    // The operator set excludes comparison chars to avoid overlap.
    static BINARY_OP: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\d\s*[+*/^]\s*\d").expect("binary op regex")
    });

    // Subtraction/negative: digit, optional space, minus, optional space, digit.
    // Separate from BINARY_OP because `-` is also unary.
    static SUBTRACTION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\d\s*-\s*\d").expect("subtraction regex")
    });

    // Comparison operators between numeric operands.
    static COMPARISON_OP: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\d\s*(>=|<=|==|!=|[><])\s*\d").expect("comparison op regex")
    });

    // Known math function followed by opening paren.
    static MATH_FN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\b(sin|cos|tan|asin|acos|atan|atan2|sinh|cosh|tanh|sqrt|cbrt|ln|log|log2|log10|exp|abs|floor|ceil|round|pow|min|max)\s*\(",
        )
        .expect("math function regex")
    });

    // Parenthesized expression containing an operator.
    static PAREN_EXPR: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\([^)]*[+\-*/^><=!][^)]*\)").expect("paren expression regex")
    });

    if BINARY_OP.is_match(query)
        || SUBTRACTION.is_match(query)
        || COMPARISON_OP.is_match(query)
        || MATH_FN.is_match(query)
        || PAREN_EXPR.is_match(query)
    {
        Some(query)
    } else {
        None
    }
}

// =========================================================
// Calculator Plugin
// =========================================================

pub struct CalculatorPlugin {
    enabled: AtomicBool,
    heuristic_enabled: AtomicBool,
}

impl CalculatorPlugin {
    pub fn new() -> Self {
        Self {
            enabled: AtomicBool::new(true),
            heuristic_enabled: AtomicBool::new(true),
        }
    }
}

impl QueryPlugin for CalculatorPlugin {
    fn id(&self) -> &str {
        "calculator"
    }

    fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn enabled_settings_key(&self) -> Option<&'static str> {
        Some("enabled")
    }

    fn prefixes(&self) -> &[&str] {
        &["="]
    }

    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit {
        settings
            .ensure("enabled", true)
            .ensure("heuristicEnabled", true)
            .ensure("historyEnabled", true)
            .ensure("retentionDays", 30)
    }

    fn setup(&self, _app: &tauri::AppHandle, ctx: &PluginContext) {
        // Read initial settings.
        let initial_enabled: bool = ctx.settings.get("enabled").unwrap_or(true);
        self.enabled.store(initial_enabled, Ordering::Relaxed);

        let initial_heuristic: bool = ctx.settings.get("heuristicEnabled").unwrap_or(true);
        self.heuristic_enabled
            .store(initial_heuristic, Ordering::Relaxed);

        // Watch each setting on its own thread. blocking_changed()
        // blocks until a new value arrives, which is the same pattern
        // the clipboard plugin uses for its enabled watch.
        {
            let mut watch = ctx.notifier.watch::<bool>("enabled");
            let flag = &self.enabled as *const AtomicBool as usize;

            std::thread::spawn(move || {
                // Safety: the plugin (and its AtomicBool) is held alive
                // by Arc in PluginHost for the lifetime of the app.
                let flag = unsafe { &*(flag as *const AtomicBool) };
                while let Some(val) = watch.blocking_changed() {
                    flag.store(val, Ordering::Relaxed);
                }
            });
        }
        {
            let mut watch = ctx.notifier.watch::<bool>("heuristicEnabled");
            let flag = &self.heuristic_enabled as *const AtomicBool as usize;

            std::thread::spawn(move || {
                let flag = unsafe { &*(flag as *const AtomicBool) };
                while let Some(val) = watch.blocking_changed() {
                    flag.store(val, Ordering::Relaxed);
                }
            });
        }
    }

    fn search(&self, query: &str, matched_prefix: Option<&str>) -> SearchResponse {
        match matched_prefix {
            Some("=") => {
                // Prefix mode: evaluate expression, return CustomUI.
                let eval_result = evaluate(query);

                let data = eval_result.as_ref().map(|r| {
                    json!({
                        "expression": query,
                        "result": r.value,
                        "resultType": r.result_type,
                    })
                });

                // TODO: History entries will be added in Layer 4.
                SearchResponse::CustomUI {
                    view: "history".into(),
                    data,
                    results: vec![],
                }
            }
            None => {
                // Heuristic mode: detect math expression.
                if !self.heuristic_enabled.load(Ordering::Relaxed) {
                    return SearchResponse::Nothing;
                }

                let expr = match try_extract_math(query) {
                    Some(e) => e,
                    None => return SearchResponse::Nothing,
                };

                let result = match evaluate(expr) {
                    Some(r) => r,
                    None => return SearchResponse::Nothing,
                };

                SearchResponse::InlineUI {
                    view: "result".into(),
                    data: Some(json!({
                        "expression": expr,
                        "result": result.value,
                        "resultType": result.result_type,
                    })),
                    results: vec![],
                }
            }
            _ => SearchResponse::Nothing,
        }
    }

    fn execute(
        &self,
        entry_id: &str,
        _action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        use tauri_plugin_clipboard_manager::ClipboardExt;
        app.clipboard()
            .write_text(entry_id)
            .map_err(|e| anyhow::anyhow!("copy to clipboard: {e}"))?;
        Ok(PostAction::Dismiss)
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_arithmetic() {
        let r = evaluate("2 + 3").unwrap();
        assert_eq!(r.value, "5");
        assert_eq!(r.result_type, "number");
    }

    #[test]
    fn float_division() {
        let r = evaluate("1 / 2").unwrap();
        assert_eq!(r.value, "0.5");
    }

    #[test]
    fn integer_display() {
        let r = evaluate("2^3").unwrap();
        assert_eq!(r.value, "8");
    }

    #[test]
    fn boolean_comparison() {
        let r = evaluate("4 > 2").unwrap();
        assert_eq!(r.value, "true");
        assert_eq!(r.result_type, "boolean");
    }

    #[test]
    fn intra_expression_variables() {
        let r = evaluate("a = 5; a + 1").unwrap();
        assert_eq!(r.value, "6");
    }

    #[test]
    fn math_function() {
        let r = evaluate("sqrt(144)").unwrap();
        assert_eq!(r.value, "12");
    }

    #[test]
    fn incomplete_expression() {
        assert!(evaluate("3+").is_none());
    }

    #[test]
    fn parse_error() {
        assert!(evaluate("abc").is_none());
    }

    #[test]
    fn scientific_notation_large() {
        let r = evaluate("10^16").unwrap();
        assert!(r.value.contains('e'), "expected scientific notation: {}", r.value);
    }

    #[test]
    fn heuristic_binary_op() {
        assert!(try_extract_math("20+24").is_some());
        assert!(try_extract_math("3 * 4").is_some());
    }

    #[test]
    fn heuristic_comparison() {
        assert!(try_extract_math("4 > 2").is_some());
        assert!(try_extract_math("1 == 1").is_some());
    }

    #[test]
    fn heuristic_math_fn() {
        assert!(try_extract_math("sin(20)").is_some());
        assert!(try_extract_math("sqrt(144)").is_some());
    }

    #[test]
    fn heuristic_paren_expr() {
        assert!(try_extract_math("(3+4)*2").is_some());
    }

    #[test]
    fn heuristic_no_match_bare_number() {
        assert!(try_extract_math("2024").is_none());
    }

    #[test]
    fn heuristic_no_match_unary_minus() {
        assert!(try_extract_math("-5").is_none());
    }

    #[test]
    fn heuristic_no_match_app_name() {
        assert!(try_extract_math("VS Code 2").is_none());
    }

    #[test]
    fn heuristic_no_match_version_string() {
        assert!(try_extract_math("v1.2.3").is_none());
    }
}
