// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ZeroTier plugin entry point.
//!
//! Wires the per-plugin host imports to the domain modules:
//!
//! * `enable()` resolves the auth token, validates it, opens
//!   the SQLite history, and (on macOS) merges the official
//!   UI's `saved_networks.json`.
//! * `search()` dispatches the per-keystroke query through
//!   intent detection and the rate-limit cache, joining live
//!   daemon state with the history table to surface
//!   Connected / JoinedOffline / KnownOnly entries.
//! * `execute()` translates an entry's primary or secondary
//!   action into Connect / Disconnect / Forget.
//! * `handle_message()` exposes the frontend RPC contract
//!   the settings panel needs (refresh, reimport, forget,
//!   clear-all, validate-token, auth-state).

use std::cell::RefCell;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use torchsnap_plugin_sdk::http::HttpError;
use torchsnap_plugin_sdk::platform::Os;
use torchsnap_plugin_sdk::prelude::*;
use torchsnap_plugin_sdk::sql::SqlHandle;

mod actions;
mod api;
mod auth;
mod cache;
mod history;
mod query;

use api::{ApiError, Client, Network, NetworkStatus};
use auth::{ResolvedToken, TokenSource};
use cache::{DEFAULT_TTL, RateLimitCache};
use query::{Intent, NetworkRow, NetworkState, ScoredMatch};

struct ZeroTierPlugin;
define_plugin!(ZeroTierPlugin);

// =========================================================
// Per-instance runtime state
//
// `wasm32-wasip2` is single-threaded per guest instance, so
// `thread_local!` here is effectively per-instance state.
// `RefCell` interior mutability lets the `&mut` -free guest
// trait methods (`fn enable()`, `fn search(query, ...)`)
// mutate state through borrows.
// =========================================================

thread_local! {
    static RUNTIME: RefCell<Runtime> = RefCell::new(Runtime::new());
}

struct Runtime {
    /// Resolved auth-token result. `None` between
    /// instantiation and the first `enable()`. After
    /// `enable()` it is always present, possibly with
    /// `TokenSource::None`.
    auth: Option<ResolvedToken>,
    /// API client built from the resolved token.
    client: Option<Client>,
    /// Last token-validation outcome (from `GET /status`).
    /// Drives the failure-state entries surfaced in
    /// `search()`.
    auth_state: AuthState,
    /// Cached `list_networks` result for up to
    /// `cache::DEFAULT_TTL`. The slot stores the full
    /// `Result` so cache hits don't lose error context.
    network_cache: RateLimitCache<Result<Vec<Network>, String>>,
}

