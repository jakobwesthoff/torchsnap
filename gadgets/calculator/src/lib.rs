// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Calculator Gadget (WASM port)
//
// Evaluates math expressions inline using `evalexpr`.
//
// Two modes:
//
// - Prefix mode (`=`): exclusive routing, full custom UI
//   with a history list below the inline result. The gadget
//   returns `CustomUI { view = "history" }` from `search()`
//   and the frontend's `CalculatorView` component renders
//   it.
//
// - Heuristic mode (no prefix): the gadget's `search()`
//   detects math-looking queries via regex heuristics and
//   returns `InlineUI { view = "result" }` so the frontend's
//   `CalculatorInline` component renders an inline result
//   above the standard search list.
//
// History is persisted to per-gadget SQLite via the WIT
// `sql` host import. Retention cleanup runs as a scheduled
// background task declared in `manifest.toml` — no
// dedicated cleanup thread (which would be impossible
// anyway since `wasm32-wasip2` has no thread support).
//
// Settings (`heuristicEnabled`, `historyEnabled`,
// `retentionDays`) are read at `enable()` time and refreshed
// reactively via `on_setting_changed`.
// =========================================================

use std::cell::Cell;
use std::sync::LazyLock;

use evalexpr::{Node, Operator, Value, build_operator_tree};
use regex::Regex;
use serde::Deserialize;
use serde_json::json;
use torchsnap_gadget_sdk::prelude::*;
use torchsnap_gadget_sdk::sql::{SqlHandle, SqlValue};

// =========================================================
// Constants
// =========================================================

/// Maximum number of history entries returned by a search.
const HISTORY_LIMIT: i64 = 100;

/// Math functions that evalexpr requires a `math::` prefix
/// for. The preprocessor below adds the prefix automatically
/// so users can write `sqrt(144)` instead of
/// `math::sqrt(144)`. Functions like `floor`, `ceil`, `round`,
/// `min`, `max` work without the prefix and are not listed
/// here.
const MATH_PREFIX_FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "sinh", "cosh", "tanh", "asinh", "acosh",
    "atanh", "sqrt", "cbrt", "ln", "log", "log2", "log10", "exp", "exp2", "pow", "abs", "hypot",
];

// =========================================================
// Reactive setting cache
//
// `wasm32-wasip2` is single-threaded, so cell-based interior
// mutability is sufficient — there is no second thread that
// could observe a torn write. The `enable` lifecycle method
// seeds these from the host's settings store; the
// `on_setting_changed` lifecycle method refreshes them when
// the user toggles a setting.
// =========================================================

thread_local! {
    static HEURISTIC_ENABLED: Cell<bool> = const { Cell::new(true) };
    static HISTORY_ENABLED: Cell<bool> = const { Cell::new(true) };
    static RETENTION_DAYS: Cell<u32> = const { Cell::new(30) };
}

// =========================================================
// Gadget trait wiring
// =========================================================

struct CalculatorPlugin;
define_gadget!(CalculatorPlugin);

impl LifecycleGuest for CalculatorPlugin {
    fn enable() {
        // Seed the in-memory cache from the settings store
        // (or from the manifest defaults if the store has no
        // user values yet). On_setting_changed below
        // refreshes these reactively when the user toggles
        // a control in the settings panel.
        HEURISTIC_ENABLED.with(|c| c.set(settings::get_or("heuristicEnabled", true)));
        HISTORY_ENABLED.with(|c| c.set(settings::get_or("historyEnabled", true)));
        RETENTION_DAYS.with(|c| c.set(settings::get_or::<u32>("retentionDays", 30)));

        logging::log(logging::LogLevel::Info, "Calculator enabled", &[], None);
    }

    fn disable() {
        logging::log(logging::LogLevel::Info, "Calculator disabled", &[], None);
    }

