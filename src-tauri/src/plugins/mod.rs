// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Plugin System
//
// A single `Plugin` trait covers both search modes (ADR 0012):
//
// - Catalog plugins override `entries()` to provide a finite
//   entry list that the host filters with nucleo.
// - Query plugins override `search()` (and optionally
//   `search_prefixes()`) to receive the raw query and return
//   pre-scored results.
// - Hybrid plugins override both — the host calls both paths
//   unconditionally.
//
// The trait is structured so it can later become the boundary
// for a WASM plugin interface.
// =========================================================

pub mod app_launcher;
pub mod calculator;
pub mod clipboard;
pub mod commands;
pub mod emoji;
pub mod system_commands;
pub mod system_preferences;

use crate::frecency::PluginFrecency;
use crate::search::types::{
    ActionId, CancellationToken, CatalogEntry, PostAction, ResultChannel,
};
use crate::settings::{PluginSettings, SettingsInit};
use crate::settings_notifier::PluginSettingsNotifier;

// =========================================================
// PluginShortcut — global shortcut declaration
// =========================================================

/// A global keyboard shortcut that a plugin wants to register.
///
/// Plugins declare shortcuts via `shortcuts()`. The host registers
/// them with the OS at startup and routes activations back through
/// `handle_shortcut()`. The actual key combo is persisted in the
/// plugin's settings namespace under `shortcut.<id>`, so users
/// can reconfigure it.
pub struct PluginShortcut {
    /// Stable identifier for this shortcut (e.g., "open-clipboard").
    /// Used for routing activations back to the plugin.
    pub id: &'static str,
    /// Human-readable label shown in the settings UI.
    pub label: &'static str,
    /// Default key combo in Tauri shortcut syntax
    /// (e.g., "CmdOrCtrl+Shift+V").
    pub default_shortcut: &'static str,
    /// Plugin-scoped settings key where the current combo is stored
    /// (e.g., "shortcut.open-clipboard"). The manager reads
    /// `plugins.<plugin_id>.<settings_key>` from the store.
    pub settings_key: &'static str,
}

// =========================================================
// PluginContext — bundled runtime context for plugin setup
// =========================================================

/// Runtime context passed to plugins during `setup()`.
///
/// Bundles scoped settings access and change notification so
/// plugins don't need an ever-growing parameter list.
pub struct PluginContext {
    pub settings: PluginSettings,
    pub notifier: PluginSettingsNotifier,
    pub frecency: PluginFrecency,
}

// =========================================================
// Plugin Trait
// =========================================================

/// Unified plugin trait for both catalog and query plugins.
///
/// Catalog-only plugins override `entries()` to provide a finite
/// list of entries that the host filters with nucleo. Query-only
/// plugins override `search()` (and optionally `search_prefixes()`)
/// to receive the raw query and return pre-scored results. Hybrid
/// plugins override both.
///
/// ## Prefix routing (ADR 0012)
///
/// Plugins may register one or more prefixes via `search_prefixes()`.
/// When the user's query starts with a registered prefix:
///
/// - Only the owning plugin is called (exclusive routing).
/// - Other plugins are skipped entirely.
/// - The prefix is stripped before passing the query.
/// - `matched_prefix` tells the plugin which prefix triggered.
///
/// ## Lifecycle
///
/// 1. Plugin is constructed and registered via `PluginHost::register`
/// 2. `initialize_settings()` is called synchronously at startup
/// 3. `setup()` is called once on a background thread
/// 4. `entries()` / `search()` are called on every search keystroke
/// 5. `execute()` is called when the user triggers an action
/// 6. `teardown()` is called once during `RunEvent::Exit`
pub trait Plugin: Send + Sync {
    /// Unique identifier for this plugin. Used as the `source`
    /// field in `ScoredEntry` and for routing `execute_action`.
    fn id(&self) -> &str;

    /// Whether the plugin is currently active.
    ///
    /// Plugins that support an enable/disable toggle override this
    /// to reflect their current state. The host checks this before
    /// including entries in search results, registering shortcuts,
    /// and other gating decisions. The default is always enabled.
    fn is_enabled(&self) -> bool {
        true
    }

