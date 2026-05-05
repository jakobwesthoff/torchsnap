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
pub mod clipboard;
pub mod commands;
pub mod system_commands;
pub mod system_preferences;

use crate::commands::types::{ActionId, CatalogEntry, GadgetResponse, PostAction};
use crate::frecency::GadgetFrecency;
use crate::settings::{GadgetSettings, SettingsInit};

// =========================================================
// GadgetShortcut — global shortcut declaration
// =========================================================

/// A global keyboard shortcut that a plugin wants to register.
///
/// Plugins declare shortcuts via `shortcuts()`. The host registers
/// them with the OS at startup and routes activations back through
/// `handle_shortcut()`. The actual key combo is persisted in the
/// plugin's settings namespace under `shortcut.<id>`, so users
/// can reconfigure it.
pub struct GadgetShortcut {
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
// GadgetContext — bundled runtime context for plugin activation
// =========================================================

/// Runtime context passed to plugins during `enable()`.
///
/// Bundles scoped settings access so plugins don't need an
/// ever-growing parameter list.
pub struct GadgetContext {
    pub settings: GadgetSettings,
    pub frecency: GadgetFrecency,
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
/// The host manages the plugin lifecycle through these phases:
///
/// 1. Plugin is constructed and registered via `PluginHost::register`
/// 2. `initialize_settings()` is called synchronously at startup
/// 3. `enable()` is called on a background thread if `enabled.<id>`
///    is `true` in the settings store (default)
/// 4. `entries()` / `search()` are called on every search keystroke
///    (only while enabled — the host gates on the enabled flag)
/// 5. `execute()` is called when the user triggers an action
/// 6. `setting_changed()` is called whenever a key in
///    `plugins.<id>.*` changes at runtime
/// 7. `disable()` is called when the user toggles the plugin off
///    or during `RunEvent::Exit`
///
/// Enable/disable may be called multiple times during the app's
/// lifetime as the user toggles the plugin on and off.
///
pub trait Gadget: Send + Sync {
    /// Unique identifier for this plugin. Used as the `source`
    /// field in `SourcedEntry` and for routing `execute_action`.
    fn id(&self) -> &str;

    /// Declare default settings for this plugin.
    ///
    /// Called synchronously at startup *before* `enable()`. The
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
    ///
    /// Note: The `enabled` key is managed by the host at
    /// `enabled.<plugin-id>` — plugins should not declare it here.
    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit {
        settings
    }

    /// Activate the plugin. Called on startup (if enabled) and on
    /// each re-enable after a user toggle.
    ///
    /// Implementations should acquire resources, start background
    /// threads, and prepare for search queries. This may be called
    /// multiple times during the app's lifetime — each call should
    /// be idempotent if resources are already initialized.
    ///
    /// Runs on a `spawn_blocking` thread — implementations are free
    /// to block.
    ///
    // FIXME: Find a cleaner way to provide context without passing
    // AppHandle and GadgetContext on every enable() call. These are
    // immutable after construction — ideally the plugin would hold
    // a reference from registration time.
    fn enable(&self, _app: &tauri::AppHandle, _ctx: &GadgetContext) {}

    /// Deactivate the plugin. Called when the user toggles the
    /// plugin off and during `RunEvent::Exit`.
    ///
    /// Implementations should release resources, stop background
    /// threads, and clean up state. The plugin may be re-enabled
    /// later — resources acquired in `enable()` should be released
    /// here.
    fn disable(&self) {}

    /// React to a settings change in this plugin's namespace.
    ///
    /// Called by the host whenever a key in `plugins.<id>.*` changes
    /// at runtime. The `key` is relative to the plugin namespace
    /// (e.g., `"retentionDays"`, not `"plugins.calculator.retentionDays"`).
    ///
    /// This is dispatched through a `CoalescingDispatcher` — rapid
    /// changes to the same key are deduplicated to the latest value.
    /// Different keys preserve chronological order.
    ///
    /// The default implementation is a no-op.
    fn setting_changed(&self, _key: &str, _value: serde_json::Value) {}

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
    /// back to `GadgetShortcut::default_shortcut`) and registers
    /// them with the OS. When a shortcut fires, the host calls
    /// `handle_shortcut()` with the matching shortcut ID.
    ///
    /// The default implementation declares no shortcuts.
    fn shortcuts(&self) -> Vec<GadgetShortcut> {
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
    ///
    /// **WASM plugins:** The `WasmGadgetBridge` adapter that
    /// wraps a WIT guest export ignores the `channel`
    /// parameter — WASM plugins are strictly request/response
    /// (per ADR 0030). If a plugin needs streaming support,
    /// it must stay native or wait for a future
    /// `messaging-stream` WIT sub-interface.
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
    fn search_prefixes(&self) -> &[String] {
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
    /// when running as an always-on plugin. Note:
    /// [`GadgetResponse::CustomUI`] is only honoured in prefix mode;
    /// in always-on mode it is downgraded to plain results.
    ///
    /// Returns `None` when the plugin has no results for this query,
    /// or `Some(GadgetResponse)` with the results/UI payload.
    ///
    /// The default is a no-op returning `None` (catalog-only plugins).
    fn search(&self, _query: &str, _matched_prefix: Option<&str>) -> Option<GadgetResponse> {
        None
    }
}