    fn on_setting_changed(key: String, value: String) {
        match key.as_str() {
            "heuristicEnabled" => {
                if let Ok(v) = serde_json::from_str::<bool>(&value) {
                    HEURISTIC_ENABLED.with(|c| c.set(v));
                }
            }
            "historyEnabled" => {
                if let Ok(v) = serde_json::from_str::<bool>(&value) {
                    HISTORY_ENABLED.with(|c| c.set(v));
                }
            }
            "retentionDays" => {
                if let Ok(v) = serde_json::from_str::<u32>(&value) {
                    RETENTION_DAYS.with(|c| c.set(v));
                }
            }
            _ => {}
        }
    }
}

impl SearchGuest for CalculatorPlugin {
    fn entries() -> Vec<CatalogEntry> {
        // Calculator is purely query-driven — no static
        // catalog entries. The host calls this once at
        // startup; returning an empty list keeps it out of
        // the always-on result list.
        vec![]
    }

    fn search(query: String, matched_prefix: Option<String>) -> SearchResponse {
        match matched_prefix.as_deref() {
            Some("=") => prefix_mode_search(&query),
            None => heuristic_mode_search(&query),
            _ => SearchResponse::Nothing,
        }
    }

    fn execute(entry: ScoredEntry, _action_id: ActionId) -> Result<PostAction, String> {
        // The launcher passes the result string as the
        // entry id when it executes a copy action — both
        // the inline view (`onExecute(calcData.result, ...)`)
        // and the prefix-mode view (`onExecute(resultToCopy,
        // ...)` after Enter on a history row) hand us the
        // text to copy. Write it to the system clipboard
        // via the WIT host import, then dismiss the
        // launcher.
        clipboard::write_text(&entry.id).map_err(|e| format!("copy to clipboard: {e}"))?;
        Ok(PostAction::Dismiss)
    }
}

// =========================================================
// Search mode dispatch
// =========================================================

/// Prefix-mode search (`=` typed). Evaluates the expression,
/// queries history (if enabled), and returns a `CustomUI`
/// response so the frontend's `CalculatorView` component
/// takes over the result area.
fn prefix_mode_search(query: &str) -> SearchResponse {
    // Empty/whitespace queries are not evaluated — the
    // frontend renders a help screen when `data` has no
    // result and no error.
    let data = if query.trim().is_empty() {
        Some(json!({ "expression": query }).to_string())
    } else {
        let payload = match evaluate(query) {
            Ok(r) => json!({
                "expression": query,
                "result": r.value,
                "resultType": r.result_type,
            }),
            Err(error) => json!({
                "expression": query,
                "error": error,
            }),
        };
        Some(payload.to_string())
    };

    // Query history filtered by the current input. If
    // history is disabled or obtaining a connection fails,
    // fall back to an empty list — the inline result still
    // renders.
    let history = if HISTORY_ENABLED.with(Cell::get) {
        let db = sql::connection();
        query_history(&db, query)
    } else {
        vec![]
    };

    SearchResponse::CustomUi(ViewResponse {
        view: "history".into(),
        data,
        results: history,
    })
}

/// Heuristic-mode search (no prefix). Detects math-looking
/// queries via regex and returns an `InlineUI` response so
/// the frontend's `CalculatorInline` component renders an
/// inline result above the standard search list. Queries
/// that fail the heuristic or fail evaluation pass through
/// silently as `Nothing`.
fn heuristic_mode_search(query: &str) -> SearchResponse {
    if !HEURISTIC_ENABLED.with(Cell::get) {
        return SearchResponse::Nothing;
    }

    let Some(expr) = try_extract_math(query) else {
        return SearchResponse::Nothing;
    };

    let Ok(result) = evaluate(expr) else {
        return SearchResponse::Nothing;
    };

    SearchResponse::InlineUi(ViewResponse {
        view: "result".into(),
        data: Some(
            json!({
                "expression": expr,
                "result": result.value,
                "resultType": result.result_type,
            })
            .to_string(),
        ),
        results: vec![],
    })
}

// =========================================================
// Frontend ↔ gadget messaging
// =========================================================

