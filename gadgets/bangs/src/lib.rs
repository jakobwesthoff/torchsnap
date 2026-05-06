// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Bangs Gadget (WASM)
//
// Detects DuckDuckGo bang patterns (e.g. `!g`, `!yt`,
// `!crates`) anywhere in the query string and surfaces a
// single result entry that opens the corresponding service
// URL with the remaining search terms.
//
// The bang database lives in the gadget's per-gadget SQL
// storage. First `enable()` after installation populates it
// from (a) the DDG bang.js endpoint when the network is
// reachable, (b) the `assets/bang.json` file bundled inside
// the `.torchsnap` archive as a fallback. Subsequent enables
// find the data already present and skip the import.
//
// The settings UI's "Refresh" button triggers a fresh network
// download on demand via the `refresh` messaging method.
// =========================================================

use std::cell::RefCell;

use serde::{Deserialize, Serialize};
use serde_json::json;

use torchsnap_gadget_sdk::http::{HttpMethod, HttpRequest};
use torchsnap_gadget_sdk::prelude::*;
use torchsnap_gadget_sdk::sql::{SqlHandle, SqlValue, query_all, query_one};

// =========================================================
// Pending-URL thread-local
//
// SAFETY INVARIANT (load-bearing):
//
//   `wasm32-wasip2` is single-threaded per guest instance,
//   so a `thread_local!` here is effectively per-instance
//   static storage. The host's UI flow writes via search()
//   and reads via execute() with no other caller that
//   touches the cell — search() runs synchronously per
//   query cycle, the UI can only execute the currently
//   displayed result, and the host serializes guest calls
//   on the store mutex as a belt-and-suspenders second
//   line of defense.
//
// Therefore the URL written by search() is always the URL
// the user sees when they trigger execute(). If either
// condition breaks (multi-threaded guest runtime, a
// background task that touches the cell, pipelined WIT
// calls that overlap), this mechanism breaks silently —
// execute() would open a stale or wrong URL.
//
// HACK(gadget-execute-data-param): see todos/gadget-host/api/01kn7v6ynyf580ax9jyyt25jgc-plugin-execute-data-param.md — remove once execute() carries an arbitrary data parameter.
//
// `RefCell<Option<String>>` rather than `Cell<Option<String>>`
// because `Option<String>` is not `Copy`. Calculator uses
// `Cell<bool>` / `Cell<u32>` which are Copy; we can't.
// =========================================================

