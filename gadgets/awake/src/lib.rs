// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Awake gadget entry point.
//!
//! Surfaces keep-awake control through the launcher: the
//! gadget answers keyword queries such as "awake" and
//! "amphetamine" and lets the user start or stop a session
//! that prevents the system from sleeping.
//!
//! Sessions are controlled through a pluggable keep-awake
//! backend so the mechanism stays decoupled from the launcher
//! surface. The only backend today drives Amphetamine.app on
//! macOS through AppleScript. On other platforms, or when
//! Amphetamine is not installed, the gadget stays silent and
//! contributes no launcher entries. Backend availability is
//! probed once when the gadget is enabled.

use std::cell::RefCell;

use serde::{Deserialize, Serialize};
use torchsnap_gadget_sdk::cache::{DEFAULT_TTL, RateLimitCache};
use torchsnap_gadget_sdk::prelude::*;

mod backend;
mod query;

use backend::{
    KeepAwakeBackend, SessionKind, SessionRequest, SessionStatus, select_backend,
};
use query::{Query, StartRequest};

struct AwakeGadget;
define_gadget!(AwakeGadget);

// =========================================================
// Per-instance runtime state
//
// `wasm32-wasip2` is single-threaded per guest instance, so
// `thread_local!` is effectively per-instance state. `RefCell`
// interior mutability lets the `&mut`-free guest trait methods
// (`fn enable()`, `fn search(query, ...)`) mutate state through
// borrows.
// =========================================================

thread_local! {
    static RUNTIME: RefCell<Runtime> = RefCell::new(Runtime::new());
}

struct Runtime {
    /// The selected keep-awake backend, or `None` when no
    /// supported mechanism is available on this host (non-macOS,
    /// or Amphetamine not installed). A `None` backend makes the
    /// gadget silent: `search()` contributes nothing.
    backend: Option<Box<dyn KeepAwakeBackend>>,
    /// Cached status probe for up to `DEFAULT_TTL`. The slot
    /// stores the full `Result` so cache hits keep the backend's
    /// error context, which `search()` renders as an error entry.
    status_cache: RateLimitCache<Result<SessionStatus, String>>,
}

impl Runtime {
    fn new() -> Self {
        Self {
            backend: None,
            status_cache: RateLimitCache::new(DEFAULT_TTL),
        }
    }
}

// =========================================================
// Lifecycle
// =========================================================

impl LifecycleGuest for AwakeGadget {
    fn enable() -> Result<(), String> {
        RUNTIME.with(|cell| {
            let mut runtime = cell.borrow_mut();
            *runtime = Runtime::new();
            runtime.backend = select_backend();
        });
        // Selection failure is not an error: a host without a
        // supported backend simply runs the gadget silently.
        Ok(())
    }

    fn disable() {
        RUNTIME.with(|cell| {
            *cell.borrow_mut() = Runtime::new();
        });
    }

    fn on_setting_changed(_key: String, _value: String) {}
}

// =========================================================
// Execute-time payload
//
// The single action on each entry round-trips this operation
// through the host's opaque `data` field: `search()` encodes it,
// `execute()` decodes it and drives the backend accordingly.
// =========================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Op {
    Start {
        minutes: Option<u32>,
        display_sleep_allowed: bool,
    },
    Stop,
}

// =========================================================
// Entry model
// =========================================================

/// Base score for every awake entry. Sits between zerotier's
/// Connected tier (750) and its known-only tier (250); bang
/// launches pin to 1000, and host frecency stacks on top of this
/// base, so a frequently-used awake entry rises without
/// out-shouting a live network match.
const BASE_SCORE: u32 = 500;

/// Stable entry ids the host tracks for frecency. One id per
/// entry *shape* rather than per query, so repeated starts share
/// history regardless of the exact duration typed.
const ID_START: &str = "awake:start";
const ID_STOP: &str = "awake:stop";
const ID_ERROR: &str = "awake:error";

/// Subtitle shown when the keyword matched but the arguments did
/// not parse — a syntax reminder in place of the status line.
const HINT_SUBTITLE: &str = r#"Type a duration: "awake 30m", "awake 2h display""#;

