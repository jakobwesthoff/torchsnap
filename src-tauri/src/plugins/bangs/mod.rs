// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Bangs Plugin
//
// Detects DuckDuckGo bang patterns (e.g. `!g`, `!yt`, `!crates`)
// anywhere in the query string and surfaces a single result entry
// that opens the corresponding service URL with the remaining
// search terms.
//
// The bang database is loaded from DDG's public bang.js into a
// local SqlStorage instance. On first launch, the plugin tries
// to fetch a fresh copy from the network; if that fails, it falls
// back to a version baked into the binary at compile time.
// =========================================================

mod import;
mod schema;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use std::thread;

use anyhow::Context;
use serde::Serialize;
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

use crate::network::website_metadata::{MetadataResult, WebsiteMetadataService};
use crate::network::Http;
use crate::search::types::{
    Action, ActionId, ActionKeybinding, EntryIcon, PluginResponse, PostAction,
    ScoredEntry,
};
use crate::settings::SettingsInit;
use crate::storage::SqlStorage;
use crate::storage::SqlValue;
use crate::unicode::Utf16Positions;

use super::{Plugin, PluginContext};
use schema::{BAKED_IN_BANGS, MIGRATION_001, PLUGIN_ID};

// =========================================================
// Plugin State
// =========================================================

struct BangState {
    sql: SqlStorage,
    http: Http,
}

/// Score assigned to bang results. High enough to appear near the
/// top (above mediocre fuzzy matches) but below a perfect
/// title match + heavy frecency. Tunable.
const BANG_SCORE: u32 = 1000;

// =========================================================
// DuckDuckGoBangsPlugin
// =========================================================

pub struct BangsPlugin {
    /// Set to `true` once the bang database is populated and ready
    /// for queries. `search()` returns immediately while this is
    /// `false`.
    ready: AtomicBool,

    /// Shared plugin state, initialized in `setup()`.
    state: Mutex<Option<Arc<BangState>>>,

    // HACK/FIXME: Stores the resolved URL from the most recent search()
    // so execute() can open it. This is a workaround because execute()
    // only receives the entry_id (which is `!<trigger>` for frecency
    // tracking) and has no access to the original query or resolved URL.
    //
    // This MUST be replaced with the arbitrary data parameter mechanism
    // once todo 01kn7v6ynyf580ax9jyyt25jgc is implemented. That
    // refactor should happen shortly after this plugin lands.
    //
    // Safety: The UI can only execute the currently displayed result,
    // and search() runs synchronously per query cycle, so the stored
    // URL always matches what's on screen.
    pending_url: Mutex<Option<String>>,

    /// Tracks whether the plugin is currently enabled.
    enabled: Arc<AtomicBool>,

    /// Shared website metadata service for favicon lookups.
    metadata_service: Arc<WebsiteMetadataService>,
}

impl BangsPlugin {
    pub fn new(
        metadata_service: Arc<WebsiteMetadataService>,
    ) -> Self {
        Self {
            ready: AtomicBool::new(false),
            state: Mutex::new(None),
            pending_url: Mutex::new(None),
            enabled: Arc::new(AtomicBool::new(true)),
            metadata_service,
        }
    }
}

// =========================================================
// Plugin Trait Implementation
// =========================================================

impl Plugin for BangsPlugin {
    fn id(&self) -> &str {
        PLUGIN_ID
    }

    fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn enabled_settings_key(&self) -> Option<&'static str> {
        Some("enabled")
    }

    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit {
        settings.ensure("enabled", true)
    }

    // =========================================================
    // Setup — Background Initialization
    //
    // Runs on a background thread during app startup. Opens the
    // bang database, populates it if needed (network fetch with
    // baked-in fallback), and signals readiness.
    // =========================================================

    fn setup(&self, app: &tauri::AppHandle, ctx: &PluginContext) {
        // Read initial enabled state and watch for changes.
        let initial_enabled: bool = ctx.settings.get("enabled").unwrap_or(true);
        self.enabled.store(initial_enabled, Ordering::Relaxed);

        let mut enabled_watch = ctx.notifier.watch::<bool>("enabled");
        let enabled_flag = Arc::clone(&self.enabled);
        thread::spawn(move || {
            loop {
                let Some(new_enabled) = enabled_watch.blocking_changed() else {
                    break;
                };
                enabled_flag.store(new_enabled, Ordering::Relaxed);
            }
        });

        let data_dir = app
            .path()
            .app_data_dir()
            .expect("app data dir is available")
            .join("plugins")
            .join(PLUGIN_ID);

        let db_path = data_dir.join("bangs.db");
        let sql = SqlStorage::open(db_path, &[MIGRATION_001]).expect("open bang database");

        let http = Http::new();
        let state = Arc::new(BangState { sql, http });

        *self.state.lock().expect("state lock not poisoned") = Some(Arc::clone(&state));

        // Check if the database already has data from a previous run.
        let has_data = check_has_data(&state.sql);

        if !has_data {
            // First launch or cleared database — try network, fall back
            // to baked-in data.
            import_from_best_source(&state);
        }

        self.ready.store(true, Ordering::Relaxed);
    }

    fn teardown(&self) {}

    // =========================================================
    // Search
    //
    // Scans the query for the first `!<identifier>` token, looks
    // it up in the bang database, and returns a single result if
    // found. Runs on every query (no prefix registration) since
    // bangs can appear anywhere in the input.
    // =========================================================

    fn search(
        &self,
        query: &str,
        _matched_prefix: Option<&str>,
    ) -> Option<PluginResponse> {
        if !self.ready.load(Ordering::Relaxed) {
            return None;
        }

        // Find the first token that looks like a bang (`!<word>`).
        let (bang_trigger, bang_token_idx) = match find_bang_token(query) {
            Some(found) => found,
            None => return None,
        };

        let state = self.state.lock().expect("state lock not poisoned");
        let state = match state.as_ref() {
            Some(s) => s,
            None => return None,
        };

        // Look up the bang in the database (case-insensitive).
        let bang = match lookup_bang(&state.sql, bang_trigger) {
            Some(b) => b,
            None => return None,
        };

        // Remove the bang token from the query and clean up whitespace.
        let clean_query = remove_bang_token(query, bang_token_idx);

        // Build the resolved URL: either the template with the query
        // substituted, or the bare domain if there's no query.
        let resolved_url = if clean_query.is_empty() {
            format!("https://{}/", bang.domain)
        } else {
            let encoded_query = urlencoding::encode(&clean_query);
            bang.url_template.replace("{{{s}}}", &encoded_query)
        };

        // Store the URL for execute() to retrieve.
        *self
            .pending_url
            .lock()
            .expect("pending_url lock not poisoned") = Some(resolved_url.clone());

        // Build the result entry with the service name highlighted.
        let title = if clean_query.is_empty() {
            format!("Open {}", bang.service_name)
        } else {
            format!("Open '{}' in {}", clean_query, bang.service_name)
        };
        let title_positions =
            Utf16Positions::from_substring(&title, &bang.service_name, false);

        let entry = ScoredEntry {
            id: format!("!{}", bang_trigger),
            title,
            subtitle: Some(resolved_url),
            icon: Some(
                match self.metadata_service.try_cached(&bang.domain) {
                    MetadataResult::Found(meta) => meta.favicon,
                    _ => EntryIcon::HeroIcon("arrow-top-right-on-square".to_string()),
                },
            ),
            score: BANG_SCORE,
            title_positions,
            subtitle_positions: Utf16Positions(vec![]),
            actions: vec![
                Action {
                    id: ActionId::Open,
                    label: "Open in Browser".to_string(),
                    keybinding: None,
                },
                Action {
                    id: ActionId::Copy,
                    label: "Copy URL".to_string(),
                    keybinding: Some(ActionKeybinding {
                        modifiers: vec!["Meta".into()],
                        key: "c".into(),
                    }),
                },
            ],
        };

        Some(PluginResponse::Results(vec![entry]))
    }

    // =========================================================
    // Execute
    //
    // Opens the resolved URL in the default browser and dismisses
    // the launcher.
    // =========================================================

    fn execute(
        &self,
        _entry_id: &str,
        action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        match action_id {
            ActionId::Open => {
                let url = self
                    .pending_url
                    .lock()
                    .expect("pending_url lock not poisoned")
                    .take()
                    .context("no pending URL to open")?;

                app.opener()
                    .open_url(&url, None::<&str>)
                    .context("open bang URL in browser")?;

                Ok(PostAction::Dismiss)
            }
            ActionId::Copy => {
                let url = self
                    .pending_url
                    .lock()
                    .expect("pending_url lock not poisoned")
                    .take()
                    .context("no pending URL to copy")?;

                use tauri_plugin_clipboard_manager::ClipboardExt;
                app.clipboard()
                    .write_text(&url)
                    .map_err(|e| anyhow::anyhow!("copy URL to clipboard: {e}"))?;

                Ok(PostAction::Dismiss)
            }
            _ => anyhow::bail!("unsupported action: {action_id:?}"),
        }
    }

    // =========================================================
    // Settings Messages
    // =========================================================

    fn handle_message(
        &self,
        method: &str,
        _payload: serde_json::Value,
        _channel: tauri::ipc::Channel<serde_json::Value>,
    ) -> anyhow::Result<serde_json::Value> {
        match method {
            "stats" => {
                let state = self.state.lock().expect("state lock not poisoned");
                let state = state.as_ref().context("plugin not yet initialized")?;

                let stats = query_stats(&state.sql)?;
                Ok(serde_json::to_value(stats).context("serialize stats")?)
            }
            "refresh" => {
                let state = self.state.lock().expect("state lock not poisoned");
                let state = state.as_ref().context("plugin not yet initialized")?;

                // Attempt a fresh download. On failure, return the error
                // as a message rather than bailing — the existing data
                // stays intact.
                match try_import_from_network(state) {
                    Ok(()) => {
                        let stats = query_stats(&state.sql)?;
                        Ok(serde_json::to_value(stats).context("serialize stats")?)
                    }
                    Err(e) => {
                        eprintln!("bangs: refresh failed: {e:#}");
                        Ok(serde_json::json!({ "error": format!("{e:#}") }))
                    }
                }
            }
            _ => anyhow::bail!("unknown message method: {method}"),
        }
    }
}