impl MessagingGuest for CalculatorPlugin {
    fn handle_message(method: String, payload: String) -> Result<String, String> {
        match method.as_str() {
            // Save an expression+result to history. Called
            // by the frontend on Enter in prefix mode (and
            // when clicking a previous history entry).
            "save_history" => save_history_method(&payload),

            // Storage statistics — entry count + database
            // size. The frontend's settings panel displays
            // these in its Statistics section.
            "stats" => stats_method(),

            // Wipe the history table. Frontend asks for
            // confirmation before sending this.
            "clear_history" => clear_history_method(),

            other => Err(format!("unknown calculator message: {other}")),
        }
    }
}

/// Frontend payload for `save_history`. Field names match
/// the JSON keys the `CalculatorView` component sends.
#[derive(Deserialize)]
struct SaveHistoryPayload {
    expression: String,
    result: String,
    #[serde(rename = "resultType")]
    result_type: String,
}

fn save_history_method(payload: &str) -> Result<String, String> {
    if !HISTORY_ENABLED.with(Cell::get) {
        return Ok(json!({ "saved": false }).to_string());
    }

    let req: SaveHistoryPayload = messaging::parse_payload(payload)?;
    let db = sql::connection();
    save_to_history(
        &db,
        &req.expression,
        &EvalResult {
            value: req.result,
            // Anything other than an explicit `"boolean"` flag
            // is rendered numerically — the frontend only
            // distinguishes these two kinds.
            result_type: if req.result_type == "boolean" {
                "boolean"
            } else {
                "number"
            },
        },
    )?;

    Ok(json!({ "saved": true }).to_string())
}

fn stats_method() -> Result<String, String> {
    let db = sql::connection();
    let rows = db
        .query("SELECT COUNT(*) FROM calc_history", &[])
        .map_err(|e| format!("count: {e}"))?;
    let entry_count = match rows.first().and_then(|row| row.first()) {
        Some(SqlValue::Integer(n)) => *n,
        _ => 0,
    };

    // dbSize is reported as 0 for now — the WIT sql
    // interface doesn't expose the file size, and computing
    // it would require an additional WASI fs capability.
    Ok(json!({
        "entryCount": entry_count,
        "dbSize": 0,
    })
    .to_string())
}

fn clear_history_method() -> Result<String, String> {
    let db = sql::connection();
    db.execute("DELETE FROM calc_history", &[])
        .map_err(|e| format!("delete: {e}"))?;
    Ok(json!({ "cleared": true }).to_string())
}

// =========================================================
// Scheduled tasks (retention cleanup)
// =========================================================

impl TasksGuest for CalculatorPlugin {
    fn run_task(task_id: String) -> Result<(), String> {
        match task_id.as_str() {
            "retention-cleanup" => cleanup_expired_history(),
            other => Err(format!("unknown task: {other}")),
        }
    }
}

/// Delete history rows older than the configured retention
/// period. Called by the host's scheduler at the cadence
/// declared in `manifest.toml`'s `[[tasks]]` block.
fn cleanup_expired_history() -> Result<(), String> {
    let days = RETENTION_DAYS.with(Cell::get).max(1);
    let db = sql::connection();
    db.execute(
        "DELETE FROM calc_history \
         WHERE computed_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?)",
        &[SqlValue::Text(format!("-{days} days"))],
    )
    .map_err(|e| format!("delete expired: {e}"))?;
    Ok(())
}

// =========================================================
// Expression evaluator
// =========================================================

/// The result of evaluating a math expression, ready for
/// frontend display.
struct EvalResult {
    /// Formatted display value (`"0.5"`, `"true"`, `"42"`).
    value: String,
    /// Type tag for frontend styling: `"number"` or
    /// `"boolean"`.
    result_type: &'static str,
}

