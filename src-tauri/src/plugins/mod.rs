// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Plugin System
//
// Plugins provide entries to the launcher's search system.
// Currently only catalog plugins are supported — they provide
// a finite list of entries that the host filters via nucleo.
// Query plugins (that handle their own filtering) will follow.
//
// The trait is structured so it can later become the boundary
// for a WASM plugin interface.
// =========================================================

pub mod commands;

use crate::search::types::{ActionId, CatalogEntry};

/// A plugin that provides a static catalog of entries.
///
/// The host calls `entries()` to get the full list and filters
/// it using nucleo. When the user executes an action, the host
/// calls `execute()` to route back to the originating plugin.
pub trait CatalogPlugin: Send + Sync {
    /// Unique identifier for this plugin. Used as the `source`
    /// field in `ScoredEntry` and for routing `execute_action`.
    fn id(&self) -> &str;

    /// Return all catalog entries this plugin provides.
    ///
    /// Called on every search. For small static catalogs this is
    /// trivially cheap. Plugins with dynamic content (e.g. if
    /// settings change) can rebuild the list on each call.
    fn entries(&self) -> Vec<CatalogEntry>;

    /// Execute an action on an entry owned by this plugin.
    fn execute(
        &self,
        entry_id: &str,
        action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<()>;
}
