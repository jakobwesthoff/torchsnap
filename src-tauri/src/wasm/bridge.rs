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

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::Context;

use crate::plugins::Plugin;
use crate::search::types::{ActionId, CatalogEntry, PluginResponse, PostAction};
use crate::settings::SettingsInit;

use super::logging::channel::LogSender;
use super::logging::{LogItem, LogItemKind, LogLevel, LogSource};
use super::manifest::Manifest;
use super::runtime::{SqlConfig, WasmPluginInstance};
use super::source::PluginSource;

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
    /// Create a bridge from a parsed manifest, a live WASM
    /// plugin instance, and the host-side bits the SQL host
    /// import needs at construction time:
    ///
    /// - `source` is the `PluginSource` (directory or
    ///   archive) the plugin was loaded from. Used here to
    ///   read migration files declared in
    ///   `manifest.storage.sql.migrations` *once*, eagerly,
    ///   so the `sql::open()` host import can resolve
    ///   without re-touching the source on every call.
    /// - `app_data_dir` is the host's per-app data root
    ///   (`tauri::AppHandle::path().app_data_dir()`); the
    ///   plugin's database file lives at
    ///   `<app_data_dir>/plugins/<plugin-id>/storage.db`.
    ///
    /// Migration-file reads are surfaced as `Err` here
    /// rather than deferred to the first `sql::open()`
    /// call, because a missing migration file is a manifest
    /// authoring bug — better to refuse to load the plugin
    /// than to half-load it and wait for the symptom to
    /// surface in a search hot path.
    pub fn new(
        manifest: Manifest,
        instance: WasmPluginInstance,
        log_sender: LogSender,
        source: &dyn PluginSource,
        app_data_dir: &std::path::Path,
    ) -> anyhow::Result<Self> {
        // Materialize the SQL configuration once at load
        // time. Plugins without `[storage.sql]` get
        // `SqlConfig::None`, which the host import
        // translates into a clear "no storage configured"
        // error if they call `sql::open()` anyway.
        let sql_config = match manifest.storage.as_ref().and_then(|s| s.sql.as_ref()) {
            None => SqlConfig::None,
            Some(sql) => {
                let mut migrations: Vec<String> = Vec::with_capacity(sql.migrations.len());
                for path in &sql.migrations {
                    let bytes = source
                        .read_file(path)
                        .with_context(|| format!("read SQL migration file `{path}`"))?;
                    let text = String::from_utf8(bytes).with_context(|| {
                        format!("SQL migration file `{path}` is not valid UTF-8")
                    })?;
                    migrations.push(text);
                }

                let db_path: PathBuf = app_data_dir
                    .join("plugins")
                    .join(manifest.plugin.id.as_str())
                    .join("storage.db");

                SqlConfig::Configured {
                    db_path,
                    migrations: Arc::new(migrations),
                }
            }
        };

        // Stash the configuration on the wasmtime store
        // immediately so that any future host-import call
        // sees a fully-initialized `PluginState`. The
        // PluginState's own `sql_storage` cache stays
        // `None` — the actual database file is materialized
        // lazily on the first `sql::open()` call.
        instance.set_sql_config(sql_config);

        Ok(Self {
            manifest,
            instance,
            log_sender,
        })
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

    /// Write the `[settings]` table defaults from the manifest to
    /// the settings store. Each entry is an `ensure()` call —
    /// existing user values are never overwritten.
    fn initialize_settings(&self, mut settings: SettingsInit) -> SettingsInit {
        for (key, toml_value) in &self.manifest.settings {
            if let Ok(json_value) = serde_json::to_value(toml_value) {
                settings = settings.ensure(key, json_value);
            }
        }
        settings
    }

    fn enable(&self, _app: &tauri::AppHandle, ctx: &crate::plugins::PluginContext) {
        // Stash the per-plugin `PluginSettings` handle on the
        // wasmtime store data BEFORE invoking the guest's
        // `enable()`, so the guest can call `settings::get`
        // during its own initialization. WASM plugins still
        // don't use `AppHandle` directly — every host
        // capability flows through WIT imports — but they
        // need the settings handle wired up.
        self.instance.set_settings(ctx.settings.clone());

        if let Err(e) = self.instance.enable() {
            self.log(LogLevel::Error, format!("enable() failed: {e:#}"));
        }
    }

    fn disable(&self) {
        if let Err(e) = self.instance.disable() {
            self.log(LogLevel::Error, format!("disable() failed: {e:#}"));
        }
        // Drop the stashed PluginSettings so subsequent
        // settings::get calls (none should happen, but be
        // defensive) revert to the "unset" no-op behavior.
        self.instance.clear_settings();
        // Drop the cached `Arc<SqlStorage>` so the database
        // file isn't held open between enable cycles. Any
        // outstanding handle inside the wasmtime
        // ResourceTable will be cleared on next instantiate
        // along with the rest of the table.
        self.instance.clear_sql_storage();
    }

    fn setting_changed(&self, key: &str, value: serde_json::Value) {
        // The host's CoalescingDispatcher (ADR 0026) has
        // already deduplicated rapid same-key writes by the
        // time we get here, so the bridge does not need its
        // own throttling. Re-encode the JSON value as a
        // string for the WIT crossing — `settings::get` and
        // `on-setting-changed` use the same encoding.
        let json = match serde_json::to_string(&value) {
            Ok(s) => s,
            Err(e) => {
                self.log(
                    LogLevel::Error,
                    format!("serialize setting value for `{key}`: {e}"),
                );
                return;
            }
        };
        if let Err(e) = self.instance.on_setting_changed(key, &json) {
            self.log(
                LogLevel::Error,
                format!("on_setting_changed({key}) failed: {e:#}"),
            );
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

    fn search(&self, query: &str, matched_prefix: Option<&str>) -> Option<PluginResponse> {
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

    /// Forward custom frontend messages to the WASM guest's
    /// `messaging::handle-message` export.
    ///
    /// The streaming `_channel` parameter is intentionally
    /// ignored — WASM plugins are strictly request/response.
    /// Plugins that need streaming should stay native, or
    /// wait for a future `messaging-stream` sub-interface.
    ///
    /// Errors are wrapped with explicit prefixes so log
    /// readers can distinguish bridge-level failures
    /// (linker, serialization, store lock) from
    /// plugin-reported failures (the inner `err(string)`
    /// arm of the WIT `result`).
    fn handle_message(
        &self,
        method: &str,
        payload: serde_json::Value,
        _channel: tauri::ipc::Channel<serde_json::Value>,
    ) -> anyhow::Result<serde_json::Value> {
        // Re-encode the payload as a JSON string for the WIT
        // crossing — same convention as `settings::get`.
        let payload_json =
            serde_json::to_string(&payload).context("serialize handle_message payload")?;

        // Two layers of error: the outer `Result` is the
        // wasmtime / store-lock layer; the inner
        // `Result<String, String>` is what the plugin
        // returned. Plugin-reported errors get a
        // `"plugin error: "` prefix to disambiguate them
        // from bridge failures in logs.
        let result_json = self
            .instance
            .handle_message(method, &payload_json)
            .context("invoke guest handle-message")?
            .map_err(|e| anyhow::anyhow!("plugin error: {e}"))?;

        // Parse the plugin's JSON response back into a
        // `serde_json::Value` for the Tauri command return.
        // A malformed response is a plugin bug — surface it
        // with a clear context so log readers can locate it.
        serde_json::from_str(&result_json).context("parse guest handle-message response")
    }
}