    /// Plugin-scoped settings key that controls whether this plugin
    /// is enabled. Return `None` if the plugin has no user-facing
    /// toggle and is always active.
    ///
    /// The host watches `plugins.<id>.<key>` reactively so it can
    /// re-register shortcuts and update gating when the value
    /// changes at runtime.
    fn enabled_settings_key(&self) -> Option<&'static str> {
        None
    }

    /// Declare default settings for this plugin.
    ///
    /// Called synchronously at startup *before* `setup()`. The
    /// `current` parameter contains any previously persisted values
    /// for this plugin. Use `ensure()` to fill in missing defaults:
    ///
    /// ```ignore
    /// fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit {
    ///     settings
    ///         .ensure("pollingInterval", 500)
    ///         .ensure("retentionDays", 30)
    /// }
    /// ```
    ///
    /// The default implementation is a pass-through (no settings).
    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit {
        settings
    }

    /// One-time initialization after registration.
    ///
    /// Called once during app startup on a dedicated background
    /// thread — implementations are free to block (e.g., scan the
    /// filesystem, run subprocesses). The host spawns one thread
    /// per plugin so all setups run in parallel.
    ///
    /// The `AppHandle` gives plugins access to Tauri APIs (path
    /// resolution, managed state, etc.) during initialization.
    /// `settings` provides scoped read access to this plugin's
    /// settings namespace (values guaranteed present after
    /// `initialize_settings` ran).
    ///
    /// `entries()` must handle the case where `setup()` has not
    /// yet completed (e.g., return an empty list).
    ///
    /// The default implementation is a no-op.
    fn setup(&self, _app: &tauri::AppHandle, _ctx: &PluginContext) {}

    /// Cleanup before the application exits.
    ///
    /// Called once during `RunEvent::Exit`. Plugins should release
    /// resources, flush pending writes, and stop background threads.
    fn teardown(&self) {}

    /// Execute an action on an entry owned by this plugin.
    fn execute(
        &self,
        entry_id: &str,
        action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction>;

    /// Declare global keyboard shortcuts this plugin wants to register.
    ///
    /// The host reads the actual key combos from settings (falling
    /// back to `PluginShortcut::default_shortcut`) and registers
    /// them with the OS. When a shortcut fires, the host calls
    /// `handle_shortcut()` with the matching shortcut ID.
    ///
    /// The default implementation declares no shortcuts.
    fn shortcuts(&self) -> Vec<PluginShortcut> {
        vec![]
    }

    /// Handle a global shortcut activation.
    ///
    /// Called when one of this plugin's registered shortcuts fires.
    /// Returns a `PostAction` that tells the host what to do (e.g.,
    /// `ShowCustomUI` to open the launcher with this plugin's view).
    ///
    /// The default implementation does nothing.
    fn handle_shortcut(
        &self,
        _shortcut_id: &str,
        _app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        Ok(PostAction::Nothing)
    }

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

    /// Prefixes that activate exclusive search routing for this plugin.
    ///
    /// Return an empty slice (the default) if this plugin does not
    /// use prefix routing. Prefixes can be multi-character (e.g.,
    /// `":"`, `"g "`, `"http://"`). Longest prefix wins when
    /// multiple match.
    fn search_prefixes(&self) -> &[&str] {
        &[]
    }

    /// Return catalog entries for host-side nucleo matching.
    ///
    /// Called on every search. For small static catalogs this is
    /// trivially cheap. Plugins with dynamic content (e.g. if
    /// settings change) can rebuild the list on each call.
    ///
    /// The default returns an empty list (query-only plugins).
    fn entries(&self) -> Vec<CatalogEntry> {
        vec![]
    }

    /// Plugin-driven search. Called on every query for plugins
    /// that handle their own matching logic.
    ///
    /// `matched_prefix` is `Some(prefix)` when a registered prefix
    /// triggered this call (query is already stripped), or `None`
    /// when running as an always-on plugin.
    ///
    /// Results are pushed into `results` via its typed send methods.
    /// The plugin may send zero or more responses. Dropping `results`
    /// (or returning) signals completion.
    ///
    /// `cancel` can be polled via `cancel.is_cancelled()` to detect
    /// early termination (e.g., the user typed a new query). Fast
    /// plugins can ignore it.
    ///
    /// The default is a no-op (catalog-only plugins).
    fn search(
        &self,
        _query: &str,
        _matched_prefix: Option<&str>,
        _results: &ResultChannel,
        _cancel: &CancellationToken,
    ) {
    }
}
