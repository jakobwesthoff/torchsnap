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
// Gadgets receive `Arc<ProvisionedCaps>` at construction
// via the factory-based registration API. The trait is
// object-safe — `GadgetHost` holds `Arc<dyn Gadget>`.
// =========================================================

pub mod app_launcher;
pub mod clipboard;
pub mod commands;
pub mod system_commands;
pub mod system_preferences;

use crate::commands::types::{ActionId, CatalogEntry, GadgetResponse, PostAction, ScoredEntry};
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
// Gadget Trait
// =========================================================

/// Unified gadget trait for both catalog and query gadgets.
///
/// ## Capability provisioning
///
/// Gadgets receive `Arc<ProvisionedCaps>` at construction via
/// the factory-based registration API. Capabilities are plain
/// fields on the gadget struct — no `OnceLock`, no `Option`.
///
/// ## Lifecycle
///
/// 1. Host calls `cap_requests()` (inherent method) and builds caps
/// 2. Host calls the factory closure with caps to construct the gadget
/// 3. Host calls `register_with_caps()` to register the gadget
/// 4. `initialize_settings()` is called synchronously at startup
/// 5. `enable()` is called on a background thread if `enabled.<id>`
///    is `true` in the settings store
/// 6. `entries()` / `search()` are called on every search keystroke
/// 7. `execute()` is called when the user triggers an action
/// 8. `setting_changed()` is called whenever a key in
///    `gadgets.<id>.*` changes at runtime
/// 9. `disable()` is called when the user toggles the gadget off
///    or during `RunEvent::Exit`
pub trait Gadget: Send + Sync {
    fn id(&self) -> &str;

    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit {
        settings
    }

    /// Activate the gadget. Runs on a `spawn_blocking` thread.
    /// Caps are available as plain fields on `self`. Returning
    /// `Err` disables the gadget — no further calls dispatched.
    fn enable(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn disable(&self) {}

    fn setting_changed(&self, _key: &str, _value: serde_json::Value) {}

    fn execute(&self, entry: &ScoredEntry, action_id: &ActionId) -> anyhow::Result<PostAction>;

    fn shortcuts(&self) -> Vec<GadgetShortcut> {
        vec![]
    }

    fn handle_shortcut(&self, _shortcut_id: &str) -> anyhow::Result<PostAction> {
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
