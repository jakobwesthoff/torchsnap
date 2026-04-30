// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// ZeroTier Plugin — query-driven launcher integration
//
// Surfaces every network the local `zerotier-one` daemon
// reports (joined / connected) plus everything previously
// observed and merged from the macOS UI's
// `saved_networks.json`. Users connect, disconnect, and
// forget networks via per-entry actions; an unrecognized
// 16-hex-char query produces a synthetic "Connect to
// network <id>" entry.
//
// Internal modules carry the real logic (API client, auth
// resolver, history store, cache, query intent, action
// dispatch). This file wires them into the plugin
// lifecycle exports.
// =========================================================

use torchsnap_plugin_sdk::prelude::*;

mod api;
mod auth;
mod cache;
mod history;
mod query;

struct ZeroTierPlugin;
define_plugin!(ZeroTierPlugin);

impl LifecycleGuest for ZeroTierPlugin {
    fn enable() {
        logging::log(
            logging::LogLevel::Info,
            "ZeroTier plugin enabled",
            &[],
            None,
        );
    }

    fn disable() {
        logging::log(
            logging::LogLevel::Info,
            "ZeroTier plugin disabled",
            &[],
            None,
        );
    }

    fn on_setting_changed(key: String, _value: String) {
        logging::log(
            logging::LogLevel::Info,
            &format!("Setting `{key}` changed"),
            &[("key".into(), key)],
            None,
        );
    }
}

impl SearchGuest for ZeroTierPlugin {
    fn entries() -> Vec<CatalogEntry> {
        vec![]
    }

    fn search(_query: String, _matched_prefix: Option<String>) -> SearchResponse {
        SearchResponse::Nothing
    }

    fn execute(_entry_id: String, _action_id: ActionId) -> Result<PostAction, String> {
        Ok(PostAction::Dismiss)
    }
}

// Replaced in P2 with the real frontend RPC dispatcher
// (`refresh`, `reimport`, `forget`, `clear_all`,
// `validate_token`, `auth_state`).
impl_noop_messaging!(ZeroTierPlugin);

// State refresh is rate-limit-driven (1s cache invalidated on
// mutating actions), not timer-driven, so this plugin declares
// no `[[tasks]]` entries.
impl_noop_tasks!(ZeroTierPlugin);