thread_local! {
    static PENDING_URL: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Score assigned to bang results. High enough to appear
/// near the top (above mediocre fuzzy matches) but below a
/// perfect title match + heavy frecency. Matches the native
/// gadget's value 1:1.
const BANG_SCORE: u32 = 1000;

/// Path inside the gadget archive where the bundled bang
/// database lives. Loaded via `assets::read` as a fallback
/// when the network fetch fails.
const BUNDLED_BANG_PATH: &str = "assets/bang.json";

struct BangsPlugin;
define_gadget!(BangsPlugin);

// =========================================================
// Lifecycle
//
// `enable()` populates the bang DB on first run. `disable()`
// clears the pending-URL cell.
// =========================================================

impl LifecycleGuest for BangsPlugin {
    fn enable() {
        let db = sql::connection();
        match check_has_data(&db) {
            Ok(true) => {
                logging::log(
                    logging::LogLevel::Info,
                    "Bangs enabled (existing data in SQL store)",
                    &[],
                    None,
                );
            }
            Ok(false) => {
                // First launch after install, or the SQL
                // store was cleared. Try network first; fall
                // back to the bundled `bang.json`.
                import_from_best_source(&db);
                logging::log(
                    logging::LogLevel::Info,
                    "Bangs enabled (imported bang database)",
                    &[],
                    None,
                );
            }
            Err(e) => {
                logging::log(
                    logging::LogLevel::Error,
                    &format!("Bangs enable: could not check SQL state: {e}"),
                    &[],
                    None,
                );
            }
        }
    }

    fn disable() {
        PENDING_URL.with(|cell| cell.borrow_mut().take());
        logging::log(logging::LogLevel::Info, "Bangs disabled", &[], None);
    }

    fn on_setting_changed(_key: String, _value: String) {
        // Bangs has no user-tunable settings (other than the
        // host-managed `enabled.bangs` flag, which the host
        // handles without notifying the guest).
    }
}

// =========================================================
// Search
// =========================================================

impl SearchGuest for BangsPlugin {
    fn entries() -> Vec<CatalogEntry> {
        // Query-only gadget — no catalog.
        Vec::new()
    }

    fn search(query: String, _matched_prefix: Option<String>) -> SearchResponse {
        let Some((bang_trigger, bang_token_idx)) = find_bang_token(&query) else {
            return SearchResponse::Nothing;
        };

        let db = sql::connection();
        let Ok(Some(bang)) = lookup_bang(&db, bang_trigger) else {
            return SearchResponse::Nothing;
        };

        let clean_query = remove_bang_token(&query, bang_token_idx);

        // Build the resolved URL: either the template with
        // the query substituted, or the bare domain if there
        // is no remaining query.
        let resolved_url = if clean_query.is_empty() {
            format!("https://{}/", bang.domain)
        } else {
            let encoded = urlencoding::encode(&clean_query);
            bang.url_template.replace("{{{s}}}", &encoded)
        };

        // Stash the URL for `execute()` — see the PENDING_URL
        // module comment for the invariant that makes this safe.
        PENDING_URL.with(|cell| {
            *cell.borrow_mut() = Some(resolved_url.clone());
        });

        let title = if clean_query.is_empty() {
            format!("Open {}", bang.service_name)
        } else {
            format!("Open '{}' in {}", clean_query, bang.service_name)
        };

        // Highlight the service-name span inside the title
        // so the result visually anchors on the target
        // service. The native gadget used `Utf16Positions` —
        // we compute the same UTF-16 offset list inline here
        // since a helper crate isn't available in the guest.
        let title_highlight_positions = utf16_positions_for_substring(&title, &bang.service_name);

        // Cache-first favicon lookup. On a cold cache we fall
        // back to the generic external-link hero icon and let
        // the host's background fetch warm the cache for
        // subsequent keystrokes — blocking on the network in a
        // per-keystroke search loop would be wrong.
        let icon = website_metadata::favicon_or(
            &bang.domain,
            EntryIcon::HeroIcon("arrow-top-right-on-square".to_string()),
        );

        let entry = ScoredEntry {
            id: format!("!{bang_trigger}"),
            title,
            subtitle: Some(resolved_url),
            icon: Some(icon),
            score: BANG_SCORE,
            title_highlight_positions,
            subtitle_highlight_positions: Vec::new(),
            actions: vec![
                Action {
                    id: ActionId::Open,
                    label: "Open in Browser".to_string(),
                },
                Action {
                    id: ActionId::Copy,
                    label: "Copy URL".to_string(),
                },
            ],
        };

        SearchResponse::Results(vec![entry])
    }

    fn execute(_entry_id: String, action_id: ActionId) -> Result<PostAction, String> {
        // Pop the pending URL. Taking (not cloning) means a
        // stray second execute() without an intervening
        // search() correctly errors out rather than
        // re-opening the last URL — defensive against
        // double-fire UI bugs.
        let url = PENDING_URL
            .with(|cell| cell.borrow_mut().take())
            .ok_or_else(|| "no pending URL to act on".to_string())?;

        match action_id {
            ActionId::Open => {
                opener::open_url(&url).map_err(|e| format!("open URL: {e}"))?;
                Ok(PostAction::Dismiss)
            }
            ActionId::Copy => {
                clipboard::write_text(&url)
                    .map_err(|e| format!("copy URL to clipboard: {e}"))?;
                Ok(PostAction::Dismiss)
            }
            other => Err(format!("unsupported action: {other:?}")),
        }
    }
}

// =========================================================
// Messaging (settings UI RPC)
// =========================================================

impl MessagingGuest for BangsPlugin {
    fn handle_message(method: String, _payload: String) -> Result<String, String> {
        match method.as_str() {
            "stats" => {
                let db = sql::connection();
                let stats = query_stats(&db)?;
                serde_json::to_string(&stats)
                    .map_err(|e| format!("serialize stats: {e}"))
            }
            "refresh" => {
                // Network-only refresh. On failure, return
                // a JSON `{ "error": "..." }` payload so the
                // settings UI can show the message without
                // the existing data getting wiped.
                let db = sql::connection();
                match try_import_from_network(&db) {
                    Ok(()) => {
                        let stats = query_stats(&db)?;
                        serde_json::to_string(&stats)
                            .map_err(|e| format!("serialize stats: {e}"))
                    }
                    Err(e) => {
                        let payload = json!({ "error": e });
                        Ok(payload.to_string())
                    }
                }
            }
            other => Err(format!("unknown message method: {other}")),
        }
    }
}

// =========================================================
// Tasks — none
// =========================================================

impl_noop_tasks!(BangsPlugin);

// =========================================================
// Bang lookup
// =========================================================

struct BangRecord {
    service_name: String,
    url_template: String,
    domain: String,
}

/// Look up a bang trigger in the database. `Ok(None)` for a
/// missing trigger, `Err` for a SQL-layer failure.
fn lookup_bang(db: &SqlHandle, trigger: &str) -> Result<Option<BangRecord>, String> {
    let row = query_one(
        db,
        "SELECT service_name, url_template, domain FROM bangs WHERE trigger = ?1",
        &[SqlValue::Text(trigger.to_string())],
    )
    .map_err(|e| format!("lookup bang `{trigger}`: {e}"))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let service_name = row
        .text(0)
        .ok_or_else(|| format!("bang row missing service_name column for `{trigger}`"))?
        .to_string();
    let url_template = row
        .text(1)
        .ok_or_else(|| format!("bang row missing url_template column for `{trigger}`"))?
        .to_string();
    let domain = row
        .text(2)
        .ok_or_else(|| format!("bang row missing domain column for `{trigger}`"))?
        .to_string();

    Ok(Some(BangRecord {
        service_name,
        url_template,
        domain,
    }))
}

// =========================================================
// Query token parsing
//
// Pure functions, exhaustively unit-tested at the bottom of
// the file. Ported verbatim from the native gadget; no host
// dependencies.
// =========================================================

/// Find the first bang token in the query. Returns the
/// trigger (without the `!`) and the byte index of the `!`
/// in the original query string.
fn find_bang_token(query: &str) -> Option<(&str, usize)> {
    for (idx, token) in query.split_whitespace().enumerate() {
        if let Some(trigger) = token.strip_prefix('!')
            && !trigger.is_empty()
        {
            let byte_offset = byte_offset_of_token(query, idx);
            return Some((trigger, byte_offset));
        }
    }
    None
}

/// Compute the byte offset of the nth whitespace-delimited
/// token in the string.
fn byte_offset_of_token(s: &str, token_index: usize) -> usize {
    let mut current_token = 0;
    let mut in_token = false;

    for (i, ch) in s.char_indices() {
        if ch.is_whitespace() {
            in_token = false;
        } else if !in_token {
            if current_token == token_index {
                return i;
            }
            in_token = true;
            current_token += 1;
        }
    }
    0
}

/// Compute the UTF-16 code-unit offsets inside `haystack`
/// that correspond to every character of the first occurrence
/// of `needle`. Returns an empty vec if `needle` does not
/// appear. The host treats these offsets as a "highlight this
/// range" hint when rendering the result title.
///
/// Inline here rather than shared via the SDK because the
/// logic is small (~15 lines) and only one guest needs it
/// today. If a second gadget lands with the same requirement,
/// lift it into the SDK.
fn utf16_positions_for_substring(haystack: &str, needle: &str) -> Vec<u32> {
    if needle.is_empty() {
        return Vec::new();
    }
    let Some(byte_offset) = haystack.find(needle) else {
        return Vec::new();
    };

    // Walk the haystack up to `byte_offset` accumulating
    // UTF-16 code-unit counts; from there, emit one offset
    // per character of `needle`.
    let mut utf16_offset: u32 = 0;
    for ch in haystack[..byte_offset].chars() {
        utf16_offset += ch.len_utf16() as u32;
    }

    let mut positions = Vec::with_capacity(needle.chars().count());
    for ch in needle.chars() {
        positions.push(utf16_offset);
        utf16_offset += ch.len_utf16() as u32;
    }
    positions
}

/// Remove the bang token at the given byte offset. Collapses
/// any resulting runs of whitespace into a single space and
/// trims leading/trailing whitespace.
fn remove_bang_token(query: &str, bang_byte_offset: usize) -> String {
    let token_end = query[bang_byte_offset..]
        .find(char::is_whitespace)
        .map(|pos| bang_byte_offset + pos)
        .unwrap_or(query.len());

    let before = &query[..bang_byte_offset];
    let after = &query[token_end..];

    let mut result = String::with_capacity(before.len() + after.len());
    result.push_str(before);
    result.push_str(after);

    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

// =========================================================
// Data import
//
// The bang database is (re-)populated via a single
// `BEGIN` / `COMMIT` transaction bracket around ~13 000
// INSERTs. Without the explicit transaction every INSERT is
// auto-committed, which turns enable-time import into a
// multi-second freeze on first launch. The WIT `sql`
// interface does not expose a `transaction()` wrapper, so
// the guest has to manage the bracket itself.
// =========================================================

#[derive(Debug, Deserialize)]
struct BangEntry {
    t: String,
    s: String,
    u: String,
    d: String,
    #[serde(default)]
    c: Option<String>,
    #[serde(default)]
    sc: Option<String>,
    #[serde(default)]
    r: i64,
}

fn parse_bang_json(json: &str) -> Result<Vec<BangEntry>, String> {
    serde_json::from_str(json).map_err(|e| format!("parse bang.js JSON: {e}"))
}

fn import_bangs(db: &SqlHandle, entries: &[BangEntry], source: &str) -> Result<(), String> {
    db.execute("BEGIN", &[])
        .map_err(|e| format!("begin transaction: {e}"))?;

    let result = (|| -> Result<(), String> {
        db.execute("DELETE FROM bangs", &[])
            .map_err(|e| format!("clear bangs: {e}"))?;
        db.execute("DELETE FROM metadata", &[])
            .map_err(|e| format!("clear metadata: {e}"))?;

        for entry in entries {
            db.execute(
                "INSERT OR IGNORE INTO bangs \
                    (trigger, service_name, url_template, domain, category, subcategory, rank) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                &[
                    SqlValue::Text(entry.t.clone()),
                    SqlValue::Text(entry.s.clone()),
                    SqlValue::Text(entry.u.clone()),
                    SqlValue::Text(entry.d.clone()),
                    match &entry.c {
                        Some(c) => SqlValue::Text(c.clone()),
                        None => SqlValue::Null,
                    },
                    match &entry.sc {
                        Some(sc) => SqlValue::Text(sc.clone()),
                        None => SqlValue::Null,
                    },
                    SqlValue::Integer(entry.r),
                ],
            )
            .map_err(|e| format!("insert bang `{}`: {e}", entry.t))?;
        }

        // Record metadata the settings UI surfaces. Import
        // date uses SQLite's strftime for consistency with
        // the native gadget and the calculator history.
        db.execute(
            "INSERT INTO metadata (key, value) \
             VALUES ('import_date', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            &[],
        )
        .map_err(|e| format!("insert metadata import_date: {e}"))?;

        let bang_count = count_rows(db, "SELECT COUNT(*) FROM bangs")
            .map_err(|e| format!("count bangs: {e}"))?;
        let domain_count = count_rows(db, "SELECT COUNT(DISTINCT domain) FROM bangs")
            .map_err(|e| format!("count domains: {e}"))?;

        for (key, value) in [
            ("source", source.to_string()),
            ("bang_count", bang_count.to_string()),
            ("domain_count", domain_count.to_string()),
        ] {
            db.execute(
                "INSERT INTO metadata (key, value) VALUES (?1, ?2)",
                &[SqlValue::Text(key.to_string()), SqlValue::Text(value)],
            )
            .map_err(|e| format!("insert metadata key `{key}`: {e}"))?;
        }

        Ok(())
    })();

    match result {
        Ok(()) => db
            .execute("COMMIT", &[])
            .map(|_| ())
            .map_err(|e| format!("commit transaction: {e}")),
        Err(e) => {
            // Best-effort rollback. If this itself fails
            // the original error is still what matters.
            let _ = db.execute("ROLLBACK", &[]);
            Err(e)
        }
    }
}

/// Run a scalar-count SELECT and return the result. Used
/// for `SELECT COUNT(*)` / `SELECT COUNT(DISTINCT ...)`.
fn count_rows(db: &SqlHandle, sql: &str) -> Result<i64, String> {
    let row = query_one(db, sql, &[])
        .map_err(|e| format!("scalar count query: {e}"))?
        .ok_or_else(|| "scalar count returned no rows".to_string())?;
    row.integer(0)
        .ok_or_else(|| "scalar count column is not INTEGER".to_string())
}

fn check_has_data(db: &SqlHandle) -> Result<bool, String> {
    let count = count_rows(db, "SELECT COUNT(*) FROM bangs")?;
    Ok(count > 0)
}

fn try_import_from_network(db: &SqlHandle) -> Result<(), String> {
    let request = HttpRequest {
        url: "https://duckduckgo.com/bang.js".to_string(),
        method: HttpMethod::Get,
        headers: vec![],
        body: None,
        // DDG's bang.js is ~2.2 MB. The default host
        // timeout and body size limit are tuned for small
        // API responses, so we override both here.
        timeout_ms: Some(30_000),
        max_body_size: Some(10 * 1024 * 1024),
        insecure_tls: false,
    };

    let response = http::fetch(&request).map_err(|e| format!("fetch bang.js: {e:?}"))?;
    if response.status < 200 || response.status >= 300 {
        return Err(format!(
            "DuckDuckGo returned HTTP {}",
            response.status
        ));
    }
    let body = String::from_utf8(response.body)
        .map_err(|e| format!("bang.js response is not UTF-8: {e}"))?;
    let entries = parse_bang_json(&body)?;
    import_bangs(db, &entries, "network")
}

fn import_from_builtin(db: &SqlHandle) -> Result<(), String> {
    let bytes = assets::read(BUNDLED_BANG_PATH)
        .map_err(|e| format!("read bundled `{BUNDLED_BANG_PATH}`: {e:?}"))?;
    let body = String::from_utf8(bytes)
        .map_err(|e| format!("bundled bang.json is not UTF-8: {e}"))?;
    let entries = parse_bang_json(&body)?;
    import_bangs(db, &entries, "builtin")
}

fn import_from_best_source(db: &SqlHandle) {
    match try_import_from_network(db) {
        Ok(()) => {}
        Err(e) => {
            logging::log(
                logging::LogLevel::Warn,
                &format!(
                    "Bangs: network import failed ({e}); falling back to bundled data"
                ),
                &[],
                None,
            );
            if let Err(e) = import_from_builtin(db) {
                logging::log(
                    logging::LogLevel::Error,
                    &format!("Bangs: bundled import also failed: {e}"),
                    &[],
                    None,
                );
            }
        }
    }
}

// =========================================================
// Settings-UI stats
// =========================================================

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BangStats {
    import_date: Option<String>,
    source: Option<String>,
    bang_count: Option<i64>,
    domain_count: Option<i64>,
}

fn query_stats(db: &SqlHandle) -> Result<BangStats, String> {
    let rows = query_all(db, "SELECT key, value FROM metadata", &[])
        .map_err(|e| format!("query metadata: {e}"))?;

    let get = |key: &str| -> Option<String> {
        rows.iter()
            .find(|r| r.text(0) == Some(key))
            .and_then(|r| r.text(1).map(|s| s.to_string()))
    };

    Ok(BangStats {
        import_date: get("import_date"),
        source: get("source"),
        bang_count: get("bang_count").and_then(|v| v.parse().ok()),
        domain_count: get("domain_count").and_then(|v| v.parse().ok()),
    })
}

// =========================================================
// Pure-function tests
//
// SQL / HTTP / assets paths are exercised by the host's own
// integration tests through the WASM gadget boundary (see
// `src-tauri/src/wasm/runtime.rs`). The tests here cover
// only the pure token-parsing helpers, which are trivial
// to exercise in the native `cargo test` pass.
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- find_bang_token ----------------------------------

    #[test]
    fn find_bang_token_leading() {
        let (trigger, offset) = find_bang_token("!g rust wasm").unwrap();
        assert_eq!(trigger, "g");
        assert_eq!(offset, 0);
    }

    #[test]
    fn find_bang_token_middle() {
        let (trigger, offset) = find_bang_token("rust !g wasm").unwrap();
        assert_eq!(trigger, "g");
        assert_eq!(offset, 5);
    }

    #[test]
    fn find_bang_token_trailing() {
        let (trigger, offset) = find_bang_token("rust wasm !crates").unwrap();
        assert_eq!(trigger, "crates");
        assert_eq!(offset, 10);
    }

    #[test]
    fn find_bang_token_first_of_multiple_wins() {
        let (trigger, _) = find_bang_token("!g rust !yt wasm").unwrap();
        assert_eq!(trigger, "g");
    }

    #[test]
    fn find_bang_token_bare_bang_rejected() {
        // `!` by itself has an empty trigger — not a bang.
        assert!(find_bang_token("! hello").is_none());
        assert!(find_bang_token("hello !").is_none());
    }

    #[test]
    fn find_bang_token_no_bang_present() {
        assert!(find_bang_token("plain query text").is_none());
    }

    #[test]
    fn find_bang_token_empty_query() {
        assert!(find_bang_token("").is_none());
    }

    #[test]
    fn find_bang_token_whitespace_only() {
        assert!(find_bang_token("   \t  ").is_none());
    }

    #[test]
    fn find_bang_token_preserves_trigger_case() {
        // Trigger comparison is case-insensitive at the SQL
        // layer (COLLATE NOCASE); the parsed token itself
        // is returned as-is.
        let (trigger, _) = find_bang_token("!GitHub rust").unwrap();
        assert_eq!(trigger, "GitHub");
    }

    #[test]
    fn find_bang_token_works_with_unicode_before() {
        let (trigger, offset) = find_bang_token("café !fr breakfast").unwrap();
        assert_eq!(trigger, "fr");
        // "café" is 5 bytes ("café" = c + a + f + é(2 bytes))
        // plus a space = 6. Verify by slicing.
        assert_eq!(&"café !fr breakfast"[offset..offset + 3], "!fr");
    }

    // ---- remove_bang_token --------------------------------

    #[test]
    fn remove_bang_token_leading() {
        let (_, offset) = find_bang_token("!g rust wasm").unwrap();
        assert_eq!(remove_bang_token("!g rust wasm", offset), "rust wasm");
    }

    #[test]
    fn remove_bang_token_middle_collapses_whitespace() {
        let (_, offset) = find_bang_token("rust !g wasm").unwrap();
        assert_eq!(remove_bang_token("rust !g wasm", offset), "rust wasm");
    }

    #[test]
    fn remove_bang_token_trailing() {
        let (_, offset) = find_bang_token("rust wasm !crates").unwrap();
        assert_eq!(remove_bang_token("rust wasm !crates", offset), "rust wasm");
    }

    #[test]
    fn remove_bang_token_only_bang() {
        let (_, offset) = find_bang_token("!g").unwrap();
        assert_eq!(remove_bang_token("!g", offset), "");
    }

    #[test]
    fn remove_bang_token_multi_space_collapse() {
        // Three spaces between "foo" and "bar" should
        // collapse to one even without a bang removal.
        let (_, offset) = find_bang_token("foo   !g   bar").unwrap();
        let out = remove_bang_token("foo   !g   bar", offset);
        assert_eq!(out, "foo bar");
    }

    #[test]
    fn remove_bang_token_leaves_tabs_and_newlines_collapsed() {
        let (_, offset) = find_bang_token("rust\t!g\nwasm").unwrap();
        assert_eq!(remove_bang_token("rust\t!g\nwasm", offset), "rust wasm");
    }

    // ---- byte_offset_of_token -----------------------------

    #[test]
    fn byte_offset_of_token_first() {
        assert_eq!(byte_offset_of_token("one two three", 0), 0);
    }

    #[test]
    fn byte_offset_of_token_second() {
        assert_eq!(byte_offset_of_token("one two three", 1), 4);
    }

    #[test]
    fn byte_offset_of_token_past_end() {
        // Past the last token the helper returns 0 (the
        // caller — find_bang_token — already iterated via
        // split_whitespace so it won't hit this in practice).
        assert_eq!(byte_offset_of_token("one two", 5), 0);
    }

    // ---- utf16_positions_for_substring ---------------------

    #[test]
    fn utf16_positions_ascii() {
        // "GitHub" appears at offset 5 ("Open ")
        let positions = utf16_positions_for_substring("Open GitHub", "GitHub");
        assert_eq!(positions, vec![5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn utf16_positions_with_astral_plane_character_before() {
        // "🎉" is U+1F389, which is one UTF-16 surrogate
        // pair (2 code units) but 4 UTF-8 bytes. The
        // highlight offsets must use UTF-16 units, not
        // bytes.
        let positions = utf16_positions_for_substring("🎉 Open GitHub", "GitHub");
        // "🎉" = 2 UTF-16 units, " " = 1, "Open" = 4, " " = 1 = 8.
        assert_eq!(positions[0], 8);
        assert_eq!(positions.len(), 6);
    }

    #[test]
    fn utf16_positions_needle_is_multichar_unicode() {
        // "café" contains é (1 UTF-16 unit), total 4 code units.
        let positions = utf16_positions_for_substring("Open café now", "café");
        // "Open " = 5 UTF-16 units.
        assert_eq!(positions, vec![5, 6, 7, 8]);
    }

    #[test]
    fn utf16_positions_empty_needle() {
        let positions = utf16_positions_for_substring("Open GitHub", "");
        assert!(positions.is_empty());
    }

    #[test]
    fn utf16_positions_needle_absent() {
        let positions = utf16_positions_for_substring("Open GitHub", "Twitter");
        assert!(positions.is_empty());
    }

    // ---- parse_bang_json ----------------------------------

    #[test]
    fn parse_bang_json_minimal_entry() {
        let json = r#"[{"t":"g","s":"Google","u":"https://g.co/?q={{{s}}}","d":"g.co"}]"#;
        let entries = parse_bang_json(json).expect("parse");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].t, "g");
        assert_eq!(entries[0].s, "Google");
        assert_eq!(entries[0].d, "g.co");
        assert!(entries[0].c.is_none());
        assert!(entries[0].sc.is_none());
        assert_eq!(entries[0].r, 0);
    }

    #[test]
    fn parse_bang_json_full_entry() {
        let json = r#"[{
            "t":"yt","s":"YouTube","u":"https://youtube.com/results?q={{{s}}}",
            "d":"youtube.com","c":"Entertainment","sc":"Video","r":1300
        }]"#;
        let entries = parse_bang_json(json).expect("parse");
        assert_eq!(entries[0].c.as_deref(), Some("Entertainment"));
        assert_eq!(entries[0].sc.as_deref(), Some("Video"));
        assert_eq!(entries[0].r, 1300);
    }

    #[test]
    fn parse_bang_json_empty_array() {
        let entries = parse_bang_json("[]").expect("parse empty");
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_bang_json_rejects_malformed() {
        assert!(parse_bang_json("not json at all").is_err());
        assert!(parse_bang_json("[{").is_err());
    }

    #[test]
    fn parse_bang_json_requires_string_t() {
        // `t` is declared `String` (no default), so a
        // missing field must error.
        let json = r#"[{"s":"Google","u":"https://g.co/","d":"g.co"}]"#;
        assert!(parse_bang_json(json).is_err());
    }
}