/// The pure description of one launcher entry, produced without
/// touching any host type so the title / subtitle / action logic
/// is unit-testable in isolation. [`spec_to_entry`] maps it onto
/// the host's [`ScoredEntry`].
#[derive(Debug, Clone, PartialEq, Eq)]
struct EntrySpec {
    /// One of the stable `awake:*` ids.
    id: &'static str,
    title: String,
    subtitle: String,
    /// `Some((label, op))` for an actionable entry. `None` for
    /// the error entry, which carries no action and no payload so
    /// the UI cannot execute it.
    action: Option<(String, Op)>,
}

// =========================================================
// Pure entry construction
//
// Everything below produces `EntrySpec`s from a status probe and
// a parsed query. No host imports appear here.
// =========================================================

/// Render a whole-minute duration as natural words: `30 minutes`,
/// `1 hour`, `1 hour 30 minutes`, `2 hours`, `1 minute`.
fn duration_words(minutes: u32) -> String {
    let hours = minutes / 60;
    let mins = minutes % 60;
    let mut parts: Vec<String> = Vec::new();
    if hours > 0 {
        let unit = if hours == 1 { "hour" } else { "hours" };
        parts.push(format!("{hours} {unit}"));
    }
    if mins > 0 {
        let unit = if mins == 1 { "minute" } else { "minutes" };
        parts.push(format!("{mins} {unit}"));
    }
    parts.join(" ")
}

/// Render seconds remaining as a compact badge: `1h 28m`, `1h`,
/// `1m`. Anything under a minute rounds down to `<1m` rather than
/// implying a precision the countdown does not warrant.
fn remaining_words(secs: u32) -> String {
    if secs < 60 {
        return "<1m".to_string();
    }
    let total_minutes = secs / 60;
    let hours = total_minutes / 60;
    let mins = total_minutes % 60;
    let mut parts: Vec<String> = Vec::new();
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if mins > 0 {
        parts.push(format!("{mins}m"));
    }
    parts.join(" ")
}

/// The descriptor tail for an active session: the part after the
/// leading `Active · ` / `Replaces: ` framing, and before the
/// display-sleep suffix.
fn descriptor_tail(kind: SessionKind) -> String {
    match kind {
        SessionKind::Timed { remaining_secs } => {
            format!("{} remaining", remaining_words(remaining_secs))
        }
        SessionKind::Infinite => "infinite session".to_string(),
        SessionKind::External => "managed by Amphetamine — may restart via Trigger".to_string(),
    }
}

/// The `Active · …` subtitle describing the running session,
/// appending the display-sleep note when the session permits the
/// display to sleep.
fn active_descriptor(kind: SessionKind, display_sleep_allowed: bool) -> String {
    let mut descriptor = format!("Active · {}", descriptor_tail(kind));
    if display_sleep_allowed {
        descriptor.push_str(" · display may sleep");
    }
    descriptor
}

/// The `Replaces: …` subtitle for a restart entry, describing the
/// session the start would replace.
fn replaces_descriptor(kind: SessionKind, display_sleep_allowed: bool) -> String {
    let mut descriptor = format!("Replaces: {}", descriptor_tail(kind));
    if display_sleep_allowed {
        descriptor.push_str(" · display may sleep");
    }
    descriptor
}

/// The title for a start / restart request, timed or infinite.
fn start_title(minutes: Option<u32>) -> String {
    match minutes {
        Some(minutes) => format!("Keep Mac awake for {}", duration_words(minutes)),
        None => "Keep Mac awake".to_string(),
    }
}

/// The status / toggle entry a bare keyword produces for the
/// current status. `Query::Status` uses this verbatim;
/// `Query::Hint` reuses it and swaps in the syntax hint subtitle.
fn status_spec(status: &SessionStatus) -> EntrySpec {
    match status {
        SessionStatus::Inactive | SessionStatus::AppNotRunning => EntrySpec {
            id: ID_START,
            title: "Keep Mac awake".to_string(),
            subtitle: "No active session · starts an infinite session".to_string(),
            action: Some((
                "Start".to_string(),
                Op::Start {
                    minutes: None,
                    display_sleep_allowed: false,
                },
            )),
        },
        SessionStatus::Active {
            kind,
            display_sleep_allowed,
        } => EntrySpec {
            id: ID_STOP,
            title: "End keep-awake session".to_string(),
            subtitle: active_descriptor(*kind, *display_sleep_allowed),
            action: Some(("End session".to_string(), Op::Stop)),
        },
    }
}

