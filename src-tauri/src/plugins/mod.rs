// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Plugin System
//
// Two plugin traits serve different search modes (ADR 0012):
//
// - CatalogPlugin: provides a finite entry list. The host
//   filters it with nucleo. Never sees the query.
// - QueryPlugin: receives the raw query and returns pre-scored
//   results. Optionally registers prefixes for exclusive
//   routing (e.g., ":" for emoji).
//
// Both traits are structured so they can later become the
// boundary for a WASM plugin interface.
// =========================================================

pub mod app_launcher;
pub mod commands;
pub mod emoji;
pub mod system_preferences;

use crate::search::types::{ActionId, CatalogEntry, PostAction, SearchResponse};

// =========================================================
// CatalogPlugin
// =========================================================

/// A plugin that provides a static catalog of entries.
///
/// The host calls `entries()` to get the full list and filters
/// it using nucleo. When the user executes an action, the host
/// calls `execute()` to route back to the originating plugin.
///
/// ## Lifecycle
///
/// 1. Plugin is constructed and registered via `CatalogRegistry::register`
/// 2. `setup()` is called once after all plugins are registered
/// 3. `entries()` is called on every search keystroke
/// 4. `execute()` is called when the user triggers an action
pub trait CatalogPlugin: Send + Sync {
    /// Unique identifier for this plugin. Used as the `source`
    /// field in `ScoredEntry` and for routing `execute_action`.
    fn id(&self) -> &str;

    /// One-time initialization after registration.
    ///
    /// Called once during app startup on a dedicated background
    /// thread — implementations are free to block (e.g., scan the
    /// filesystem, run subprocesses). The host spawns one thread
    /// per plugin so all setups run in parallel.
    ///
    /// The `AppHandle` gives plugins access to Tauri APIs (path
    /// resolution, managed state, etc.) during initialization.
    ///
    /// `entries()` must handle the case where `setup()` has not
    /// yet completed (e.g., return an empty list).
    ///
    /// The default implementation is a no-op.
    fn setup(&self, _app: &tauri::AppHandle) {}

    /// Cleanup before the application exits.
    ///
    /// Called once during `RunEvent::Exit`. Plugins should release
    /// resources, flush pending writes, and stop background threads.
    fn teardown(&self) {}

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
    ) -> anyhow::Result<PostAction>;

    /// Handle a custom message from the plugin's frontend component.
    ///
    /// This is the plugin-side handler for the `sendMessage` prop
    /// in the plugin UI (ADR 0016). The `channel` can be used to
    /// stream live updates back to the frontend. The default
    /// returns an error — override only when the plugin needs
    /// custom frontend ↔ backend communication.
    fn handle_message(
        &self,
        _method: &str,
        _payload: serde_json::Value,
        _channel: tauri::ipc::Channel<serde_json::Value>,
    ) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("plugin does not handle custom messages")
    }
}

// =========================================================
// QueryPlugin
// =========================================================

/// A plugin that handles its own search logic.
///
/// Unlike `CatalogPlugin`, a query plugin receives the raw query
/// string and returns pre-scored results. This is useful for
/// plugins that need custom matching (e.g., two-pass shortcode +
/// keyword search for emoji) or that generate results dynamically.
///
/// ## Prefix routing (ADR 0012)
///
/// Plugins may register one or more prefixes via `prefixes()`.
/// When the user's query starts with a registered prefix:
///
/// - Only the owning plugin is called (exclusive routing).
/// - Catalog plugins and prefix-less query plugins are skipped.
/// - The prefix is stripped before passing the query.
/// - `matched_prefix` tells the plugin which prefix triggered.
///
/// Plugins with no prefixes run on every query alongside catalog
/// plugins.
///
/// ## Lifecycle
///
/// Same as `CatalogPlugin`: construct → register → `setup()` →
/// `search()` on every keystroke → `execute()` on action.
pub trait QueryPlugin: Send + Sync {
    /// Unique identifier for this plugin.
    fn id(&self) -> &str;

    /// Prefixes that activate exclusive routing for this plugin.
    ///
    /// Return an empty slice to receive every query (always-on).
    /// Prefixes can be multi-character (e.g., `":"`, `"g "`,
    /// `"http://"`). Longest prefix wins when multiple match.
    fn prefixes(&self) -> &[&str] {
        &[]
    }

    /// One-time initialization. See `CatalogPlugin::setup()`.
    fn setup(&self, _app: &tauri::AppHandle) {}

    /// Cleanup before application exit. See `CatalogPlugin::teardown()`.
    fn teardown(&self) {}

    /// Search for results matching the given query.
    ///
    /// `matched_prefix` is `Some(prefix)` when a registered prefix
    /// triggered this call (query is already stripped), or `None`
    /// when running as an always-on plugin.
    ///
    /// Returns `SearchResponse::Results` for standard list rendering,
    /// or `SearchResponse::CustomUI` to request the plugin's frontend
    /// component take over the result area (ADR 0013).
    fn search(&self, query: &str, matched_prefix: Option<&str>) -> SearchResponse;

    /// Execute an action on an entry owned by this plugin.
    fn execute(
        &self,
        entry_id: &str,
        action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction>;

    /// Handle a custom message from the plugin's frontend component.
    ///
    /// This is the plugin-side handler for the `sendMessage` prop
    /// in the plugin UI (ADR 0016). The `channel` can be used to
    /// stream live updates back to the frontend. The default
    /// returns an error — override only when the plugin needs
    /// custom frontend ↔ backend communication.
    fn handle_message(
        &self,
        _method: &str,
        _payload: serde_json::Value,
        _channel: tauri::ipc::Channel<serde_json::Value>,
    ) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("plugin does not handle custom messages")
    }
}
