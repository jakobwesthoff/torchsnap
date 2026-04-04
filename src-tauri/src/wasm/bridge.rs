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

use std::time::SystemTime;

use crate::plugins::Plugin;
use crate::search::types::{ActionId, CatalogEntry, PostAction, PluginResponse};

use super::logging::channel::LogSender;
use super::logging::{LogItem, LogItemKind, LogLevel, LogSource};
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
    log_sender: LogSender,
}

impl WasmPluginBridge {
    /// Create a bridge from a parsed manifest and a live
    /// WASM plugin instance.
    pub fn new(manifest: Manifest, instance: WasmPluginInstance, log_sender: LogSender) -> Self {
        Self {
            manifest,
            instance,
            log_sender,
        }
    }

    /// Emit a log entry for bridge-level events (errors from
    /// guest calls that are caught and handled here).
    fn log(&self, level: LogLevel, message: String) {
        self.log_sender.send(LogItem {
            seq: 0,
            timestamp: SystemTime::now(),
            source: LogSource::Plugin(self.manifest.plugin.id.to_string()),
            kind: LogItemKind::Message {
                level,
                message,
                metadata: vec![],
                span_id: None,
            },
        });
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
            self.log(LogLevel::Error, format!("enable() failed: {e:#}"));
        }
    }

    fn teardown(&self) {
        if let Err(e) = self.instance.disable() {
            self.log(LogLevel::Error, format!("disable() failed: {e:#}"));
        }
    }

    fn entries(&self) -> Vec<CatalogEntry> {
        match self.instance.entries() {
            Ok(entries) => entries,
            Err(e) => {
                self.log(LogLevel::Error, format!("entries() failed: {e:#}"));
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
    ) -> Option<PluginResponse> {
        match self.instance.search(query, matched_prefix) {
            Ok(response) => match response {
                PluginResponse::Results(ref entries) if entries.is_empty() => None,
                _ => Some(response),
            },
            Err(e) => {
                self.log(LogLevel::Error, format!("search() failed: {e:#}"));
                None
            }
        }
    }

    fn search_prefixes(&self) -> &[String] {
        &self.manifest.plugin.prefixes
    }
}
