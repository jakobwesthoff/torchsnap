// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Gadget System
//
// A single `Gadget` trait covers both search modes (ADR 0012):
//
// - Catalog gadgets override `entries()` to provide a finite
//   entry list that the host filters with nucleo.
// - Query gadgets override `search()` (and optionally
//   `search_prefixes()`) to receive the raw query and return
//   pre-scored results.
// - Hybrid gadgets override both — the host calls both paths
//   unconditionally.
//
// The trait uses an associated `type Caps` for per-gadget
// capability provisioning. `AnyGadget` provides the object-
// safe wrapper for dynamic dispatch in `GadgetHost`.
// =========================================================

pub mod app_launcher;
pub mod clipboard;
pub mod commands;
pub mod system_commands;
pub mod system_preferences;

use std::sync::Arc;

use tauri_plugin_store::Store;

use crate::commands::types::{ActionId, CatalogEntry, GadgetResponse, PostAction, ScoredEntry};
use crate::frecency::FrecencyStore;
use crate::icons::IconCache;
use crate::network::website_metadata::WebsiteMetadataService;
use crate::settings::SettingsInit;

// =========================================================
// GadgetShortcut — global shortcut declaration
// =========================================================

/// A global keyboard shortcut that a gadget wants to register.
pub struct GadgetShortcut {
    pub id: &'static str,
    pub label: &'static str,
    pub default_shortcut: &'static str,
    pub settings_key: &'static str,
}

// =========================================================
// OpenerCaps — closure-based opener capabilities
// =========================================================

/// Closure-based opener capabilities, built once from AppHandle
/// during host setup. Shared by native and WASM gadgets alike.
pub struct OpenerCaps {
    pub open_url: Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>,
    pub open_path: Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>,
    pub reveal_path: Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>,
}

// =========================================================
// ProvisioningContext — shared resources for gadget activation
// =========================================================

/// Bundled runtime context passed to `Gadget::provision()`.
///
/// Carries all host-managed shared resources a gadget might
/// need to build its capabilities. Constructed once in
/// `lib.rs::setup` after all shared resources exist, stored
/// on `GadgetHost` for re-enable cycles.
pub struct ProvisioningContext {
    pub app: tauri::AppHandle,
    pub store: Arc<Store<tauri::Wry>>,
    pub frecency: Arc<FrecencyStore>,
    pub icon_cache: Arc<IconCache>,
    pub metadata_service: Arc<WebsiteMetadataService>,
    pub opener: Arc<OpenerCaps>,
}

impl Clone for ProvisioningContext {
    fn clone(&self) -> Self {
        Self {
            app: self.app.clone(),
            store: Arc::clone(&self.store),
            frecency: Arc::clone(&self.frecency),
            icon_cache: Arc::clone(&self.icon_cache),
            metadata_service: Arc::clone(&self.metadata_service),
            opener: Arc::clone(&self.opener),
        }
    }
}

// =========================================================
// Gadget Trait
// =========================================================

/// Unified gadget trait for both catalog and query gadgets.
///
/// ## Capability provisioning
///
/// Each gadget declares an associated `type Caps` representing
/// the resources it needs during its enable lifetime.
/// `provision()` builds the caps from the shared
/// `ProvisioningContext`; `enable()` installs them and starts
/// the gadget. The host calls these through the object-safe
/// `AnyGadget` wrapper, which combines them into a single
/// `provision_and_enable()` call.
///
/// ## Lifecycle
///
/// 1. Gadget is constructed and registered via `GadgetHost::register`
/// 2. `initialize_settings()` is called synchronously at startup
/// 3. `provision()` + `enable()` are called on a background thread
///    if `enabled.<id>` is `true` in the settings store
/// 4. `entries()` / `search()` are called on every search keystroke
/// 5. `execute()` is called when the user triggers an action
/// 6. `setting_changed()` is called whenever a key in
///    `gadgets.<id>.*` changes at runtime
/// 7. `disable()` is called when the user toggles the gadget off
///    or during `RunEvent::Exit`
pub trait Gadget: Send + Sync {
    /// Per-gadget capability type, built by `provision()` and
    /// consumed by `enable()`. Gadgets without capabilities
    /// use `()`.
    type Caps: Send + Sync;

    fn id(&self) -> &str;

    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit {
        settings
    }

