// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WASM Plugin Bridge
//
// Adapts a `WasmPluginInstance` to the native `Plugin` trait
// so that WASM plugins can participate in the existing
// `PluginHost` search and execution pipeline alongside
// native plugins.
//
// The bridge holds the manifest (for metadata like ID and
// prefixes) and the WASM instance (for guest calls). It
// translates between the native Plugin trait interface and
// the typed WASM guest exports.
// =========================================================

use crate::plugins::Plugin;
use crate::search::types::{
    ActionId, CancellationToken, CatalogEntry, PostAction, PluginResponse, ResultChannel,
};

use super::manifest::Manifest;
use super::runtime::WasmPluginInstance;

// =========================================================
// WasmPluginBridge
// =========================================================

/// Bridges a WASM plugin instance to the native `Plugin`
/// trait, allowing it to be registered with `PluginHost`.
///
/// Metadata (ID, prefixes) comes from the manifest.
/// Guest calls (entries, execute) go through the
/// `WasmPluginInstance` which handles store locking and
/// type conversion internally.
pub struct WasmPluginBridge {
    manifest: Manifest,
    instance: WasmPluginInstance,

    // The Plugin trait's `search_prefixes()` returns `&[&str]`,
    // which requires a stable backing store. We keep the owned
    // strings and a parallel vec of pointers that borrow from
    // them. This is safe because both vecs are immutable after
    // construction and live together in the same struct.
    _prefix_storage: Vec<String>,
    prefix_ptrs: Vec<&'static str>,
}

impl WasmPluginBridge {
    /// Create a bridge from a parsed manifest and a live
    /// WASM plugin instance.
    pub fn new(manifest: Manifest, instance: WasmPluginInstance) -> Self {
        let prefix_storage = manifest.plugin.prefixes.clone();

        // Deliberately leak the prefix strings. Plugins live
        // for the entire application lifetime, so this memory
        // is never reclaimed but also never dangling.
        let prefix_ptrs: Vec<&'static str> = prefix_storage
            .iter()
            .map(|s| &*Box::leak(s.clone().into_boxed_str()))
            .collect();

        Self {
            manifest,
            instance,
            _prefix_storage: prefix_storage,
            prefix_ptrs,
        }
    }
}

impl Plugin for WasmPluginBridge {
    fn id(&self) -> &str {
        self.manifest.plugin.id.as_str()
    }

    fn setup(&self, _app: &tauri::AppHandle, _ctx: &crate::plugins::PluginContext) {
        // WASM plugins don't use AppHandle or PluginContext —
        // they get capabilities through WIT host imports.
        if let Err(e) = self.instance.enable() {
            eprintln!(
                "[wasm:{}] enable() failed: {e:#}",
                self.manifest.plugin.id
            );
        }
    }

    fn teardown(&self) {
        if let Err(e) = self.instance.disable() {
            eprintln!(
                "[wasm:{}] disable() failed: {e:#}",
                self.manifest.plugin.id
            );
        }
    }

    fn entries(&self) -> Vec<CatalogEntry> {
        match self.instance.entries() {
            Ok(entries) => entries,
            Err(e) => {
                eprintln!(
                    "[wasm:{}] entries() failed: {e:#}",
                    self.manifest.plugin.id
                );
                vec![]
            }
        }
    }

    fn execute(
        &self,
        entry_id: &str,
        action_id: &ActionId,
        _app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        self.instance.execute(entry_id, action_id)
    }

    fn search(
        &self,
        query: &str,
        matched_prefix: Option<&str>,
        results: &ResultChannel,
        _cancel: &CancellationToken,
    ) {
        match self.instance.search(query, matched_prefix) {
            Ok(response) => match response {
                PluginResponse::Results(entries) if !entries.is_empty() => {
                    results.send_results(entries);
                }
                _ => {}
            },
            Err(e) => {
                eprintln!(
                    "[wasm:{}] search() failed: {e:#}",
                    self.manifest.plugin.id
                );
            }
        }
    }

    fn search_prefixes(&self) -> &[&str] {
        &self.prefix_ptrs
    }
}
