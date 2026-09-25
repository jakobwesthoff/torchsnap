// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ZeroTier gadget entry point.
//!
//! Wires the per-gadget host imports to the domain modules:
//!
//! * `enable()` resolves the auth token, validates it, opens
//!   the SQLite history, and (on macOS) merges the official
//!   UI's `saved_networks.json`.
//! * `search()` dispatches the per-keystroke query through
//!   intent detection and the rate-limit cache, joining live
//!   daemon state with the history table to surface
//!   Connected / JoinedOffline / KnownOnly entries.
//! * `execute()` runs an action's `Command`: join or leave
//!   (decided from live state), copy the id, forget, or open
//!   the settings.
//! * `Messaging::handle()` exposes the frontend RPC contract
//!   the settings panel needs (refresh, reimport, forget,
//!   clear-all, validate-token, auth-state, list-known).

use std::cell::RefCell;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use torchsnap_gadget_sdk::platform::Os;
use torchsnap_gadget_sdk::prelude::*;
use torchsnap_gadget_sdk::sql_storage::SqlHandle;

mod actions;
mod api;
mod auth;
mod history;
mod query;

use torchsnap_gadget_sdk::cache::{DEFAULT_TTL, RateLimitCache};

use api::{ApiError, Client, Network, NetworkStatus};
use auth::{ResolvedToken, TokenSource};
use query::{Intent, NetworkRow, NetworkState, ScoredMatch};

struct ZeroTierPlugin;
define_gadget!(ZeroTierPlugin);

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
    /// When token resolution and validation last ran. Drives
    /// the throttled retry in `search()` while `auth_state`
    /// is not `Validated`.
    last_auth_attempt: Option<Instant>,
    /// Cached `list_networks` result for up to
    /// `DEFAULT_TTL`. The slot stores the full
    /// `Result` so cache hits don't lose error context.
    network_cache: RateLimitCache<Result<Vec<Network>, String>>,
}

impl Runtime {
    fn new() -> Self {
        Self {
            auth: None,
            client: None,
            auth_state: AuthState::Unconfigured,
            last_auth_attempt: None,
            network_cache: RateLimitCache::new(DEFAULT_TTL),
        }
    }
}

/// Minimum time between two automatic auth retries. The
/// daemon may start, or its token file may appear, after the
/// gadget was enabled; retrying lets the gadget pick that up
/// without a restart. A refused connection to the loopback
/// port fails immediately, so the interval only bounds how
/// often the token files are re-read while the daemon is down.
const AUTH_RETRY_INTERVAL: Duration = Duration::from_secs(5);

/// Whether `search()` should re-run token resolution and
/// validation. A validated token is never re-checked: the
/// daemon never rotates it.
fn auth_retry_due(state: AuthState, last_attempt: Option<Instant>, now: Instant) -> bool {
    if state == AuthState::Validated {
        return false;
    }
    match last_attempt {
        None => true,
        Some(at) => now.duration_since(at) >= AUTH_RETRY_INTERVAL,
    }
}