/// Evaluate a math expression. Returns `Ok(EvalResult)` on
/// success or `Err(message)` with a human-readable error
/// from the parser or evaluator.
///
/// Integer literals are promoted to floats via AST rewriting
/// before evaluation so `1 / 2` returns `0.5` instead of
/// `0` (evalexpr's default integer division). Boolean
/// results from comparison operators are preserved because
/// only `Const { Value::Int }` nodes are rewritten — the
/// comparison operators themselves are untouched.
fn evaluate(expr: &str) -> Result<EvalResult, String> {
    let preprocessed = preprocess_math_functions(expr);
    let mut tree = build_operator_tree(&preprocessed).map_err(|e| e.to_string())?;
    promote_ints_to_floats(&mut tree);

    // Each query gets a fresh context — no state persists
    // between queries, but intra-expression variables work
    // (e.g., `a = 5; a + 1`).
    let mut context = evalexpr::HashMapContext::new();
    let value = tree
        .eval_with_context_mut(&mut context)
        .map_err(|e| e.to_string())?;

    match value {
        Value::Float(f) => Ok(EvalResult {
            value: format_float(f),
            result_type: "number",
        }),
        Value::Int(i) => Ok(EvalResult {
            value: i.to_string(),
            result_type: "number",
        }),
        Value::Boolean(b) => Ok(EvalResult {
            value: b.to_string(),
            result_type: "boolean",
        }),
        // String, Tuple, Empty — unsupported for display.
        _ => Err("unsupported result type".to_string()),
    }
}