/// The entry for a well-formed start request. When a session is
/// already running the start becomes a restart, replacing it.
fn start_spec(status: &SessionStatus, req: &StartRequest) -> EntrySpec {
    let title = start_title(req.minutes);
    let op = Op::Start {
        minutes: req.minutes,
        display_sleep_allowed: req.display_sleep_allowed,
    };
    match status {
        SessionStatus::Inactive | SessionStatus::AppNotRunning => {
            let mut subtitle = match req.minutes {
                Some(_) => "Starts a new session".to_string(),
                None => "Starts an infinite session".to_string(),
            };
            if req.display_sleep_allowed {
                subtitle.push_str(" · display may sleep");
            }
            EntrySpec {
                id: ID_START,
                title,
                subtitle,
                action: Some(("Start".to_string(), op)),
            }
        }
        SessionStatus::Active {
            kind,
            display_sleep_allowed,
        } => EntrySpec {
            id: ID_START,
            title: format!("Restart: {title}"),
            // The display-sleep note reflects the session being
            // replaced, not the incoming request.
            subtitle: replaces_descriptor(*kind, *display_sleep_allowed),
            action: Some(("Start".to_string(), op)),
        },
    }
}

/// Build the single entry for a matched query. `parsed` is never
/// `Query::NoMatch`: `search()` short-circuits that to
/// `SearchResponse::Nothing` before reaching here.
///
/// A backend error overrides every parsed case with a
/// non-actionable error entry so the failure is visible but
/// cannot be executed.
fn build_spec(status: &Result<SessionStatus, String>, parsed: &Query) -> EntrySpec {
    let status = match status {
        Ok(status) => status,
        Err(message) => {
            return EntrySpec {
                id: ID_ERROR,
                title: "Amphetamine error".to_string(),
                subtitle: message.clone(),
                action: None,
            };
        }
    };

    match parsed {
        Query::NoMatch => unreachable!("NoMatch is filtered out before build_spec"),
        Query::Status => status_spec(status),
        Query::Hint => {
            let mut spec = status_spec(status);
            spec.subtitle = HINT_SUBTITLE.to_string();
            spec
        }
        Query::Start(req) => start_spec(status, req),
    }
}

// =========================================================
// Host-type mapping
// =========================================================

/// The fallback icon for entries with no real application to
/// point at: the error entry, and any entry the backend does not
/// supply its own icon for.
fn default_icon() -> EntryIcon {
    EntryIcon::HeroIcon("bolt".to_string())
}

/// Choose the icon for the entry `search()` is about to build.
/// A backend error overrides `backend_icon` with the default:
/// the error entry does not represent the backend's application,
/// so it never shows that application's icon. Otherwise the
/// backend's icon wins, falling back to the default when the
/// backend does not supply one.
fn resolve_icon(
    status: &Result<SessionStatus, String>,
    backend_icon: Option<EntryIcon>,
) -> EntryIcon {
    match status {
        Err(_) => default_icon(),
        Ok(_) => backend_icon.unwrap_or_else(default_icon),
    }
}

/// Map the pure [`EntrySpec`] onto the host's [`ScoredEntry`],
/// carrying the caller-chosen `icon`. Kept a pure mapper: the
/// backend-dependent icon choice lives in `search()`.
fn spec_to_entry(spec: EntrySpec, icon: EntryIcon) -> ScoredEntry {
    let (actions, data) = match spec.action {
        Some((label, op)) => (
            vec![Action {
                id: ActionId::Open,
                label,
            }],
            // `Op` is a small serde enum with no float or map-key
            // hazards, so JSON serialization cannot fail.
            Some(data::encode(&op).expect("Op serializes to JSON")),
        ),
        None => (Vec::new(), None),
    };

    ScoredEntry {
        id: spec.id.to_string(),
        title: spec.title,
        subtitle: Some(spec.subtitle),
        icon: Some(icon),
        score: BASE_SCORE,
        title_highlight_positions: Vec::new(),
        subtitle_highlight_positions: Vec::new(),
        actions,
        data,
    }
}