/// Whether this search should retry auth. Only queries about
/// ZeroTier retry, so unrelated searches never pay for token
/// reads or a status call.
fn should_retry_auth(
    runtime: &Runtime,
    intent: &Intent,
    known: &[NetworkRow],
    now: Instant,
) -> bool {
    query::addresses_zerotier(intent, known)
        && auth_retry_due(runtime.auth_state, runtime.last_auth_attempt, now)
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
    fn enable() -> Result<(), String> {
        RUNTIME.with(|cell| {
            let mut runtime = cell.borrow_mut();
            *runtime = Runtime::new();
            initialize(&mut runtime);
        });
        Ok(())
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
    runtime.last_auth_attempt = Some(Instant::now());
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
    if matches!(torchsnap_gadget_sdk::platform::current_os(), Os::Macos) {
        let _ = merge_saved_networks();
    }
}

fn merge_saved_networks() -> Result<usize, String> {
    let path =
        torchsnap_gadget_sdk::path_resolver::resolve("${xdg-config}/ZeroTier/saved_networks.json")
            .map_err(|e| format!("resolve saved_networks path: {e:?}"))?;
    let bytes =
        torchsnap_gadget_sdk::filesystem::read_file(&path).map_err(|e| format!("read: {e:?}"))?;
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

/// What an entry's actions do. Network commands carry the bare
/// network id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Command {
    /// Leave the network if joined, join it otherwise. Decided
    /// from live state when the action runs, not when the entry
    /// was built.
    Toggle(String),
    CopyId(String),
    /// Leave (if joined) and drop the history row.
    Forget(String),
    /// The fix for a missing or rejected token is the token
    /// field in the settings.
    OpenSettings,
}

// Query-only gadget: `entries()` keeps the trait's empty default.
impl Search for ZeroTierPlugin {
    type Command = Command;

    fn search(query: String, _matched_prefix: Option<String>) -> SearchResponse<Command> {
        let intent = query::intent_for(&query);
        if matches!(intent, Intent::None) {
            return SearchResponse::Nothing;
        }

        // Remembered networks come from the local history table,
        // so they are available even while the daemon is down.
        // They decide whether the query is about ZeroTier at all.
        let known = load_known_rows();
        let known_rows = merge_live_and_known(&[], &known);

        RUNTIME.with(|cell| {
            {
                let mut runtime = cell.borrow_mut();
                if should_retry_auth(&runtime, &intent, &known_rows, Instant::now()) {
                    initialize(&mut runtime);
                    runtime.network_cache.invalidate();
                }
            }

            let runtime = cell.borrow();
            let entries = build_search_entries(&runtime, &intent, &known, &known_rows);
            if entries.is_empty() {
                SearchResponse::Nothing
            } else {
                SearchResponse::Results(entries)
            }
        })
    }

    fn execute(command: Command) -> Result<PostAction, String> {
        match command {
            Command::OpenSettings => Ok(PostAction::OpenSettings),

            // Copy is independent of the daemon, network cache,
            // and history table, so it works even when auth isn't
            // configured (the user can still copy an id from a
            // synthetic-Join entry, for example).
            Command::CopyId(network_id) => {
                torchsnap_gadget_sdk::clipboard::write_text(&network_id)
                    .map_err(|e| format!("clipboard write: {e}"))?;
                Ok(PostAction::Dismiss)
            }

            Command::Toggle(network_id) => {
                change_membership(&network_id, |runtime, client, db, joined| {
                    if joined {
                        actions::disconnect(client, &network_id)
                    } else {
                        let name_hint = lookup_name_hint(runtime, &network_id);
                        actions::connect(client, db, &network_id, &name_hint, now_ms())
                    }
                })
            }

            Command::Forget(network_id) => {
                change_membership(&network_id, |_, client, db, joined| {
                    actions::forget(client, db, &network_id, joined)
                })
            }
        }
    }
}

/// Run a daemon-side change for one network. Hands `change` the
/// runtime, the API client, the history database and whether the
/// network is currently joined, and invalidates the network cache
/// afterwards so the next search shows the new state.
fn change_membership(
    network_id: &str,
    change: impl FnOnce(&Runtime, &Client, &SqlHandle, bool) -> Result<(), String>,
) -> Result<PostAction, String> {
    RUNTIME.with(|cell| {
        let runtime = cell.borrow_mut();
        let Some(client) = runtime.client.as_ref() else {
            return Err("ZeroTier auth not configured".into());
        };
        let client = client.clone();
        let db = history::connection();
        let live = current_live_state(&runtime);
        let currently_joined = live.iter().any(|n| n.id.eq_ignore_ascii_case(network_id));
        let result = change(&runtime, &client, &db, currently_joined);
        runtime.network_cache.invalidate();
        result?;
        Ok(PostAction::Dismiss)
    })
}

// =========================================================
// Search-entry assembly
// =========================================================

/// `known` is the raw history table; `known_rows` is the same
/// data as `KnownOnly` rows, used to decide whether a failure
/// entry applies before any daemon call.
fn build_search_entries(
    runtime: &Runtime,
    intent: &Intent,
    known: &[history::HistoryRow],
    known_rows: &[NetworkRow],
) -> Vec<ScoredEntry<Command>> {
    if let Some(failure) = failure_entry(runtime, intent, known_rows) {
        return vec![failure];
    }

    let live = current_live_state(runtime);
    let rows = merge_live_and_known(&live, known);

    match intent {
        Intent::None => Vec::new(),
        Intent::Match(q) => query::match_networks(q, &rows)
            .into_iter()
            .map(|m| scored_match_to_entry(&m))
            .collect(),
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

/// The warning entry for an unusable auth state, shown only
/// for queries about ZeroTier (see
/// [`query::addresses_zerotier`]) so unrelated searches stay
/// clean. Token problems link to the settings panel where the
/// token is pasted. A stopped daemon is informational only:
/// nothing in Torchsnap can start it, and `search()` retries
/// on its own once it runs.
fn failure_entry(
    runtime: &Runtime,
    intent: &Intent,
    known: &[NetworkRow],
) -> Option<ScoredEntry<Command>> {
    // Enter opens the settings; the entry has nothing else to do,
    // so the `open_settings` slot stays empty.
    let open_settings = || Actions::new().primary("Open settings", Command::OpenSettings);
    let (title, actions) = match runtime.auth_state {
        AuthState::Validated => return None,
        AuthState::Unconfigured => ("ZeroTier token not configured", open_settings()),
        AuthState::Rejected => ("ZeroTier authentication failed", open_settings()),
        AuthState::DaemonUnreachable => ("ZeroTier daemon not running", Actions::new()),
    };
    if !query::addresses_zerotier(intent, known) {
        return None;
    }
    Some(ScoredEntry {
        id: format!("failure:{title}"),
        title: title.to_string(),
        subtitle: None,
        icon: Some(EntryIcon::HeroIcon("exclamation-triangle".into())),
        score: 1,
        title_highlight_positions: vec![],
        subtitle_highlight_positions: vec![],
        actions,
    })
}

/// A network entry's actions: Enter joins or leaves (`label` says
/// which), Cmd+C copies the id, and Cmd+Backspace forgets the
/// network when `forget` is set.
fn network_actions(network_id: &str, label: &str, forget: bool) -> Actions<Command> {
    let actions = Actions::new()
        .primary(label, Command::Toggle(network_id.to_string()))
        .copy(Action::labeled(
            "Copy network id",
            Command::CopyId(network_id.to_string()),
        ));
    if forget {
        actions.delete(Action::labeled(
            "Forget",
            Command::Forget(network_id.to_string()),
        ))
    } else {
        actions
    }
}

// Bundled ZeroTier brand glyph in `assets/icon.svg`. The host
// bridge resolves this gadget-relative path to the launcher's
// `torchsnap-gadget://` asset URL automatically; see the WIT
// docs on `entry-icon::asset-icon`. The SVG itself is a CC0
// derivative — see `assets/ATTRIBUTIONS.md`.
fn network_icon() -> EntryIcon {
    EntryIcon::AssetIcon("assets/icon.svg".into())
}

// Title shape: `<network-name> · <ZeroTier action>`. The name
// leads so fuzzy-match highlights stay at the start and the
// network identity is what the user scans for; the action
// suffix disambiguates rows of the same name in different
// states (Connected vs. Stored).
const SUFFIX_DISCONNECT: &str = " · Disconnect ZeroTier Network";
const SUFFIX_CONNECT: &str = " · Connect to ZeroTier Network";

fn synthetic_connect_entry(id: &str) -> ScoredEntry<Command> {
    ScoredEntry {
        id: query::entry_id(id),
        title: format!("{id} · Join ZeroTier Network"),
        subtitle: Some("Not yet known — will join on activation".to_string()),
        icon: Some(network_icon()),
        score: query::SYNTHETIC_CONNECT_SCORE,
        title_highlight_positions: vec![],
        subtitle_highlight_positions: vec![],
        actions: network_actions(id, "Join", false),
    }
}

fn scored_match_to_entry(m: &ScoredMatch) -> ScoredEntry<Command> {
    let row = &m.row;
    let (subtitle, primary_label, suffix) = match row.state {
        NetworkState::Connected => {
            let addrs = row.assigned_addresses.join(", ");
            let subtitle = if addrs.is_empty() {
                "Connected".to_string()
            } else {
                format!("Connected · {addrs}")
            };
            (subtitle, "Disconnect", SUFFIX_DISCONNECT)
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
            (badge.to_string(), "Disconnect", SUFFIX_DISCONNECT)
        }
        NetworkState::KnownOnly => ("Stored".to_string(), "Connect", SUFFIX_CONNECT),
    };

    // Anonymous networks (no name set in Central, or first
    // sighting before the daemon fetched config) substitute
    // the id so the title never renders with a leading
    // separator and a void before it.
    let display_name: &str = if row.name.is_empty() {
        &row.id
    } else {
        &row.name
    };

    // Highlights came from matching `row.name`, which now
    // sits at the head of the title — no offset shift needed.
    let title_highlight_positions = m.title_highlight_positions.clone();

    ScoredEntry {
        id: query::entry_id(&row.id),
        title: format!("{display_name}{suffix}"),
        subtitle: Some(format!("{} · {}", subtitle, row.id)),
        icon: Some(network_icon()),
        score: m.score,
        title_highlight_positions,
        subtitle_highlight_positions: vec![],
        actions: network_actions(&row.id, primary_label, true),
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
    // The closure runs only on cache miss (TTL = 1s), so the
    // SQL upserts below fire at most once per second per
    // session — not per keystroke. `upsert_observed` itself
    // preserves any previously-captured name when the current
    // observation has no name yet, so calling it on every
    // observation is safe: it captures the name on whatever
    // refresh first carries one.
    let result = runtime.network_cache.get_or_fetch(move || {
        let outcome = client
            .list_networks()
            .map_err(|e| format!("list_networks: {e}"));
        if let Ok(nets) = &outcome {
            let db = history::connection();
            let now = now_ms();
            for net in nets {
                let _ = history::upsert_observed(&db, net, now);
            }
        }
        outcome
    });
    result.unwrap_or_default()
}

fn load_known_rows() -> Vec<history::HistoryRow> {
    let db = history::connection();
    history::list_all(&db).unwrap_or_default()
}

fn merge_live_and_known(live: &[Network], known: &[history::HistoryRow]) -> Vec<NetworkRow> {
    // Live state wins; known-only entries fill in.
    let mut by_id: std::collections::HashMap<String, NetworkRow> = std::collections::HashMap::new();
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
    if let Ok(rows) = history::list_all(&db)
        && let Some(r) = rows.iter().find(|r| r.id == id)
    {
        return r.name.clone();
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
    /// `true` on macOS so the settings UI can show the
    /// "Re-import from ZeroTier UI" button only on the
    /// platform where `saved_networks.json` exists.
    is_macos: bool,
}

#[derive(Serialize)]
struct KnownNetwork {
    id: String,
    name: String,
    last_seen: i64,
    last_status: Option<String>,
    /// `connected`, `joined-offline`, or `known-only`.
    /// Resolved by joining the history row against the
    /// cached live state — keeps the per-row status badge
    /// consistent between the launcher results and the
    /// settings list.
    state: &'static str,
}

/// Every method the ZeroTier settings panel calls through
/// `sendMessage`.
#[derive(Debug, serde::Deserialize, PartialEq)]
#[serde(tag = "method", content = "payload", rename_all = "snake_case")]
enum Request {
    /// Drop the cached live network list so the next search
    /// asks the daemon again.
    Refresh,
    /// Merge the networks from the daemon's saved-networks file
    /// into the history.
    Reimport,
    /// Leave (if joined) and forget one network.
    Forget { id: String },
    /// Delete the whole history.
    ClearAll,
    /// The current auth state only.
    ValidateToken,
    /// The auth state plus where the token came from.
    AuthState,
    /// Every known network with its live state.
    ListKnown,
}

impl Messaging for ZeroTierPlugin {
    type Request = Request;

    fn handle(request: Request) -> Result<serde_json::Value, String> {
        match request {
            Request::Refresh => {
                RUNTIME.with(|cell| {
                    cell.borrow_mut().network_cache.invalidate();
                });
                Ok(serde_json::json!({ "ok": true }))
            }

            Request::Reimport => {
                let inserted = merge_saved_networks().map_err(|e| e.to_string())?;
                Ok(serde_json::json!({ "inserted": inserted }))
            }

            Request::Forget { id } => RUNTIME.with(|cell| {
                let runtime = cell.borrow();
                let Some(client) = runtime.client.as_ref() else {
                    return Err("ZeroTier auth not configured".into());
                };
                let live = current_live_state(&runtime);
                let joined = live.iter().any(|n| n.id == id);
                actions::forget(client, &history::connection(), &id, joined)?;
                Ok(serde_json::json!({ "ok": true }))
            }),

            Request::ClearAll => {
                history::clear_all(&history::connection())?;
                Ok(serde_json::json!({ "ok": true }))
            }

            Request::ValidateToken => RUNTIME.with(|cell| {
                let runtime = cell.borrow();
                Ok(serde_json::json!({ "state": auth_state_name(runtime.auth_state) }))
            }),

            Request::AuthState => RUNTIME.with(|cell| {
                let runtime = cell.borrow();
                let source = match runtime.auth.as_ref().map(|a| a.source) {
                    Some(TokenSource::AutoDetected) => "auto",
                    Some(TokenSource::ManualPaste) => "manual",
                    _ => "none",
                };
                let is_macos = matches!(torchsnap_gadget_sdk::platform::current_os(), Os::Macos);
                serde_json::to_value(AuthStateResponse {
                    state: auth_state_name(runtime.auth_state),
                    source,
                    is_macos,
                })
                .map_err(|e| e.to_string())
            }),

            Request::ListKnown => RUNTIME.with(|cell| {
                let runtime = cell.borrow();
                let live = current_live_state(&runtime);
                let known = load_known_rows();
                let mut by_id = std::collections::HashMap::new();
                for net in &live {
                    by_id.insert(net.id.clone(), net);
                }
                let networks: Vec<KnownNetwork> = known
                    .iter()
                    .map(|row| {
                        let state = match by_id.get(&row.id) {
                            Some(net) if net.status == NetworkStatus::Ok => "connected",
                            Some(_) => "joined-offline",
                            None => "known-only",
                        };
                        KnownNetwork {
                            id: row.id.clone(),
                            name: row.name.clone(),
                            last_seen: row.last_seen,
                            last_status: row.last_status.clone(),
                            state,
                        }
                    })
                    .collect();
                serde_json::to_value(networks).map_err(|e| e.to_string())
            }),
        }
    }
}

/// The auth state as the settings panel names it.
fn auth_state_name(state: AuthState) -> &'static str {
    match state {
        AuthState::Validated => "validated",
        AuthState::Rejected => "rejected",
        AuthState::Unconfigured => "unconfigured",
        AuthState::DaemonUnreachable => "daemon-unreachable",
    }
}

impl_noop_tasks!(ZeroTierPlugin);

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;
    use torchsnap_gadget_sdk::messaging;

    #[test]
    fn settings_requests_decode_from_the_payloads_the_panel_sends() {
        assert_eq!(
            messaging::decode_request("forget", r#"{"id":"8056c2e21c000001"}"#),
            Ok(Request::Forget {
                id: "8056c2e21c000001".into()
            })
        );
        for (method, request) in [
            ("refresh", Request::Refresh),
            ("reimport", Request::Reimport),
            ("clear_all", Request::ClearAll),
            ("validate_token", Request::ValidateToken),
            ("auth_state", Request::AuthState),
            ("list_known", Request::ListKnown),
        ] {
            assert_eq!(
                messaging::decode_request(method, "{}"),
                Ok(request),
                "{method}"
            );
        }
    }

    #[test]
    fn forget_without_an_id_is_rejected() {
        messaging::decode_request::<Request>("forget", "{}").expect_err("id is required");
    }

    #[test]
    fn auth_state_names_match_the_settings_panel() {
        assert_eq!(auth_state_name(AuthState::Validated), "validated");
        assert_eq!(auth_state_name(AuthState::Rejected), "rejected");
        assert_eq!(auth_state_name(AuthState::Unconfigured), "unconfigured");
        assert_eq!(
            auth_state_name(AuthState::DaemonUnreachable),
            "daemon-unreachable"
        );
    }

    fn runtime_with(state: AuthState) -> Runtime {
        let mut runtime = Runtime::new();
        runtime.auth_state = state;
        runtime
    }

    const FAILING_STATES: [AuthState; 3] = [
        AuthState::Unconfigured,
        AuthState::Rejected,
        AuthState::DaemonUnreachable,
    ];

    fn id_intent() -> Intent {
        Intent::JoinById("abcdef0123456789".into())
    }

    fn remembered(name: &str) -> NetworkRow {
        NetworkRow {
            id: "aaaa000000000001".into(),
            name: name.into(),
            state: NetworkState::KnownOnly,
            assigned_addresses: vec![],
        }
    }

    // ---- failure_entry ---------------------------------------

    #[test]
    fn unrelated_query_shows_no_failure_entry() {
        let known = [remembered("starling-lab")];
        for state in FAILING_STATES {
            let runtime = runtime_with(state);
            let intent = Intent::Match("firefox".into());
            assert!(
                failure_entry(&runtime, &intent, &known).is_none(),
                "{state:?}"
            );
        }
    }

    #[test]
    fn empty_query_shows_no_failure_entry() {
        for state in FAILING_STATES {
            let runtime = runtime_with(state);
            assert!(
                failure_entry(&runtime, &Intent::None, &[]).is_none(),
                "{state:?}"
            );
        }
    }

    #[test]
    fn zerotier_queries_show_the_failure_entry() {
        let known = [remembered("starling-lab")];
        let intents = [
            id_intent(),
            Intent::Match("zerotier".into()),
            Intent::Match("starling".into()),
        ];
        for state in FAILING_STATES {
            let runtime = runtime_with(state);
            for intent in &intents {
                assert!(
                    failure_entry(&runtime, intent, &known).is_some(),
                    "{state:?} {intent:?}"
                );
            }
        }
    }

    #[test]
    fn validated_auth_shows_no_failure_entry() {
        let runtime = runtime_with(AuthState::Validated);
        assert!(failure_entry(&runtime, &id_intent(), &[]).is_none());
    }

    #[test]
    fn network_actions_toggle_copy_and_forget_the_network() {
        let actions = network_actions("8056c2e21c000001", "Disconnect", true);
        assert_eq!(
            actions.primary,
            Some(Action::labeled(
                "Disconnect",
                Command::Toggle("8056c2e21c000001".into())
            ))
        );
        assert_eq!(
            actions.copy,
            Some(Action::labeled(
                "Copy network id",
                Command::CopyId("8056c2e21c000001".into())
            ))
        );
        assert_eq!(
            actions.delete,
            Some(Action::labeled(
                "Forget",
                Command::Forget("8056c2e21c000001".into())
            ))
        );
        assert_eq!(actions.iter().count(), 3);
    }

    #[test]
    fn synthetic_join_entry_cannot_be_forgotten() {
        let entry = synthetic_connect_entry("8056c2e21c000001");
        assert_eq!(entry.id, query::entry_id("8056c2e21c000001"));
        assert_eq!(
            entry.actions.primary.map(|a| a.command),
            Some(Command::Toggle("8056c2e21c000001".into()))
        );
        assert!(entry.actions.copy.is_some());
        assert!(entry.actions.delete.is_none());
    }

    #[test]
    fn daemon_unreachable_entry_has_no_action() {
        let runtime = runtime_with(AuthState::DaemonUnreachable);
        let entry =
            failure_entry(&runtime, &id_intent(), &[]).expect("an id query addresses ZeroTier");
        assert_eq!(entry.title, "ZeroTier daemon not running");
        assert_eq!(entry.actions.iter().count(), 0);
    }

    #[test]
    fn token_problems_link_to_the_settings() {
        for (state, title) in [
            (AuthState::Unconfigured, "ZeroTier token not configured"),
            (AuthState::Rejected, "ZeroTier authentication failed"),
        ] {
            let runtime = runtime_with(state);
            let entry =
                failure_entry(&runtime, &id_intent(), &[]).expect("an id query addresses ZeroTier");
            assert_eq!(entry.title, title);
            assert_eq!(
                entry.actions.primary,
                Some(Action::labeled("Open settings", Command::OpenSettings))
            );
            assert_eq!(entry.actions.iter().count(), 1);
        }
    }

    #[test]
    fn open_settings_asks_the_host_to_open_the_settings() {
        let runtime = runtime_with(AuthState::Unconfigured);
        let entry =
            failure_entry(&runtime, &id_intent(), &[]).expect("an id query addresses ZeroTier");
        let command = entry.actions.primary.expect("settings action").command;
        let post_action = <ZeroTierPlugin as Search>::execute(command);
        assert_eq!(post_action, Ok(PostAction::OpenSettings));
    }

    #[test]
    fn failure_entry_ranks_below_every_network_result() {
        let runtime = runtime_with(AuthState::DaemonUnreachable);
        let entry =
            failure_entry(&runtime, &id_intent(), &[]).expect("an id query addresses ZeroTier");
        assert!(entry.score < query::SYNTHETIC_CONNECT_SCORE);
        assert!(entry.id.starts_with("failure:"));
    }

    // ---- auth_retry_due --------------------------------------

    #[test]
    fn failed_auth_is_retried_once_the_interval_has_passed() {
        let now = Instant::now();
        let recent = now
            .checked_sub(Duration::from_secs(1))
            .expect("process has run for longer than a second");
        let stale = now
            .checked_sub(AUTH_RETRY_INTERVAL)
            .expect("process has run for longer than the retry interval");

        assert!(!auth_retry_due(
            AuthState::DaemonUnreachable,
            Some(recent),
            now
        ));
        assert!(auth_retry_due(
            AuthState::DaemonUnreachable,
            Some(stale),
            now
        ));
        assert!(auth_retry_due(AuthState::Unconfigured, None, now));
    }

    #[test]
    fn validated_auth_is_never_retried() {
        assert!(!auth_retry_due(AuthState::Validated, None, Instant::now()));
    }

    // ---- should_retry_auth -----------------------------------

    #[test]
    fn zerotier_query_retries_failed_auth() {
        let runtime = runtime_with(AuthState::DaemonUnreachable);
        let known = [remembered("starling-lab")];
        for intent in [
            id_intent(),
            Intent::Match("zero".into()),
            Intent::Match("starling".into()),
        ] {
            assert!(
                should_retry_auth(&runtime, &intent, &known, Instant::now()),
                "{intent:?}"
            );
        }
    }

    #[test]
    fn unrelated_query_never_retries_auth() {
        let runtime = runtime_with(AuthState::DaemonUnreachable);
        let known = [remembered("starling-lab")];
        for intent in [Intent::None, Intent::Match("firefox".into())] {
            assert!(
                !should_retry_auth(&runtime, &intent, &known, Instant::now()),
                "{intent:?}"
            );
        }
    }

    #[test]
    fn retry_respects_the_interval_after_an_attempt() {
        let now = Instant::now();
        let mut runtime = runtime_with(AuthState::DaemonUnreachable);
        runtime.last_auth_attempt = Some(now);
        assert!(!should_retry_auth(&runtime, &id_intent(), &[], now));
    }

    #[test]
    fn validated_auth_is_not_retried_for_zerotier_queries() {
        let runtime = runtime_with(AuthState::Validated);
        assert!(!should_retry_auth(
            &runtime,
            &id_intent(),
            &[],
            Instant::now()
        ));
    }
}
