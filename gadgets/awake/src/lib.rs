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

use torchsnap_gadget_sdk::prelude::*;

mod backend;
mod query;

struct AwakeGadget;
define_gadget!(AwakeGadget);

// =========================================================
// Lifecycle
// =========================================================

impl LifecycleGuest for AwakeGadget {
    fn enable() -> Result<(), String> {
        Ok(())
    }

    fn disable() {}

    fn on_setting_changed(_key: String, _value: String) {}
}

// =========================================================
// Search
// =========================================================

impl SearchGuest for AwakeGadget {
    fn entries() -> Vec<CatalogEntry> {
        vec![]
    }

    fn search(_query: String, _matched_prefix: Option<String>) -> SearchResponse {
        SearchResponse::Nothing
    }

    fn execute(_entry: ScoredEntry, _action_id: ActionId) -> Result<PostAction, String> {
        Err("gadget not yet functional".into())
    }
}

impl_noop_messaging!(AwakeGadget);
impl_noop_tasks!(AwakeGadget);