// =========================================================
// Search
// =========================================================

impl SearchGuest for AwakeGadget {
    fn entries() -> Vec<CatalogEntry> {
        vec![]
    }

    fn search(query: String, _matched_prefix: Option<String>) -> SearchResponse {
        let parsed = query::parse(&query);
        if matches!(parsed, Query::NoMatch) {
            return SearchResponse::Nothing;
        }

        RUNTIME.with(|cell| {
            let runtime = cell.borrow();

            // No backend means the gadget stays silent even for a
            // matched keyword: an uninstalled or non-macOS host
            // hides the surface rather than advertising a control
            // it cannot honour.
            let Some(backend) = runtime.backend.as_ref() else {
                return SearchResponse::Nothing;
            };

            let status = runtime.status_cache.get_or_fetch(|| backend.status());
            let icon = resolve_icon(&status, backend.entry_icon());
            let entry = spec_to_entry(build_spec(&status, &parsed), icon);
            SearchResponse::Results(vec![entry])
        })
    }

    fn execute(entry: ScoredEntry, action_id: ActionId) -> Result<PostAction, String> {
        // Every awake entry carries a single `Open` action; the
        // host should never route another kind here.
        if !matches!(action_id, ActionId::Open) {
            return Err(format!("unsupported action: {action_id:?}"));
        }

        // The error entry carries no payload; a missing or
        // undecodable payload therefore means "not executable".
        let Some(payload) = entry.data.as_ref() else {
            return Err("entry carries no executable action".to_string());
        };
        let op: Op = data::decode(payload)?;

        RUNTIME.with(|cell| {
            let runtime = cell.borrow();
            let Some(backend) = runtime.backend.as_ref() else {
                return Err("keep-awake backend not available".to_string());
            };

            match op {
                Op::Start {
                    minutes,
                    display_sleep_allowed,
                } => backend.start(&SessionRequest {
                    minutes,
                    display_sleep_allowed,
                })?,
                Op::Stop => backend.stop()?,
            }

            // Force the next search to observe the new session
            // state immediately rather than waiting out the TTL.
            runtime.status_cache.invalidate();
            Ok(PostAction::Dismiss)
        })
    }
}