    /// Build the capability bundle from shared host resources.
    /// Called before `enable()` on every activation cycle.
    fn provision(&self, ctx: &ProvisioningContext) -> anyhow::Result<Self::Caps>;

    /// Activate the gadget with provisioned capabilities.
    /// Runs on a `spawn_blocking` thread.
    fn enable(&self, _caps: Self::Caps) {}

    fn disable(&self) {}

    fn setting_changed(&self, _key: &str, _value: serde_json::Value) {}

    fn execute(
        &self,
        entry: &ScoredEntry,
        action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction>;

    fn shortcuts(&self) -> Vec<GadgetShortcut> {
        vec![]
    }

    fn handle_shortcut(
        &self,
        _shortcut_id: &str,
    ) -> anyhow::Result<PostAction> {
        Ok(PostAction::Nothing)
    }

    fn handle_message(
        &self,
        _method: &str,
        _payload: serde_json::Value,
        _channel: tauri::ipc::Channel<serde_json::Value>,
    ) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("gadget does not handle custom messages")
    }

    fn search_prefixes(&self) -> &[String] {
        &[]
    }

    fn entries(&self) -> Vec<CatalogEntry> {
        vec![]
    }

    fn search(&self, _query: &str, _matched_prefix: Option<&str>) -> Option<GadgetResponse> {
        None
    }
}

// =========================================================
// AnyGadget — object-safe dynamic dispatch wrapper
// =========================================================

/// Object-safe trait for `GadgetHost` to dispatch through.
///
/// The blanket impl on `Gadget` erases the associated `Caps`
/// type by combining `provision()` + `enable()` into a single
/// `provision_and_enable()` call. All other methods delegate
/// directly.
pub trait AnyGadget: Send + Sync {
    fn id(&self) -> &str;
    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit;
    fn provision_and_enable(&self, ctx: &ProvisioningContext) -> anyhow::Result<()>;
    fn disable(&self);
    fn setting_changed(&self, key: &str, value: serde_json::Value);
    fn execute(
        &self,
        entry: &ScoredEntry,
        action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction>;
    fn shortcuts(&self) -> Vec<GadgetShortcut>;
    fn handle_shortcut(
        &self,
        shortcut_id: &str,
    ) -> anyhow::Result<PostAction>;
    fn handle_message(
        &self,
        method: &str,
        payload: serde_json::Value,
        channel: tauri::ipc::Channel<serde_json::Value>,
    ) -> anyhow::Result<serde_json::Value>;
    fn search_prefixes(&self) -> &[String];
    fn entries(&self) -> Vec<CatalogEntry>;
    fn search(&self, query: &str, matched_prefix: Option<&str>) -> Option<GadgetResponse>;
}

impl<G: Gadget> AnyGadget for G {
    fn id(&self) -> &str {
        Gadget::id(self)
    }

    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit {
        Gadget::initialize_settings(self, settings)
    }

    fn provision_and_enable(&self, ctx: &ProvisioningContext) -> anyhow::Result<()> {
        let caps = Gadget::provision(self, ctx)?;
        Gadget::enable(self, caps);
        Ok(())
    }

    fn disable(&self) {
        Gadget::disable(self);
    }

    fn setting_changed(&self, key: &str, value: serde_json::Value) {
        Gadget::setting_changed(self, key, value);
    }

    fn execute(
        &self,
        entry: &ScoredEntry,
        action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        Gadget::execute(self, entry, action_id, app)
    }

    fn shortcuts(&self) -> Vec<GadgetShortcut> {
        Gadget::shortcuts(self)
    }

    fn handle_shortcut(
        &self,
        shortcut_id: &str,
    ) -> anyhow::Result<PostAction> {
        Gadget::handle_shortcut(self, shortcut_id)
    }

    fn handle_message(
        &self,
        method: &str,
        payload: serde_json::Value,
        channel: tauri::ipc::Channel<serde_json::Value>,
    ) -> anyhow::Result<serde_json::Value> {
        Gadget::handle_message(self, method, payload, channel)
    }

    fn search_prefixes(&self) -> &[String] {
        Gadget::search_prefixes(self)
    }

    fn entries(&self) -> Vec<CatalogEntry> {
        Gadget::entries(self)
    }

    fn search(&self, query: &str, matched_prefix: Option<&str>) -> Option<GadgetResponse> {
        Gadget::search(self, query, matched_prefix)
    }
}
