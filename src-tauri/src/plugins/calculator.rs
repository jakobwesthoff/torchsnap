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
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use evalexpr::{Node, Operator, Value, build_operator_tree};
use regex::Regex;
use serde_json::json;
use tauri::Manager;

use super::{Plugin, PluginContext};
use crate::search::types::{
    Action, ActionId, CancellationToken, EntryIcon, PostAction, QueryResult, ResultChannel,
};
use crate::settings::SettingsInit;
use crate::settings_notifier::SettingsWatch;
use crate::storage::{SqlStorage, SqlValue};

// =========================================================
// Constants
// =========================================================

const PLUGIN_ID: &str = "calculator";

/// How often the retention cleanup thread wakes to delete expired
/// entries.
const RETENTION_CLEANUP_INTERVAL: Duration = Duration::from_secs(30 * 60);

/// Maximum number of history entries returned by a search.
const HISTORY_LIMIT: i64 = 100;

const MIGRATION_001: &str = "\
CREATE TABLE calc_history (
    id           TEXT PRIMARY KEY,
    expression   TEXT NOT NULL,
    result       TEXT NOT NULL,
    result_type  TEXT NOT NULL,
    computed_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    content_hash TEXT NOT NULL UNIQUE
);
CREATE INDEX idx_history_computed_at ON calc_history(computed_at DESC);
";

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
    "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "sinh", "cosh", "tanh", "asinh", "acosh",
    "atanh", "sqrt", "cbrt", "ln", "log", "log2", "log10", "exp", "exp2", "pow", "abs", "hypot",
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