/// Preprocess an expression to add `math::` prefix to known
/// math function names. The list is hard-coded because
/// evalexpr doesn't expose its math function set
/// programmatically.
fn preprocess_math_functions(expr: &str) -> String {
    static MATH_FN_RE: LazyLock<Regex> = LazyLock::new(|| {
        let pattern = MATH_PREFIX_FUNCTIONS.join("|");
        Regex::new(&format!(r"(?i)\b({pattern})\s*\("))
            .expect("math function preprocess regex")
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

/// Walk the parsed AST and replace all integer constants
/// with float equivalents. This forces float arithmetic
/// throughout the expression, preventing integer division
/// truncation.
fn promote_ints_to_floats(node: &mut Node) {
    for op in node.iter_operators_mut() {
        if let Operator::Const {
            value: Value::Int(i),
        } = op
        {
            *op = Operator::Const {
                value: Value::Float(*i as f64),
            };
        }
    }
}

/// Format a float for display:
/// - Integer values (no fractional part) → no decimal point
/// - Scientific notation for very large (>10^15) or very
///   small (<10^-6) magnitudes
/// - Otherwise → up to 10 significant digits, trailing
///   zeros stripped
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
    if abs != 0.0 && !(1e-6..=1e15).contains(&abs) {
        return format!("{f:.6e}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string();
    }

    // Integer display when there's no fractional part.
    if f.fract() == 0.0 && abs < 1e15 {
        return format!("{f:.0}");
    }

    // General decimal: up to 10 significant digits, strip
    // trailing zeros.
    let s = format!("{f:.10}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

// =========================================================
// Heuristic math detection
// =========================================================

/// Check whether a query string looks like a math expression
/// that should trigger the calculator in heuristic mode.
/// Returns the original query (the full string) if any of
/// the heuristics fire; `None` otherwise.
fn try_extract_math(query: &str) -> Option<&str> {
    // Two numeric operands with an arithmetic operator
    // between them. Excludes comparison chars to avoid
    // overlap with COMPARISON_OP below.
    static BINARY_OP: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\d\s*[+*/^]\s*\d").expect("binary op regex"));

    // Subtraction/negative: digit, optional space, minus,
    // optional space, digit. Separate from BINARY_OP because
    // `-` is also unary.
    static SUBTRACTION: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\d\s*-\s*\d").expect("subtraction regex"));

    // Comparison operators between numeric operands.
    static COMPARISON_OP: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\d\s*(>=|<=|==|!=|[><])\s*\d").expect("comparison op regex"));

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
// History storage
// =========================================================

/// Save an expression+result to history with deduplication.
///
/// If the same expression already exists (by content hash),
/// its timestamp and result are bumped. Otherwise a new row
/// is inserted with a fresh ULID.
fn save_to_history(db: &SqlHandle, expression: &str, result: &EvalResult) -> Result<(), String> {
    let content_hash = blake3::hash(expression.as_bytes()).to_hex().to_string();

    // Check for an existing entry with the same expression.
    let existing = db
        .query(
            "SELECT id FROM calc_history WHERE content_hash = ?",
            &[SqlValue::Text(content_hash.clone())],
        )
        .map_err(|e| format!("dedup query: {e}"))?;

    if existing.is_empty() {
        let id = ulid::Ulid::new().to_string().to_lowercase();
        db.execute(
            "INSERT INTO calc_history (id, expression, result, result_type, content_hash) \
             VALUES (?, ?, ?, ?, ?)",
            &[
                SqlValue::Text(id),
                SqlValue::Text(expression.to_string()),
                SqlValue::Text(result.value.clone()),
                SqlValue::Text(result.result_type.to_string()),
                SqlValue::Text(content_hash),
            ],
        )
        .map_err(|e| format!("insert: {e}"))?;
    } else {
        db.execute(
            "UPDATE calc_history \
             SET result = ?, result_type = ?, \
                 computed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             WHERE content_hash = ?",
            &[
                SqlValue::Text(result.value.clone()),
                SqlValue::Text(result.result_type.to_string()),
                SqlValue::Text(content_hash),
            ],
        )
        .map_err(|e| format!("update: {e}"))?;
    }

    Ok(())
}

/// Query history entries, optionally filtered by an
/// expression substring. Returns entries ordered by most
/// recent first.
fn query_history(db: &SqlHandle, filter: &str) -> Vec<ScoredEntry> {
    // We intentionally don't fetch `result_type` here. The
    // host's `ScoredEntry` has no metadata field to carry it
    // across the WIT boundary, so any data we read would just
    // be discarded. This means boolean history entries render
    // with the default "number" styling in the history list
    // — a minor visual regression vs the native calculator
    // that will resolve when `ScoredEntry` gains a metadata
    // field. The inline result area (typed query, not
    // history) still displays the correct styling because the
    // `data` payload of the CustomUI response carries
    // `resultType` directly.
    let (sql_text, params) = if filter.is_empty() {
        (
            "SELECT id, expression, result FROM calc_history \
             ORDER BY computed_at DESC LIMIT ?"
                .to_string(),
            vec![SqlValue::Integer(HISTORY_LIMIT)],
        )
    } else {
        (
            "SELECT id, expression, result FROM calc_history \
             WHERE expression LIKE ? \
             ORDER BY computed_at DESC LIMIT ?"
                .to_string(),
            vec![
                SqlValue::Text(format!("%{filter}%")),
                SqlValue::Integer(HISTORY_LIMIT),
            ],
        )
    };

    let rows = match db.query(&sql_text, &params) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    rows.into_iter()
        .filter_map(|columns| {
            // Each row carries id, expression, result as Text columns.
            let id = expect_text(columns.first())?;
            let expression = expect_text(columns.get(1))?;
            let result = expect_text(columns.get(2))?;
            Some(ScoredEntry {
                id,
                title: expression,
                subtitle: Some(result),
                icon: Some(EntryIcon::HeroIcon("clock".into())),
                score: 0,
                title_highlight_positions: vec![],
                subtitle_highlight_positions: vec![],
                actions: vec![Action {
                    id: ActionId::Copy,
                    label: "Copy to Clipboard".to_string(),
                }],
                data: None,
            })
        })
        .collect()
}

fn expect_text(value: Option<&SqlValue>) -> Option<String> {
    match value? {
        SqlValue::Text(s) => Some(s.clone()),
        _ => None,
    }
}

// =========================================================
// Tests — pure compute paths only
//
// Storage and settings tests live in the integration test
// fixture (still pending) — those need a real wasmtime host
// to wire up the WIT imports. The tests below cover the
// expression evaluator and the heuristic detector, which
// are pure functions with no host dependencies.
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
        assert!(evaluate("3+").is_err());
    }

    #[test]
    fn parse_error() {
        assert!(evaluate("abc").is_err());
    }

    #[test]
    fn scientific_notation_large() {
        let r = evaluate("10^16").unwrap();
        assert!(
            r.value.contains('e'),
            "expected scientific notation: {}",
            r.value
        );
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
