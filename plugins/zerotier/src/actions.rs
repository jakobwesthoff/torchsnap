// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Mutating-operation orchestration.
//!
//! Each user action on a network entry resolves to one of
//! three primitives:
//!
//! * [`connect`] — `POST /network/{id}` followed by an
//!   upsert into the history table.
//! * [`disconnect`] — `DELETE /network/{id}`. The history
//!   row is left in place so the network remains visible as
//!   "Stored" for re-Connect.
//! * [`forget`] — drop the history row. If the network is
//!   currently joined, disconnect first so the daemon and
//!   the plugin's state stay in sync.
//!
//! All three return the cache-invalidation signal to the
//! caller; the lib-level orchestration calls
//! [`crate::cache::RateLimitCache::invalidate`] to make the
//! next `search()` reflect post-action state without waiting
//! for TTL.

use torchsnap_plugin_sdk::sql::SqlHandle;

use crate::api::{ApiError, Client, Network, NetworkStatus};
use crate::history;

/// Connect to a network. The daemon's `POST /network/{id}`
/// is idempotent — same call works for first-time-join and
/// for re-Connect to a previously-known network. The history
/// row is upserted so the network surfaces in subsequent
/// renders even before the next `list_networks` refresh.
pub fn connect(
    client: &Client,
    db: &SqlHandle,
    id: &str,
    name_hint: &str,
    now_ms: i64,
) -> Result<(), String> {
    client.join_network(id).map_err(|e| e.to_string())?;
    // Seed a placeholder row so the launcher can show the
    // network as "joined, requesting configuration" before
    // the next live refresh confirms its `OK` status.
    let placeholder = Network {
        id: id.to_string(),
        name: name_hint.to_string(),
        status: NetworkStatus::RequestingConfiguration,
        network_type: None,
        mac: String::new(),
        port_device_name: String::new(),
        mtu: 0,
        allow_managed: false,
        allow_global: false,
        allow_default: false,
        allow_dns: false,
        assigned_addresses: vec![],
        routes: vec![],
        dns: None,
    };
    history::upsert_observed(db, &placeholder, now_ms)?;
    Ok(())
}

/// Disconnect from a network. The history row stays — the
/// network downgrades to "Stored, not currently joined" on
/// the next render.
pub fn disconnect(client: &Client, id: &str) -> Result<(), String> {
    client.leave_network(id).map_err(|e| e.to_string())?;
    Ok(())
}

/// Forget a network. If currently joined, disconnect first.
/// `currently_joined` is the caller's view (typically the
/// last cached `list_networks` response) — pass `false` to
/// skip the `DELETE` call when the row is already
/// known-only.
pub fn forget(
    client: &Client,
    db: &SqlHandle,
    id: &str,
    currently_joined: bool,
) -> Result<(), String> {
    if currently_joined {
        // Best-effort: a daemon that's already lost the
        // network still surfaces here as "joined" if our
        // cache predates the change. Swallow `NotFound`
        // status errors so Forget always proceeds to drop
        // the history row.
        match client.leave_network(id) {
            Ok(()) => {}
            Err(ApiError::HttpStatus { status: 404, .. }) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    history::forget(db, id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    // The action helpers wrap host-import calls (`http::fetch`
    // via `Client`, `sql::*` via `SqlHandle`) — neither is
    // available in the native-target test runner, so the
    // exercising of `connect` / `disconnect` / `forget` lives
    // in the WASM-runtime integration tests under
    // `src-tauri/tests/`.
    //
    // The unit-testable invariants this module exposes
    // (entry-id parsing, intent classification) live in
    // `query.rs`, where they are covered exhaustively.
}