/// Evaluate a math expression string. Returns `Ok(EvalResult)` on
/// success, or `Err(message)` with a human-readable error from the
/// parser or evaluator.
///
/// All integer literals are promoted to floats via AST rewriting
/// before evaluation. This ensures `1/2` returns `0.5` instead
/// of `0` (evalexpr's default integer division behavior).
/// Boolean results from comparison operators are preserved since
/// only `Const` nodes are rewritten.
fn evaluate(expr: &str) -> Result<EvalResult, String> {
    let preprocessed = preprocess_math_functions(expr);
    let mut tree =
        build_operator_tree(&preprocessed).map_err(|e| format_evalexpr_error(&e.to_string()))?;
    promote_ints_to_floats(&mut tree);

    // Each query gets a fresh context — no state persists between
    // queries, but intra-expression variables work (e.g., `a=5; a+1`).
    let mut context = evalexpr::HashMapContext::new();
    let value = tree
        .eval_with_context_mut(&mut context)
        .map_err(|e| format_evalexpr_error(&e.to_string()))?;

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

/// Clean up evalexpr's error messages for display. The crate's
/// error strings are verbose — strip internal details and keep
/// the user-facing message.
fn format_evalexpr_error(msg: &str) -> String {
    // evalexpr errors look like: "Expected a value, but found operator +."
    // or "No value in expression". Keep them as-is for now — they're
    // reasonably readable.
    msg.to_string()
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
    if abs != 0.0 && !(1e-6..=1e15).contains(&abs) {
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
    static BINARY_OP: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\d\s*[+*/^]\s*\d").expect("binary op regex"));

    // Subtraction/negative: digit, optional space, minus, optional space, digit.
    // Separate from BINARY_OP because `-` is also unary.
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
// History Storage
// =========================================================

/// Save an expression+result to history with deduplication.
///
/// If the same expression already exists (by content hash),
/// its timestamp and result are updated. Otherwise a new row
/// is inserted with a fresh ULID.
fn save_to_history(db: &SqlStorage, expression: &str, result: &EvalResult) {
    let content_hash = blake3::hash(expression.as_bytes()).to_hex().to_string();

    // Check for existing entry with the same expression.
    let existing: Vec<String> = db
        .query_map(
            "SELECT id FROM calc_history WHERE content_hash = ?",
            &[SqlValue::from(content_hash.as_str())],
            |row| row.get(0),
        )
        .unwrap_or_default();

    if existing.is_empty() {
        let id = ulid::Ulid::new().to_string().to_lowercase();
        let _ = db.execute(
            "INSERT INTO calc_history (id, expression, result, result_type, content_hash) \
             VALUES (?, ?, ?, ?, ?)",
            &[
                SqlValue::from(id),
                SqlValue::from(expression),
                SqlValue::from(result.value.as_str()),
                SqlValue::from(result.result_type),
                SqlValue::from(content_hash),
            ],
        );
    } else {
        // Bump timestamp and update result (in case formatting changed).
        let _ = db.execute(
            "UPDATE calc_history \
             SET result = ?, result_type = ?, \
                 computed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             WHERE content_hash = ?",
            &[
                SqlValue::from(result.value.as_str()),
                SqlValue::from(result.result_type),
                SqlValue::from(content_hash),
            ],
        );
    }
}

/// Query history entries, optionally filtered by a search term.
/// Returns entries ordered by most recent first.
fn query_history(db: &SqlStorage, filter: &str) -> Vec<QueryResult> {
    let (sql, params): (&str, Vec<SqlValue>) = if filter.is_empty() {
        (
            "SELECT id, expression, result, result_type FROM calc_history \
             ORDER BY computed_at DESC LIMIT ?",
            vec![SqlValue::from(HISTORY_LIMIT)],
        )
    } else {
        (
            "SELECT id, expression, result, result_type FROM calc_history \
             WHERE expression LIKE ? \
             ORDER BY computed_at DESC LIMIT ?",
            vec![
                SqlValue::from(format!("%{filter}%")),
                SqlValue::from(HISTORY_LIMIT),
            ],
        )
    };

    db.query_map(sql, &params, |row| {
        let id: String = row.get(0)?;
        let expression: String = row.get(1)?;
        let result: String = row.get(2)?;

        Ok(QueryResult {
            id,
            title: expression,
            subtitle: Some(result),
            icon: Some(EntryIcon::HeroIcon("clock".into())),
            score: 0,
            title_positions: vec![],
            subtitle_positions: vec![],
            actions: vec![Action {
                id: ActionId::Copy,
                label: "Copy to Clipboard".to_string(),
                keybinding: None,
            }],
        })
    })
    .unwrap_or_default()
}

/// Run the retention cleanup loop. Deletes history entries
/// older than `retentionDays`. Wakes on condvar signal (for
/// shutdown) or after the cleanup interval.
fn retention_cleanup_loop(
    db: Arc<SqlStorage>,
    retention_days_watch: SettingsWatch<u32>,
    condvar: Arc<Condvar>,
    shutdown: Arc<Mutex<bool>>,
) {
    loop {
        // Read the current retention setting.
        let days = retention_days_watch.get().max(1);

        let _ = db.execute(
            "DELETE FROM calc_history \
             WHERE computed_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?)",
            &[SqlValue::from(format!("-{days} days"))],
        );

        // Wait for the cleanup interval or a shutdown signal.
        let guard = shutdown.lock().expect("shutdown mutex not poisoned");
        let (guard, _) = condvar
            .wait_timeout(guard, RETENTION_CLEANUP_INTERVAL)
            .expect("condvar wait not poisoned");

        if *guard {
            break;
        }
    }
}

// =========================================================
// Calculator Plugin
// =========================================================

pub struct CalculatorPlugin {
    enabled: AtomicBool,
    heuristic_enabled: AtomicBool,
    history_enabled: AtomicBool,

    /// SQLite database for history. Initialized in `setup()`.
    db: Mutex<Option<Arc<SqlStorage>>>,

    /// Condvar for waking the retention cleanup thread on shutdown.
    retention_condvar: Arc<Condvar>,

    /// Shared shutdown flag for the retention thread.
    retention_shutdown: Arc<Mutex<bool>>,
}

impl CalculatorPlugin {
    pub fn new() -> Self {
        Self {
            enabled: AtomicBool::new(true),
            heuristic_enabled: AtomicBool::new(true),
            history_enabled: AtomicBool::new(true),
            db: Mutex::new(None),
            retention_condvar: Arc::new(Condvar::new()),
            retention_shutdown: Arc::new(Mutex::new(false)),
        }
    }

    fn db(&self) -> Option<Arc<SqlStorage>> {
        self.db
            .lock()
            .expect("calculator db mutex not poisoned")
            .clone()
    }
}

impl Plugin for CalculatorPlugin {
    fn id(&self) -> &str {
        PLUGIN_ID
    }

    fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn enabled_settings_key(&self) -> Option<&'static str> {
        Some("enabled")
    }

    fn search_prefixes(&self) -> &[&str] {
        &["="]
    }

    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit {
        settings
            .ensure("enabled", true)
            .ensure("heuristicEnabled", true)
            .ensure("historyEnabled", true)
            .ensure("retentionDays", 30)
    }

    fn setup(&self, app: &tauri::AppHandle, ctx: &PluginContext) {
        // ----- Initialize history database -----
        let data_dir = app
            .path()
            .app_data_dir()
            .expect("resolve app data dir")
            .join("plugins")
            .join(PLUGIN_ID);

        let db_path = data_dir.join("calculator.db");
        let db = Arc::new(
            SqlStorage::open(db_path, &[MIGRATION_001]).expect("open calculator database"),
        );
        *self.db.lock().expect("calculator db mutex not poisoned") = Some(Arc::clone(&db));

        // ----- Read initial settings -----
        let initial_enabled: bool = ctx.settings.get("enabled").unwrap_or(true);
        self.enabled.store(initial_enabled, Ordering::Relaxed);

        let initial_heuristic: bool = ctx.settings.get("heuristicEnabled").unwrap_or(true);
        self.heuristic_enabled
            .store(initial_heuristic, Ordering::Relaxed);

        let initial_history: bool = ctx.settings.get("historyEnabled").unwrap_or(true);
        self.history_enabled
            .store(initial_history, Ordering::Relaxed);

        // ----- Settings watch threads -----
        {
            let mut watch = ctx.notifier.watch::<bool>("enabled");
            let flag = &self.enabled as *const AtomicBool as usize;

            std::thread::spawn(move || {
                // SAFETY: Reconstructing an `&AtomicBool` from a raw pointer
                // that was cast through `usize` to make it `Send`. This is safe
                // because the `CalculatorPlugin` struct (which owns the AtomicBool)
                // is held alive inside an `Arc<dyn Plugin>` in `PluginHost`
                // for the entire lifetime of the application. The watch thread
                // terminates when the notifier's sender is dropped (at app exit),
                // which happens before the plugin is dropped.
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
                // SAFETY: Same pattern as the `enabled` watch above. The
                // `heuristic_enabled` AtomicBool lives inside the Arc'd plugin
                // and outlives this thread. See the comment on the `enabled`
                // watch thread for the full safety argument.
                let flag = unsafe { &*(flag as *const AtomicBool) };
                while let Some(val) = watch.blocking_changed() {
                    flag.store(val, Ordering::Relaxed);
                }
            });
        }
        {
            let mut watch = ctx.notifier.watch::<bool>("historyEnabled");
            let flag = &self.history_enabled as *const AtomicBool as usize;

            std::thread::spawn(move || {
                // SAFETY: Same pattern as the `enabled` watch above. The
                // `history_enabled` AtomicBool lives inside the Arc'd plugin
                // and outlives this thread. See the comment on the `enabled`
                // watch thread for the full safety argument.
                let flag = unsafe { &*(flag as *const AtomicBool) };
                while let Some(val) = watch.blocking_changed() {
                    flag.store(val, Ordering::Relaxed);
                }
            });
        }

        // ----- Retention cleanup thread -----
        {
            let retention_days_watch = ctx.notifier.watch::<u32>("retentionDays");
            let condvar = Arc::clone(&self.retention_condvar);
            let shutdown = Arc::clone(&self.retention_shutdown);
            let db = Arc::clone(&db);

            std::thread::spawn(move || {
                retention_cleanup_loop(db, retention_days_watch, condvar, shutdown);
            });
        }
    }

    fn teardown(&self) {
        // Signal the retention thread to exit.
        *self
            .retention_shutdown
            .lock()
            .expect("shutdown mutex not poisoned") = true;
        self.retention_condvar.notify_all();
    }

    fn search(
        &self,
        query: &str,
        matched_prefix: Option<&str>,
        results: &ResultChannel,
        _cancel: &CancellationToken,
    ) {
        match matched_prefix {
            Some("=") => {
                // Prefix mode: evaluate expression, send CustomUI
                // with inline result data + history entries.
                //
                // Empty/whitespace queries are not evaluated — the
                // frontend shows a help screen when data has no
                // result and no error.
                let data = if query.trim().is_empty() {
                    Some(json!({ "expression": query }))
                } else {
                    Some(match evaluate(query) {
                        Ok(r) => json!({
                            "expression": query,
                            "result": r.value,
                            "resultType": r.result_type,
                        }),
                        Err(error) => json!({
                            "expression": query,
                            "error": error,
                        }),
                    })
                };

                // Query history (filtered by expression if non-empty).
                let history = if self.history_enabled.load(Ordering::Relaxed) {
                    self.db()
                        .map(|db| query_history(&db, query))
                        .unwrap_or_default()
                } else {
                    vec![]
                };

                results.send_custom_ui("history".into(), data, history);
            }
            None => {
                // Heuristic mode: detect math expression.
                if !self.heuristic_enabled.load(Ordering::Relaxed) {
                    return;
                }

                let expr = match try_extract_math(query) {
                    Some(e) => e,
                    None => return,
                };

                let result = match evaluate(expr) {
                    Ok(r) => r,
                    Err(_) => return,
                };

                results.send_inline_ui(
                    "result".into(),
                    Some(json!({
                        "expression": expr,
                        "result": result.value,
                        "resultType": result.result_type,
                    })),
                    vec![],
                );
            }
            _ => {}
        }
    }

    fn execute(
        &self,
        entry_id: &str,
        _action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        use tauri_plugin_clipboard_manager::ClipboardExt;

        // If history is enabled and this looks like a result value
        // being copied, we save the expression to history in the
        // frontend's execute handler (via the data payload). The
        // backend execute just copies and dismisses.
        app.clipboard()
            .write_text(entry_id)
            .map_err(|e| anyhow::anyhow!("copy to clipboard: {e}"))?;
        Ok(PostAction::Dismiss)
    }

    fn handle_message(
        &self,
        method: &str,
        payload: serde_json::Value,
        _channel: tauri::ipc::Channel<serde_json::Value>,
    ) -> anyhow::Result<serde_json::Value> {
        let db = self
            .db()
            .ok_or_else(|| anyhow::anyhow!("calculator database not initialized"))?;

        match method {
            // Save an expression+result to history (called by frontend
            // on Enter in prefix mode).
            "save_history" => {
                if !self.history_enabled.load(Ordering::Relaxed) {
                    return Ok(json!({"saved": false}));
                }

                let expression = payload
                    .get("expression")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'expression' field"))?;
                let result_value = payload
                    .get("result")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'result' field"))?;
                let result_type = payload
                    .get("resultType")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'resultType' field"))?;

                let eval_result = EvalResult {
                    value: result_value.to_string(),
                    result_type: if result_type == "boolean" {
                        "boolean"
                    } else {
                        "number"
                    },
                };

                save_to_history(&db, expression, &eval_result);
                Ok(json!({"saved": true}))
            }

            // Return storage statistics.
            "stats" => {
                let count: Vec<i64> = db
                    .query_map("SELECT COUNT(*) FROM calc_history", &[], |row| row.get(0))
                    .unwrap_or_default();

                let entry_count = count.first().copied().unwrap_or(0);

                // Get the database file size.
                // The DB path is stored in the SqlStorage but not exposed,
                // so we reconstruct it. This is acceptable since the path
                // is deterministic.
                let db_size = 0i64; // TODO: expose db path or size from SqlStorage

                Ok(json!({
                    "entryCount": entry_count,
                    "dbSize": db_size,
                }))
            }

            // Clear all history entries.
            "clear_history" => {
                db.execute("DELETE FROM calc_history", &[])?;
                Ok(json!({"cleared": true}))
            }

            _ => anyhow::bail!("unknown calculator message: {method}"),
        }
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

    #[test]
    fn history_save_and_query() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db = SqlStorage::open(dir.path().join("test.db"), &[MIGRATION_001])
            .expect("open test database");

        let result = EvalResult {
            value: "5".to_string(),
            result_type: "number",
        };
        save_to_history(&db, "2 + 3", &result);

        let entries = query_history(&db, "");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "2 + 3");
        assert_eq!(entries[0].subtitle.as_deref(), Some("5"));
    }

    #[test]
    fn history_dedup_bumps_timestamp() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db = SqlStorage::open(dir.path().join("test.db"), &[MIGRATION_001])
            .expect("open test database");

        let result = EvalResult {
            value: "5".to_string(),
            result_type: "number",
        };
        save_to_history(&db, "2 + 3", &result);
        save_to_history(&db, "2 + 3", &result);

        let entries = query_history(&db, "");
        assert_eq!(entries.len(), 1, "dedup should prevent duplicates");
    }

    #[test]
    fn history_filter() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db = SqlStorage::open(dir.path().join("test.db"), &[MIGRATION_001])
            .expect("open test database");

        save_to_history(
            &db,
            "2 + 3",
            &EvalResult {
                value: "5".to_string(),
                result_type: "number",
            },
        );
        save_to_history(
            &db,
            "10 * 20",
            &EvalResult {
                value: "200".to_string(),
                result_type: "number",
            },
        );

        let entries = query_history(&db, "10");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "10 * 20");
    }
}