impl Runtime {
    fn new() -> Self {
        Self {
            auth: None,
            client: None,
            auth_state: AuthState::Unconfigured,
            network_cache: RateLimitCache::new(DEFAULT_TTL),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum AuthState {
    /// No token resolved yet, or auto-detection found nothing
    /// and no manual paste is configured.
    #[default]
    Unconfigured,
    /// Token resolved and validated against `GET /status`.
    Validated,
    /// Token resolved but the daemon rejected it.
    Rejected,
    /// Token resolved but the daemon could not be reached
    /// at validation time.
    DaemonUnreachable,
}

// =========================================================
// Lifecycle
// =========================================================

impl LifecycleGuest for ZeroTierPlugin {
    fn enable() {
        RUNTIME.with(|cell| {
            let mut runtime = cell.borrow_mut();
            *runtime = Runtime::new();
            initialize(&mut runtime);
        });
    }

    fn disable() {
        RUNTIME.with(|cell| {
            *cell.borrow_mut() = Runtime::new();
        });
    }

    fn on_setting_changed(key: String, _value: String) {
        if key == "manualToken" {
            // The user pasted a new token; re-resolve and
            // re-validate against the daemon.
            RUNTIME.with(|cell| {
                let mut runtime = cell.borrow_mut();
                runtime.network_cache.invalidate();
                initialize(&mut runtime);
            });
        }
    }
}

// =========================================================
// One-time setup shared by `enable()` and the manual-token
// re-resolve path. Resolves the token, validates it, opens
// the SQLite handle, and merges `saved_networks.json` on
// macOS.
// =========================================================

fn initialize(runtime: &mut Runtime) {
    let resolved = auth::resolve();
    runtime.auth = Some(resolved.clone());

    if matches!(resolved.source, TokenSource::None) {
        runtime.auth_state = AuthState::Unconfigured;
        runtime.client = None;
        return;
    }

    let client = Client::new(resolved.token.clone());
    runtime.auth_state = match client.status() {
        Ok(_) => AuthState::Validated,
        Err(ApiError::AuthRejected) => AuthState::Rejected,
        Err(ApiError::DaemonUnreachable(_)) => AuthState::DaemonUnreachable,
        Err(_) => AuthState::DaemonUnreachable,
    };
    runtime.client = Some(client);

    // Merge `saved_networks.json` only on macOS — that file
    // is a private cache of the official macOS UI and does
    // not exist on Linux or Windows.
    if matches!(torchsnap_plugin_sdk::platform::current_os(), Os::Macos) {
        let _ = merge_saved_networks();
    }
}

fn merge_saved_networks() -> Result<usize, String> {
    let path = torchsnap_plugin_sdk::paths::resolve(
        "${xdg-config}/ZeroTier/saved_networks.json",
    )
    .map_err(|e| format!("resolve saved_networks path: {e:?}"))?;
    let bytes =
        torchsnap_plugin_sdk::fs::read_file(&path).map_err(|e| format!("read: {e:?}"))?;
    let json = String::from_utf8(bytes).map_err(|e| format!("utf8: {e}"))?;
    let db = history::connection();
    history::import_saved_networks(&db, &json, now_ms())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// =========================================================
// Search
// =========================================================

impl SearchGuest for ZeroTierPlugin {
    fn entries() -> Vec<CatalogEntry> {
        vec![]
    }

    fn search(query: String, _matched_prefix: Option<String>) -> SearchResponse {
        let intent = query::intent_for(&query);
        if matches!(intent, Intent::None) {
            return SearchResponse::Nothing;
        }

        RUNTIME.with(|cell| {
            let runtime = cell.borrow();
            let entries = build_search_entries(&runtime, &intent);
            if entries.is_empty() {
                SearchResponse::Nothing
            } else {
                SearchResponse::Results(entries)
            }
        })
    }

    fn execute(entry_id: String, action_id: ActionId) -> Result<PostAction, String> {
        // `OpenSettings` is host-routed and never reaches us,
        // but defend in depth — return Dismiss so we
        // gracefully no-op if the host ever forwards it.
        if matches!(action_id, ActionId::OpenSettings) {
            return Ok(PostAction::Dismiss);
        }

        let Some(network_id) = query::parse_entry_id(&entry_id) else {
            return Err(format!("not a network entry: {entry_id}"));
        };

        RUNTIME.with(|cell| -> Result<PostAction, String> {
            let mut runtime = cell.borrow_mut();
            let Some(client) = runtime.client.as_ref() else {
                return Err("ZeroTier auth not configured".into());
            };
            let client = client.clone();
            let db = history::connection();
            let now = now_ms();
            let live = current_live_state(&runtime);
            let currently_joined = live
                .iter()
                .any(|n| n.id.eq_ignore_ascii_case(&network_id));
            let result = match action_id {
                ActionId::Open => {
                    if currently_joined {
                        actions::disconnect(&client, &network_id)
                    } else {
                        let name_hint = lookup_name_hint(&runtime, &network_id);
                        actions::connect(&client, &db, &network_id, &name_hint, now)
                    }
                }
                ActionId::Delete => {
                    actions::forget(&client, &db, &network_id, currently_joined)
                }
                _ => return Err(format!("unsupported action: {action_id:?}")),
            };
            runtime.network_cache.invalidate();
            result?;
            Ok(PostAction::Dismiss)
        })
    }
}

// =========================================================
// Search-entry assembly
// =========================================================

fn build_search_entries(runtime: &Runtime, intent: &Intent) -> Vec<ScoredEntry> {
    // Failure-state entries — surfaced only when the query
    // has ZT context (bare ID, or a non-empty `Match` that
    // would otherwise produce results) so unrelated queries
    // aren't polluted.
    if let Some(failure) = failure_entry(runtime, intent) {
        return vec![failure];
    }

    let live = current_live_state(runtime);
    let known = load_known_rows();
    let rows = merge_live_and_known(&live, &known);

    match intent {
        Intent::None => Vec::new(),
        Intent::Match(q) => {
            query::match_networks(q, &rows)
                .into_iter()
                .map(|m| scored_match_to_entry(&m))
                .collect()
        }
        Intent::JoinById(id) => {
            // If the id maps to an existing row, surface that
            // row (Connected / JoinedOffline / KnownOnly) —
            // no synthetic entry. Otherwise produce the
            // synthetic Connect entry.
            if let Some(row) = rows.iter().find(|r| r.id == *id) {
                vec![scored_match_to_entry(&ScoredMatch {
                    row: row.clone(),
                    score: query::base_score_for(row.state) + 50,
                    title_highlight_positions: vec![],
                })]
            } else {
                vec![synthetic_connect_entry(id)]
            }
        }
    }
}

fn failure_entry(runtime: &Runtime, intent: &Intent) -> Option<ScoredEntry> {
    if matches!(intent, Intent::None) {
        return None;
    }
    let (title, label, action_id) = match runtime.auth_state {
        AuthState::Validated => return None,
        AuthState::Unconfigured => (
            "ZeroTier token not configured",
            "Open settings",
            ActionId::OpenSettings,
        ),
        AuthState::Rejected => (
            "ZeroTier authentication failed",
            "Open settings",
            ActionId::OpenSettings,
        ),
        AuthState::DaemonUnreachable => (
            "ZeroTier daemon not running",
            "Dismiss",
            ActionId::Open,
        ),
    };
    Some(ScoredEntry {
        id: format!("failure:{title}"),
        title: title.to_string(),
        subtitle: None,
        icon: Some(EntryIcon::HeroIcon("exclamation-triangle".into())),
        score: 1,
        title_highlight_positions: vec![],
        subtitle_highlight_positions: vec![],
        actions: vec![Action {
            id: action_id,
            label: label.to_string(),
        }],
    })
}

fn synthetic_connect_entry(id: &str) -> ScoredEntry {
    ScoredEntry {
        id: query::entry_id(id),
        title: format!("Connect to network {id}"),
        subtitle: Some("Join a new ZeroTier network".to_string()),
        icon: Some(EntryIcon::HeroIcon("globe-alt".into())),
        score: query::SYNTHETIC_CONNECT_SCORE,
        title_highlight_positions: vec![],
        subtitle_highlight_positions: vec![],
        actions: vec![Action {
            id: ActionId::Open,
            label: "Connect".to_string(),
        }],
    }
}

fn scored_match_to_entry(m: &ScoredMatch) -> ScoredEntry {
    let row = &m.row;
    let (subtitle, primary_label) = match row.state {
        NetworkState::Connected => {
            let addrs = row.assigned_addresses.join(", ");
            let subtitle = if addrs.is_empty() {
                "Connected".to_string()
            } else {
                format!("Connected · {addrs}")
            };
            (subtitle, "Disconnect")
        }
        NetworkState::JoinedOffline(status) => {
            let badge = match status {
                NetworkStatus::RequestingConfiguration => "Connecting…",
                NetworkStatus::AccessDenied => "Access denied",
                NetworkStatus::AuthenticationRequired => "Authentication required",
                NetworkStatus::NotFound => "Network not found",
                NetworkStatus::PortError => "Port error",
                NetworkStatus::Unknown => "Unknown status",
                NetworkStatus::Ok => "Connected",
            };
            (badge.to_string(), "Disconnect")
        }
        NetworkState::KnownOnly => ("Stored".to_string(), "Connect"),
    };

    ScoredEntry {
        id: query::entry_id(&row.id),
        title: row.name.clone(),
        subtitle: Some(format!("{} · {}", subtitle, row.id)),
        icon: Some(EntryIcon::HeroIcon("globe-alt".into())),
        score: m.score,
        title_highlight_positions: m.title_highlight_positions.clone(),
        subtitle_highlight_positions: vec![],
        actions: vec![
            Action {
                id: ActionId::Open,
                label: primary_label.to_string(),
            },
            Action {
                id: ActionId::Delete,
                label: "Forget".to_string(),
            },
        ],
    }
}

// =========================================================
// State assembly helpers
// =========================================================

fn current_live_state(runtime: &Runtime) -> Vec<Network> {
    let Some(client) = runtime.client.as_ref() else {
        return Vec::new();
    };
    let client = client.clone();
    let result = runtime.network_cache.get_or_fetch(move || {
        client
            .list_networks()
            .map_err(|e| format!("list_networks: {e}"))
    });
    result.unwrap_or_default()
}

fn load_known_rows() -> Vec<history::HistoryRow> {
    let db = history::connection();
    history::list_all(&db).unwrap_or_default()
}

fn merge_live_and_known(
    live: &[Network],
    known: &[history::HistoryRow],
) -> Vec<NetworkRow> {
    // Live state wins; known-only entries fill in.
    let mut by_id: std::collections::HashMap<String, NetworkRow> =
        std::collections::HashMap::new();
    for net in live {
        by_id.insert(net.id.clone(), NetworkRow::from_live(net));
    }
    for row in known {
        if !by_id.contains_key(&row.id) {
            by_id.insert(
                row.id.clone(),
                NetworkRow {
                    id: row.id.clone(),
                    name: row.name.clone(),
                    state: NetworkState::KnownOnly,
                    assigned_addresses: vec![],
                },
            );
        }
    }
    by_id.into_values().collect()
}

fn lookup_name_hint(runtime: &Runtime, id: &str) -> String {
    let live = current_live_state(runtime);
    if let Some(net) = live.iter().find(|n| n.id == id) {
        return net.name.clone();
    }
    // Fall back to history.
    let db = history::connection();
    if let Ok(rows) = history::list_all(&db) {
        if let Some(r) = rows.iter().find(|r| r.id == id) {
            return r.name.clone();
        }
    }
    String::new()
}

// =========================================================
// Frontend RPC
//
// The settings panel calls `sendMessage(method, payload)` —
// the host routes the call here. Method dispatch lives below.
// =========================================================

#[derive(Serialize)]
struct AuthStateResponse {
    state: &'static str,
    source: &'static str,
}

impl MessagingGuest for ZeroTierPlugin {
    fn handle_message(method: String, payload: String) -> Result<String, String> {
        match method.as_str() {
            "refresh" => {
                RUNTIME.with(|cell| {
                    cell.borrow_mut().network_cache.invalidate();
                });
                Ok("{\"ok\":true}".into())
            }

            "reimport" => {
                let inserted = merge_saved_networks().map_err(|e| e.to_string())?;
                Ok(format!("{{\"inserted\":{inserted}}}"))
            }

            "forget" => {
                #[derive(serde::Deserialize)]
                struct Req {
                    id: String,
                }
                let req: Req = serde_json::from_str(&payload)
                    .map_err(|e| format!("parse payload: {e}"))?;
                RUNTIME.with(|cell| -> Result<String, String> {
                    let runtime = cell.borrow();
                    let Some(client) = runtime.client.as_ref() else {
                        return Err("ZeroTier auth not configured".into());
                    };
                    let live = current_live_state(&runtime);
                    let joined = live.iter().any(|n| n.id == req.id);
                    actions::forget(client, &history::connection(), &req.id, joined)?;
                    Ok("{\"ok\":true}".into())
                })
            }

            "clear_all" => {
                history::clear_all(&history::connection())?;
                Ok("{\"ok\":true}".into())
            }

            "validate_token" => RUNTIME.with(|cell| {
                let runtime = cell.borrow();
                let state = match runtime.auth_state {
                    AuthState::Validated => "validated",
                    AuthState::Rejected => "rejected",
                    AuthState::Unconfigured => "unconfigured",
                    AuthState::DaemonUnreachable => "daemon-unreachable",
                };
                Ok(format!("{{\"state\":\"{state}\"}}"))
            }),

            "auth_state" => RUNTIME.with(|cell| {
                let runtime = cell.borrow();
                let state = match runtime.auth_state {
                    AuthState::Validated => "validated",
                    AuthState::Rejected => "rejected",
                    AuthState::Unconfigured => "unconfigured",
                    AuthState::DaemonUnreachable => "daemon-unreachable",
                };
                let source = match runtime.auth.as_ref().map(|a| a.source) {
                    Some(TokenSource::AutoDetected) => "auto",
                    Some(TokenSource::ManualPaste) => "manual",
                    _ => "none",
                };
                serde_json::to_string(&AuthStateResponse { state, source })
                    .map_err(|e| e.to_string())
            }),

            other => Err(format!("unknown method: {other}")),
        }
    }
}

impl_noop_tasks!(ZeroTierPlugin);