// =========================================================
// Bang Lookup Helpers
// =========================================================

/// Result of looking up a bang trigger in the database.
struct BangRecord {
    service_name: String,
    url_template: String,
    domain: String,
}

/// Look up a bang trigger in the SQL database. Returns `None` if
/// the trigger is not found.
fn lookup_bang(sql: &SqlStorage, trigger: &str) -> Option<BangRecord> {
    let results: Vec<BangRecord> = sql
        .query_map(
            "SELECT service_name, url_template, domain FROM bangs WHERE trigger = ?1",
            &[SqlValue::from(trigger)],
            |row| {
                Ok(BangRecord {
                    service_name: row.get(0)?,
                    url_template: row.get(1)?,
                    domain: row.get(2)?,
                })
            },
        )
        .ok()?;

    results.into_iter().next()
}

// =========================================================
// Query Token Parsing
//
// Finds the first `!<identifier>` token in the query string
// and provides utilities to remove it cleanly.
// =========================================================

/// Find the first bang token in the query. Returns the trigger
/// (without the `!` prefix) and the byte index of the `!` in
/// the original query string.
fn find_bang_token(query: &str) -> Option<(&str, usize)> {
    for (idx, token) in query.split_whitespace().enumerate() {
        if let Some(trigger) = token.strip_prefix('!')
            && !trigger.is_empty()
        {
            // Compute the byte offset of this token in the
            // original query string.
            let byte_offset = byte_offset_of_token(query, idx);
            return Some((trigger, byte_offset));
        }
    }
    None
}

/// Compute the byte offset of the nth whitespace-delimited token
/// in the string.
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

/// Remove the bang token at the given byte offset from the query
/// string. Collapses any resulting double whitespace and trims.
fn remove_bang_token(query: &str, bang_byte_offset: usize) -> String {
    // Find the end of the bang token (next whitespace or end of string).
    let token_end = query[bang_byte_offset..]
        .find(char::is_whitespace)
        .map(|pos| bang_byte_offset + pos)
        .unwrap_or(query.len());

    let before = &query[..bang_byte_offset];
    let after = &query[token_end..];

    let mut result = String::with_capacity(before.len() + after.len());
    result.push_str(before);
    result.push_str(after);

    // Collapse multiple spaces and trim.
    let collapsed: String = result.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
}

// =========================================================
// Data Import Helpers
// =========================================================

/// Check whether the bangs table already has data.
fn check_has_data(sql: &SqlStorage) -> bool {
    let count: Vec<i64> = sql
        .query_map("SELECT COUNT(*) FROM bangs", &[], |row| row.get(0))
        .unwrap_or_default();
    count.first().copied().unwrap_or(0) > 0
}

/// Try to import bangs from the network, then fall back to the
/// baked-in data if the network fails.
fn import_from_best_source(state: &BangState) {
    match try_import_from_network(state) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("bangs: network fetch failed, using baked-in data: {e:#}");
            import_from_builtin(&state.sql);
        }
    }
}

/// Attempt to download and import a fresh bang database from DDG.
fn try_import_from_network(state: &BangState) -> anyhow::Result<()> {
    let mut response = state
        .http
        .get("https://duckduckgo.com/bang.js")
        .send()
        .context("fetch bang.js from DuckDuckGo")?;

    let body = response.text().context("read bang.js response body")?;
    let entries = import::parse_bang_json(&body).context("parse network bang.js")?;

    import::import_bangs(&state.sql, &entries, "network")
        .context("import network bang data into database")?;

    Ok(())
}

/// Import the baked-in bang database (compile-time fallback).
fn import_from_builtin(sql: &SqlStorage) {
    let entries = import::parse_bang_json(BAKED_IN_BANGS).expect("baked-in bang.json is valid");

    import::import_bangs(sql, &entries, "builtin").expect("import baked-in bang data");
}

// =========================================================
// Stats for Settings UI
// =========================================================

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BangStats {
    import_date: Option<String>,
    source: Option<String>,
    bang_count: Option<i64>,
    domain_count: Option<i64>,
}

/// Query import metadata for the settings UI.
fn query_stats(sql: &SqlStorage) -> anyhow::Result<BangStats> {
    let metadata: Vec<(String, String)> = sql
        .query_map("SELECT key, value FROM metadata", &[], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .context("query metadata")?;

    let get = |key: &str| -> Option<String> {
        metadata
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };

    Ok(BangStats {
        import_date: get("import_date"),
        source: get("source"),
        bang_count: get("bang_count").and_then(|v| v.parse().ok()),
        domain_count: get("domain_count").and_then(|v| v.parse().ok()),
    })
}