impl_noop_messaging!(AwakeGadget);
impl_noop_tasks!(AwakeGadget);

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ----- construction helpers ------------------------------

    fn timed(remaining_secs: u32, display: bool) -> Result<SessionStatus, String> {
        Ok(SessionStatus::Active {
            kind: SessionKind::Timed { remaining_secs },
            display_sleep_allowed: display,
        })
    }

    fn infinite(display: bool) -> Result<SessionStatus, String> {
        Ok(SessionStatus::Active {
            kind: SessionKind::Infinite,
            display_sleep_allowed: display,
        })
    }

    fn external(display: bool) -> Result<SessionStatus, String> {
        Ok(SessionStatus::Active {
            kind: SessionKind::External,
            display_sleep_allowed: display,
        })
    }

    fn inactive() -> Result<SessionStatus, String> {
        Ok(SessionStatus::Inactive)
    }

    fn app_not_running() -> Result<SessionStatus, String> {
        Ok(SessionStatus::AppNotRunning)
    }

    fn start_query(minutes: Option<u32>, display: bool) -> Query {
        Query::Start(StartRequest {
            minutes,
            display_sleep_allowed: display,
        })
    }

    fn start_action(minutes: Option<u32>, display: bool) -> Option<(String, Op)> {
        Some((
            "Start".to_string(),
            Op::Start {
                minutes,
                display_sleep_allowed: display,
            },
        ))
    }

    // ----- Status × status -----------------------------------

    #[test]
    fn status_inactive_offers_infinite_start() {
        let spec = build_spec(&inactive(), &Query::Status);
        assert_eq!(spec.id, ID_START);
        assert_eq!(spec.title, "Keep Mac awake");
        assert_eq!(spec.subtitle, "No active session · starts an infinite session");
        assert_eq!(spec.action, start_action(None, false));
    }

    #[test]
    fn status_app_not_running_offers_infinite_start() {
        let spec = build_spec(&app_not_running(), &Query::Status);
        assert_eq!(spec.id, ID_START);
        assert_eq!(spec.title, "Keep Mac awake");
        assert_eq!(spec.subtitle, "No active session · starts an infinite session");
        assert_eq!(spec.action, start_action(None, false));
    }

    #[test]
    fn status_active_timed_offers_stop() {
        let spec = build_spec(&timed(5281, false), &Query::Status);
        assert_eq!(spec.id, ID_STOP);
        assert_eq!(spec.title, "End keep-awake session");
        assert_eq!(spec.subtitle, "Active · 1h 28m remaining");
        assert_eq!(spec.action, Some(("End session".to_string(), Op::Stop)));
    }

    #[test]
    fn status_active_infinite_offers_stop() {
        let spec = build_spec(&infinite(false), &Query::Status);
        assert_eq!(spec.id, ID_STOP);
        assert_eq!(spec.title, "End keep-awake session");
        assert_eq!(spec.subtitle, "Active · infinite session");
        assert_eq!(spec.action, Some(("End session".to_string(), Op::Stop)));
    }

    #[test]
    fn status_active_external_offers_stop() {
        let spec = build_spec(&external(false), &Query::Status);
        assert_eq!(spec.id, ID_STOP);
        assert_eq!(spec.title, "End keep-awake session");
        assert_eq!(
            spec.subtitle,
            "Active · managed by Amphetamine — may restart via Trigger"
        );
        assert_eq!(spec.action, Some(("End session".to_string(), Op::Stop)));
    }

    #[test]
    fn status_active_timed_display_allowed_appends_display_note() {
        let spec = build_spec(&timed(5281, true), &Query::Status);
        assert_eq!(spec.subtitle, "Active · 1h 28m remaining · display may sleep");
    }

    // ----- Start × Inactive ----------------------------------

    #[test]
    fn start_timed_inactive() {
        let spec = build_spec(&inactive(), &start_query(Some(30), false));
        assert_eq!(spec.id, ID_START);
        assert_eq!(spec.title, "Keep Mac awake for 30 minutes");
        assert_eq!(spec.subtitle, "Starts a new session");
        assert_eq!(spec.action, start_action(Some(30), false));
    }

    #[test]
    fn start_timed_display_inactive() {
        let spec = build_spec(&inactive(), &start_query(Some(120), true));
        assert_eq!(spec.title, "Keep Mac awake for 2 hours");
        assert_eq!(spec.subtitle, "Starts a new session · display may sleep");
        assert_eq!(spec.action, start_action(Some(120), true));
    }

    #[test]
    fn start_infinite_inactive() {
        let spec = build_spec(&inactive(), &start_query(None, false));
        assert_eq!(spec.id, ID_START);
        assert_eq!(spec.title, "Keep Mac awake");
        assert_eq!(spec.subtitle, "Starts an infinite session");
        assert_eq!(spec.action, start_action(None, false));
    }

    #[test]
    fn start_infinite_display_inactive() {
        let spec = build_spec(&inactive(), &start_query(None, true));
        assert_eq!(spec.title, "Keep Mac awake");
        assert_eq!(spec.subtitle, "Starts an infinite session · display may sleep");
        assert_eq!(spec.action, start_action(None, true));
    }

    // ----- Start × Active (restart) --------------------------

    #[test]
    fn start_timed_over_active_timed_is_restart() {
        let spec = build_spec(&timed(5281, false), &start_query(Some(30), false));
        assert_eq!(spec.id, ID_START);
        assert_eq!(spec.title, "Restart: Keep Mac awake for 30 minutes");
        assert_eq!(spec.subtitle, "Replaces: 1h 28m remaining");
        assert_eq!(spec.action, start_action(Some(30), false));
    }

    #[test]
    fn start_infinite_over_active_infinite_is_restart() {
        let spec = build_spec(&infinite(false), &start_query(None, false));
        assert_eq!(spec.title, "Restart: Keep Mac awake");
        assert_eq!(spec.subtitle, "Replaces: infinite session");
        assert_eq!(spec.action, start_action(None, false));
    }

    #[test]
    fn start_over_active_external_is_restart() {
        let spec = build_spec(&external(false), &start_query(Some(90), false));
        assert_eq!(spec.title, "Restart: Keep Mac awake for 1 hour 30 minutes");
        assert_eq!(
            spec.subtitle,
            "Replaces: managed by Amphetamine — may restart via Trigger"
        );
        assert_eq!(spec.action, start_action(Some(90), false));
    }

    #[test]
    fn restart_display_note_reflects_active_session_not_request() {
        // Request has display=false, but the running session has
        // display=true: the note comes from the session replaced.
        let spec = build_spec(&timed(5281, true), &start_query(Some(30), false));
        assert_eq!(spec.subtitle, "Replaces: 1h 28m remaining · display may sleep");
        assert_eq!(spec.action, start_action(Some(30), false));
    }

    // ----- Hint × status -------------------------------------

    #[test]
    fn hint_inactive_keeps_start_shape_with_hint_subtitle() {
        let spec = build_spec(&inactive(), &Query::Hint);
        assert_eq!(spec.id, ID_START);
        assert_eq!(spec.title, "Keep Mac awake");
        assert_eq!(spec.subtitle, HINT_SUBTITLE);
        assert_eq!(spec.action, start_action(None, false));
    }

    #[test]
    fn hint_active_keeps_stop_shape_with_hint_subtitle() {
        let spec = build_spec(&infinite(false), &Query::Hint);
        assert_eq!(spec.id, ID_STOP);
        assert_eq!(spec.title, "End keep-awake session");
        assert_eq!(spec.subtitle, HINT_SUBTITLE);
        assert_eq!(spec.action, Some(("End session".to_string(), Op::Stop)));
    }

    // ----- Err × parsed --------------------------------------

    #[test]
    fn error_status_has_no_action() {
        let status: Result<SessionStatus, String> = Err("boom".to_string());
        let spec = build_spec(&status, &Query::Status);
        assert_eq!(spec.id, ID_ERROR);
        assert_eq!(spec.title, "Amphetamine error");
        assert_eq!(spec.subtitle, "boom");
        assert_eq!(spec.action, None);
    }

    #[test]
    fn error_start_has_no_action() {
        let status: Result<SessionStatus, String> = Err("boom".to_string());
        let spec = build_spec(&status, &start_query(Some(30), false));
        assert_eq!(spec.id, ID_ERROR);
        assert_eq!(spec.action, None);
    }

    #[test]
    fn error_hint_has_no_action() {
        let status: Result<SessionStatus, String> = Err("boom".to_string());
        let spec = build_spec(&status, &Query::Hint);
        assert_eq!(spec.id, ID_ERROR);
        assert_eq!(spec.action, None);
    }

    // ----- duration words ------------------------------------

    #[test]
    fn duration_words_formats() {
        assert_eq!(duration_words(30), "30 minutes");
        assert_eq!(duration_words(60), "1 hour");
        assert_eq!(duration_words(90), "1 hour 30 minutes");
        assert_eq!(duration_words(120), "2 hours");
        assert_eq!(duration_words(1), "1 minute");
        assert_eq!(duration_words(61), "1 hour 1 minute");
    }

    // ----- remaining words -----------------------------------

    #[test]
    fn remaining_words_formats() {
        assert_eq!(remaining_words(5281), "1h 28m");
        assert_eq!(remaining_words(119), "1m");
        assert_eq!(remaining_words(3600), "1h");
        assert_eq!(remaining_words(45), "<1m");
        assert_eq!(remaining_words(60), "1m");
    }

    // ----- Op serde round-trip -------------------------------

    #[test]
    fn op_round_trips_through_data_codec() {
        for op in [
            Op::Start {
                minutes: Some(30),
                display_sleep_allowed: false,
            },
            Op::Start {
                minutes: None,
                display_sleep_allowed: true,
            },
            Op::Stop,
        ] {
            let encoded = data::encode(&op).expect("encode");
            let decoded: Op = data::decode(&encoded).expect("decode");
            assert_eq!(decoded, op);
        }
    }

    // ----- host-type mapping ---------------------------------

    /// A distinctive non-default icon, standing in for whatever a
    /// backend's `entry_icon()` returns, so tests can tell "the
    /// icon `spec_to_entry` was given" apart from "the default
    /// bolt icon" by construction rather than by value overlap.
    fn app_icon() -> EntryIcon {
        EntryIcon::AppIcon("com.if.Amphetamine".to_string())
    }

    fn assert_is_app_icon(icon: &Option<EntryIcon>) {
        assert!(
            matches!(icon, Some(EntryIcon::AppIcon(id)) if id == "com.if.Amphetamine"),
            "expected the icon passed to spec_to_entry, got {icon:?}"
        );
    }

    #[test]
    fn every_entry_shape_carries_base_score_and_the_icon_passed_in() {
        let specs = [
            build_spec(&inactive(), &Query::Status),
            build_spec(&infinite(false), &Query::Status),
            build_spec(&inactive(), &start_query(Some(30), false)),
            build_spec(&timed(5281, false), &start_query(None, false)),
            build_spec(&inactive(), &Query::Hint),
            build_spec(&Err("boom".to_string()), &Query::Status),
        ];
        for spec in specs {
            let entry = spec_to_entry(spec, app_icon());
            assert_eq!(entry.score, BASE_SCORE);
            assert_is_app_icon(&entry.icon);
        }
    }

    #[test]
    fn start_shaped_entries_use_start_id_and_carry_payload() {
        for spec in [
            build_spec(&inactive(), &Query::Status),
            build_spec(&inactive(), &start_query(Some(30), false)),
            build_spec(&timed(5281, false), &start_query(None, false)),
            build_spec(&inactive(), &Query::Hint),
        ] {
            let entry = spec_to_entry(spec, default_icon());
            assert_eq!(entry.id, ID_START);
            assert!(entry.data.is_some());
            assert_eq!(entry.actions.len(), 1);
        }
    }

    #[test]
    fn stop_shaped_entries_use_stop_id() {
        for spec in [
            build_spec(&infinite(false), &Query::Status),
            build_spec(&timed(5281, false), &Query::Hint),
        ] {
            let entry = spec_to_entry(spec, default_icon());
            assert_eq!(entry.id, ID_STOP);
            assert!(entry.data.is_some());
        }
    }

    #[test]
    fn error_entry_carries_no_action_or_payload() {
        let entry = spec_to_entry(
            build_spec(&Err("boom".to_string()), &Query::Status),
            default_icon(),
        );
        assert_eq!(entry.id, ID_ERROR);
        assert!(entry.data.is_none());
        assert!(entry.actions.is_empty());
    }

    // ----- resolve_icon ---------------------------------------

    #[test]
    fn resolve_icon_error_status_is_default_even_with_a_backend_icon() {
        let status: Result<SessionStatus, String> = Err("boom".to_string());
        assert!(
            matches!(resolve_icon(&status, None), EntryIcon::HeroIcon(ref name) if name == "bolt")
        );
        assert!(matches!(
            resolve_icon(&status, Some(app_icon())),
            EntryIcon::HeroIcon(ref name) if name == "bolt"
        ));
    }

    #[test]
    fn resolve_icon_ok_status_falls_back_to_default_without_a_backend_icon() {
        assert!(matches!(
            resolve_icon(&inactive(), None),
            EntryIcon::HeroIcon(ref name) if name == "bolt"
        ));
    }

    #[test]
    fn resolve_icon_ok_status_uses_the_backend_icon_when_present() {
        assert!(matches!(
            resolve_icon(&inactive(), Some(app_icon())),
            EntryIcon::AppIcon(ref id) if id == "com.if.Amphetamine"
        ));
    }
}
