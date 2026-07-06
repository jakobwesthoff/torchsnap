// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Keep-awake backend abstraction.
//!
//! A keep-awake session is controlled through a pluggable
//! backend so the launcher surface stays decoupled from the
//! mechanism that actually prevents sleep. Different platforms
//! or different tools (a future native `caffeinate`, an
//! `IOKit` assertion, a Windows power request) can plug in
//! behind the same [`KeepAwakeBackend`] trait.
//!
//! Backend selection probes the platform and the tool's
//! availability once, at gadget enable, via [`select_backend`].
//! The only backend today drives Amphetamine.app on macOS
//! through AppleScript; see [`amphetamine`].

// The backend is wired into `search()` / `execute()` in the
// next change on this branch (Phase 5). Until then nothing in
// the crate constructs a backend or reads its data types, so
// the whole module reads as dead code. The allow is removed
// once the search wiring consumes these items.
#![allow(dead_code)]

use torchsnap_gadget_sdk::filesystem;
use torchsnap_gadget_sdk::platform::{self, Os};

mod amphetamine;

use amphetamine::AmphetamineBackend;

// =========================================================
// Session data model
// =========================================================

/// A request to begin a keep-awake session, as understood by
/// any backend. Mirrors the launcher-parsed
/// `query::StartRequest` but lives at the backend boundary so
/// the query grammar and the backends stay independent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionRequest {
    /// Session length in whole minutes, or `None` for an
    /// indefinite (infinite) session.
    pub(crate) minutes: Option<u32>,
    /// Whether the display may sleep while the system stays
    /// awake.
    pub(crate) display_sleep_allowed: bool,
}

/// What kind of session is currently running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionKind {
    /// Runs until explicitly stopped.
    Infinite,
    /// Ends after a fixed duration; `remaining_secs` counts
    /// down toward zero.
    Timed { remaining_secs: u32 },
    /// A session Amphetamine started and manages on its own
    /// (a Trigger-, application-, or date-based session)
    /// rather than one this gadget requested.
    External,
}

/// The observed state of the keep-awake mechanism.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionStatus {
    /// The backend is reachable but no session is active.
    Inactive,
    /// The backing application is not running, so there is
    /// nothing to report and nothing to stop.
    AppNotRunning,
    /// A session is active.
    Active {
        kind: SessionKind,
        display_sleep_allowed: bool,
    },
}

// =========================================================
// Backend trait
// =========================================================

/// A mechanism that can start, stop, and report a keep-awake
/// session. Implementations are stateless value types; all
/// state lives in the backing tool.
pub(crate) trait KeepAwakeBackend {
    /// Report the current session state without side effects.
    /// Must not launch a not-running backing application.
    fn status(&self) -> Result<SessionStatus, String>;

    /// Start a session per `req`, replacing any session already
    /// running.
    fn start(&self, req: &SessionRequest) -> Result<(), String>;

    /// End the current session. A no-op session end is not an
    /// error at this layer; backends surface only genuine
    /// failures.
    fn stop(&self) -> Result<(), String>;
}

// =========================================================
// Backend selection
// =========================================================

/// Where Amphetamine's bundle metadata lives when installed
/// to the standard applications folder. Its presence is the
/// availability probe for the Amphetamine backend.
const AMPHETAMINE_INFO_PLIST: &str = "/Applications/Amphetamine.app/Contents/Info.plist";

/// Pick the keep-awake backend for this host, or `None` when
/// no supported mechanism is available. Probed once at gadget
/// enable; the gadget stays silent in the launcher when this
/// returns `None`.
pub(crate) fn select_backend() -> Option<Box<dyn KeepAwakeBackend>> {
    // Amphetamine is macOS-only.
    if !matches!(platform::current_os(), Os::Macos) {
        return None;
    }

    // `file_exists` conflates every failure reason (missing,
    // permission-denied, invalid, I/O) into `false`, which is
    // exactly the signal we want: any of them means the backend
    // is unavailable.
    //
    // TODO: Amphetamine installed outside `/Applications` (for
    // example `~/Applications` or a custom location) is not
    // detected; only the standard install path is probed.
    if filesystem::file_exists(AMPHETAMINE_INFO_PLIST) {
        Some(Box::new(AmphetamineBackend))
    } else {
        None
    }
}
