// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WASM Plugin Runtime
//
// Manages the wasmtime engine (shared across all plugins)
// and provides per-plugin instances that wrap a Store and
// typed component bindings.
//
// Architecture:
//   WasmRuntime (one per app, owns Engine + Component cache)
//    ├── compile(plugin_id, wasm_bytes)
//    └── instantiate(plugin_id) → WasmPluginInstance
//
//   WasmPluginInstance (one per plugin, owns Store + Plugin)
//    ├── enable() / disable()
//    ├── entries() → Vec<CatalogEntry>
//    └── execute(entry_id, action_id) → PostAction
//
// The compile/instantiate split lets the expensive step run
// once per plugin while instantiation stays cheap enough to
// repeat on demand. See ADR 0033.
// =========================================================

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use anyhow::Context;

use wasmtime::component::{Component, HasSelf, Linker, Resource, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use super::bindings;
use super::logging::channel::LogSender;
// reqwest is a direct dependency used for HTTP method construction and
// timeout error detection in the http::Host implementation.
use super::logging::spans::{Logger, SpanRegistry};
use super::logging::{LogItem, LogItemKind, LogLevel, LogSource};
use crate::frecency::PluginFrecency;
use crate::settings::PluginSettings;
use crate::storage::{SqlStorage, SqlValue as HostSqlValue};
use reqwest;

// =========================================================
// Per-Plugin Store State
//
// This is the `T` in `Store<T>`. It holds the WASI context
// and any per-plugin state the host imports need access to
// (e.g., the plugin ID for log tagging, the log sender).
// =========================================================

pub struct PluginState {
    plugin_id: String,
    wasi: WasiCtx,
    wasi_table: ResourceTable,
    log_sender: LogSender,
    span_registry: Arc<SpanRegistry>,
    /// Per-plugin namespaced settings reader. `None` until the
    /// bridge stashes the `PluginContext.settings` handle on
    /// `enable()`. The settings host import (`settings::get`)
    /// errors gracefully if accessed before that happens —
    /// which it shouldn't, since the host always calls
    /// `enable()` before any guest code runs.
    settings: Option<PluginSettings>,
    /// Per-plugin SQL storage configuration. Populated by
    /// the bridge at construction time from the manifest's
    /// `[storage.sql]` block (`SqlConfig::None` when the
    /// plugin declares no SQL storage). The bridge's
    /// `enable()` materializes the actual database into
    /// `sql_storage` below; `sql::connection()` then hands
    /// out handles backed by the same `Arc<SqlStorage>`.
    sql_config: SqlConfig,
    sql_storage: Option<Arc<SqlStorage>>,
    /// Resource reps for every `SqlHandleEntry` currently
    /// live in `wasi_table`. Pushed on `sql::connection()`,
    /// removed on the WIT-driven `drop()` of an individual
    /// handle, and drained-and-deleted on
    /// `clear_sql_storage()` so that disable cleanly
    /// releases every outstanding `Arc<SqlStorage>`
    /// reference. Without this list, a plugin that opened
    /// a handle and never explicitly dropped it would leak
    /// the rusqlite `Connection` until the
    /// `WasmPluginInstance` itself is dropped (i.e. until
    /// app shutdown).
    sql_handle_reps: Vec<u32>,
    /// Closure that writes a string to the system clipboard.
    /// Stashed by the bridge from the `tauri::AppHandle` on
    /// `enable()` so the `clipboard::write-text` host import
    /// can resolve without `PluginState` itself depending on
    /// the Tauri AppHandle type. `None` between enable
    /// cycles; the host import returns an error if accessed
    /// outside an enable lifetime (which should never
    /// happen — every guest call runs inside one).
    clipboard_writer: Option<ClipboardWriter>,
    /// Per-plugin namespaced frecency reader. Same lifecycle
    /// as `settings`: stashed by the bridge on `enable()` from
    /// the `PluginContext.frecency` handle and cleared on
    /// `disable()`. The host automatically records selections
    /// before `execute()` and applies score bonuses after
    /// `search()` — this handle is only for plugins that
    /// need to read frecency state directly (e.g. to drive an
    /// empty-query browse mode).
    frecency: Option<PluginFrecency>,
    /// URL schemes this plugin is permitted to open. Populated
    /// by the bridge from `[permissions.opener].schemes` in the
    /// manifest. Empty means deny-all. Compared
    /// case-insensitively against the scheme extracted from the
    /// URL at call time by `opener::open-url`.
    opener_schemes: Vec<String>,
    /// Closure that opens a URL in the OS default handler.
    /// Same lifecycle and decoupling contract as
    /// `clipboard_writer`: the bridge constructs this from its
    /// `AppHandle` on `enable()` and clears it on `disable()`.
    opener_writer: Option<UrlOpenerFn>,
    /// Origins this plugin is permitted to fetch. Populated by
    /// the bridge from `[permissions.http].origins` in the
    /// manifest and already normalized to
    /// `ascii_serialization()` form. Empty means deny-all;
    /// `["*"]` means trust-all. Compared against the
    /// ASCII-serialized origin of the request URL at call time.
    http_origins: Vec<String>,
    /// HTTP client for this plugin, created on `enable()` and
    /// cleared on `disable()`. `None` outside an enable
    /// lifetime; the `http::fetch` host import returns an error
    /// in that case.
    http_client: Option<Arc<crate::network::Http>>,
    /// The plugin's own source handle, stashed by the bridge
    /// on `enable()` so the `assets::read` / `assets::exists`
    /// host imports can read files bundled inside the plugin
    /// archive (or development directory) without re-opening
    /// it. `None` outside an enable lifetime — the host
    /// imports return an `io-error` in that case, matching
    /// the contract of the other capability stashes.
    plugin_source: Option<Arc<dyn super::source::PluginSource + Send + Sync>>,
    /// Whether the plugin's manifest grants `open-path`. Drives
    /// the `opener::open-path` host import gate; the actual
    /// host-side delegation goes through `open_path_writer`.
    opener_open_path: bool,
    /// Whether the plugin's manifest grants `reveal-path`. Drives
    /// the `opener::reveal-path` host import gate.
    opener_reveal_path: bool,
    /// Closure that opens a filesystem path with the OS-registered
    /// application. Same lifecycle and decoupling contract as
    /// `opener_writer`: the bridge constructs this from its
    /// `AppHandle` on `enable()` and clears it on `disable()`.
    open_path_writer: Option<UrlOpenerFn>,
    /// Closure that reveals a filesystem path in the OS file
    /// manager. Same shape and lifecycle as `open_path_writer`.
    reveal_path_writer: Option<UrlOpenerFn>,
    /// Resolved `${...}` substitution variables for this plugin
    /// instance. Populated by the bridge on `enable()`; consumed
    /// by `paths::resolve` and (in a later phase) by
    /// `command::run` rule compilation. `None` outside an enable
    /// lifetime — `paths::resolve` returns `unterminated` in that
    /// case as a placeholder for "interface not initialized" since
    /// the variant has no dedicated "uninitialized" arm.
    path_context: Option<super::permission_vars::PathContext>,
}

/// Closure type for the clipboard write capability.
///
/// Boxed and stored on `PluginState` instead of holding a
/// `tauri::AppHandle` directly so the runtime layer stays
/// decoupled from Tauri-specific types. The bridge
/// constructs the closure from its own `AppHandle` and
/// stashes it on `enable()`.
pub type ClipboardWriter = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// Closure type for the opener `open-url` capability.
///
/// Follows the same decoupling pattern as `ClipboardWriter`: the bridge
/// builds the closure from `tauri::AppHandle` at `enable()` time and
/// stashes it here so this module never imports Tauri types directly.
pub type UrlOpenerFn = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// Whether and how the plugin's SQL storage is configured.
///
/// Materialized at bridge construction so the wasmtime host
/// import can resolve `sql::connection()` synchronously.
/// Migration files are read once via
/// `PluginSource::read_file` at load time.
#[derive(Clone)]
pub enum SqlConfig {
    /// Plugin did not declare a `[storage.sql]` block in its
    /// manifest. `sql::connection()` traps if called.
    None,
    /// Plugin opted into SQL storage. The migration strings
    /// are pre-loaded; the database file is created by the
    /// bridge during `enable()` before the guest runs.
    Configured {
        db_path: PathBuf,
        migrations: Arc<Vec<String>>,
    },
}

/// Internal entry stored in the wasmtime `ResourceTable`
/// behind every `Resource<SqlHandleEntry>` returned to a
/// plugin. Holding the `Arc<SqlStorage>` here lets the WIT
/// resource drop semantics (which run when the plugin lets
/// the handle go out of scope) cleanly release just this
/// reference; the underlying `SqlStorage` stays alive on
/// `PluginState::sql_storage` until the plugin is disabled.
pub struct SqlHandleEntry {
    storage: Arc<SqlStorage>,
}

// The WasiView trait is required by wasmtime-wasi to locate
// the WasiCtx and ResourceTable within our custom state.
// In wasmtime 43, ctx() returns a WasiCtxView that bundles
// both references together.
impl WasiView for PluginState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.wasi_table,
        }
    }
}

// =========================================================
// Host Import Implementations
//
// Each WIT `import` interface generates a trait that we
// implement on PluginState. wasmtime calls these when the
// guest invokes an imported function.
// =========================================================

impl bindings::torchsnap::plugin::logging::Host for PluginState {
    fn log(
        &mut self,
        level: bindings::torchsnap::plugin::logging::LogLevel,
        message: String,
        metadata: Vec<(String, String)>,
        span: Option<u64>,
    ) {
        let log_level = match level {
            bindings::torchsnap::plugin::logging::LogLevel::Trace => LogLevel::Trace,
            bindings::torchsnap::plugin::logging::LogLevel::Debug => LogLevel::Debug,
            bindings::torchsnap::plugin::logging::LogLevel::Info => LogLevel::Info,
            bindings::torchsnap::plugin::logging::LogLevel::Warn => LogLevel::Warn,
            bindings::torchsnap::plugin::logging::LogLevel::Error => LogLevel::Error,
        };

        self.log_sender.send(LogItem {
            seq: 0,
            timestamp: SystemTime::now(),
            source: LogSource::Plugin(self.plugin_id.clone()),
            kind: LogItemKind::Message {
                level: log_level,
                message,
                metadata,
                span_id: span,
            },
        });
    }

    fn span_start(
        &mut self,
        name: String,
        parent: Option<u64>,
        metadata: Vec<(String, String)>,
    ) -> u64 {
        let name_for_item = name.clone();
        let meta_for_item = metadata.clone();

        match self.span_registry.start(
            name,
            parent,
            LogSource::Plugin(self.plugin_id.clone()),
            metadata,
        ) {
            Some((id, depth)) => {
                // Emit span-start log item so the frontend can
                // track open spans in real time.
                self.log_sender.send(LogItem {
                    seq: 0,
                    timestamp: SystemTime::now(),
                    source: LogSource::Plugin(self.plugin_id.clone()),
                    kind: LogItemKind::SpanStart {
                        span_id: id,
                        name: name_for_item,
                        parent_id: parent,
                        depth,
                        metadata: meta_for_item,
                    },
                });
                id
            }
            // If nesting depth exceeded, return 0 as a sentinel.
            // The guest can still pass this to span_end, which
            // will be a no-op (ID not found in registry).
            None => 0,
        }
    }

    fn span_end(&mut self, span_id: u64, metadata: Vec<(String, String)>) {
        if let Some(completed) = self.span_registry.end(span_id, metadata) {
            self.log_sender.send(LogItem {
                seq: 0,
                timestamp: SystemTime::now(),
                source: completed.source.clone(),
                kind: completed.into(),
            });
        }
    }
}

// =========================================================
// Settings host import
//
// Routes guest `settings::get(key)` calls through the
// per-plugin `PluginSettings` handle stashed on
// `PluginState`. The plugin namespace prefix
// (`plugins.<id>.`) is added by `PluginSettings::get_raw`
// itself, so plugins can never escape their own bucket.
// =========================================================

impl bindings::torchsnap::plugin::settings::Host for PluginState {
    fn get(&mut self, key: String) -> Option<String> {
        // The bridge stashes `PluginSettings` BEFORE
        // calling the guest's `enable()`, so any guest
        // call (which can only run after `enable()`
        // returns) should always find the handle present.
        // The `debug_assert` documents that invariant and
        // fires loudly during development if the lifecycle
        // ever changes; in release builds we still
        // gracefully degrade to "unset" so a stale
        // `settings::get` (e.g. between disable and a
        // re-enable cycle that hasn't restashed yet)
        // returns `None` instead of panicking the guest.
        debug_assert!(
            self.settings.is_some(),
            "settings::get called before bridge stashed PluginSettings — lifecycle invariant broken",
        );
        let settings = self.settings.as_ref()?;
        settings.get_raw(&key)
    }
}

// =========================================================
// Frecency host import
//
// Routes guest `frecency::is-enabled` / `frecency::top-items`
// calls through the per-plugin `PluginFrecency` handle
// stashed on `PluginState`. Same "degrade gracefully when
// the handle is missing" contract as the settings import:
// an accidental call outside an enable lifetime returns
// empty results rather than trapping.
//
// Record and boost are intentionally NOT exposed here —
// the host already records selections before `execute()`
// dispatches and applies score bonuses to `search()` results
// before they reach the frontend, so plugins never need to
// touch those paths directly.
// =========================================================

impl bindings::torchsnap::plugin::frecency::Host for PluginState {
    fn is_enabled(&mut self) -> bool {
        self.frecency.as_ref().is_some_and(|f| f.is_enabled())
    }

    fn top_items(
        &mut self,
        limit: u32,
    ) -> Vec<bindings::torchsnap::plugin::frecency::FrecencyItem> {
        let Some(frecency) = self.frecency.as_ref() else {
            return Vec::new();
        };
        frecency
            .top_items(limit as usize)
            .into_iter()
            .map(Into::into)
            .collect()
    }
}

// =========================================================
// Clipboard host import
//
// Routes guest `clipboard::write-text(text)` calls through
// the closure stashed by the bridge on `enable()`. The
// closure wraps `tauri_plugin_clipboard_manager` so this
// module never depends on the Tauri AppHandle directly.
//
// Read access is intentionally not exposed by the WIT
// interface — see the doc comment on the `clipboard`
// interface in `torchsnap-plugin.wit`.
// =========================================================

impl bindings::torchsnap::plugin::clipboard::Host for PluginState {
    fn write_text(&mut self, text: String) -> Result<(), String> {
        let writer = self.clipboard_writer.as_ref().ok_or_else(|| {
            "clipboard writer not initialized — clipboard::write-text called outside enable lifetime"
                .to_string()
        })?;
        writer(&text)
    }
}

// =========================================================
// SQL host import
//
// The bridge materializes the per-plugin database during
// `enable()` before the guest runs. `sql::connection()`
// hands out lightweight handles backed by the same
// `Arc<SqlStorage>`. Migration strings are pre-loaded by
// the bridge at construction time from the manifest's
// `[storage.sql] migrations = [...]` list.
//
// Subsequent calls within the same enable lifetime return a
// new `Resource` handle pointing at the cached
// `Arc<SqlStorage>` — the underlying connection (and its
// internal mutex) is shared across every outstanding handle.
// Concurrent host imports are not actually possible because
// the wasmtime store mutex (`WasmPluginInstance::store`)
// already serializes every guest call.
//
// The Bindgen-generated `WitSqlValue` and `HostSqlHandle`
// trait names are used by-path here so the mapping between
// the WIT variant and the host's internal `SqlValue` is
// kept entirely in this file.
// =========================================================

impl bindings::torchsnap::plugin::sql::Host for PluginState {
    fn connection(&mut self) -> Resource<SqlHandleEntry> {
        // The bridge opens the database before the guest's
        // enable() runs, so sql_storage is always populated
        // for plugins that declared [storage.sql]. A missing
        // storage here means the plugin called connection()
        // without declaring storage — that's a bug, so we
        // trap rather than returning a Result the guest would
        // have to handle on every call.
        let storage = self.sql_storage.as_ref().expect(
            "sql::connection() called but no SQL storage is initialized — \
                     declare [storage.sql] in manifest.toml",
        );

        // Push a fresh resource entry pointing at the cached
        // master Arc. Wasmtime resources are unique handles —
        // we cannot return the literal same handle twice —
        // but every handle backs onto the same underlying
        // connection so the plugin sees identical semantics.
        let entry = SqlHandleEntry {
            storage: Arc::clone(storage),
        };
        let handle = self
            .wasi_table
            .push(entry)
            .expect("allocate SQL handle in resource table");

        // Track the rep so `clear_sql_storage` can drain
        // every outstanding handle on disable, even if the
        // guest forgot to drop them. Without this list, a
        // leaked handle would keep the rusqlite connection
        // alive until the entire WasmPluginInstance is
        // dropped (effectively until app shutdown).
        self.sql_handle_reps.push(handle.rep());

        handle
    }
}

impl bindings::torchsnap::plugin::sql::HostSqlHandle for PluginState {
    fn execute(
        &mut self,
        handle: Resource<SqlHandleEntry>,
        sql: String,
        params: Vec<bindings::torchsnap::plugin::sql::SqlValue>,
    ) -> Result<u64, String> {
        let entry = self
            .wasi_table
            .get(&handle)
            .map_err(|e| format!("resolve SQL handle: {e}"))?;
        let native_params: Vec<HostSqlValue> = params.into_iter().map(Into::into).collect();
        entry
            .storage
            .execute(&sql, &native_params)
            .map(|n| n as u64)
            .map_err(|e| format!("execute: {e:#}"))
    }

    fn query(
        &mut self,
        handle: Resource<SqlHandleEntry>,
        sql: String,
        params: Vec<bindings::torchsnap::plugin::sql::SqlValue>,
    ) -> Result<Vec<Vec<bindings::torchsnap::plugin::sql::SqlValue>>, String> {
        let entry = self
            .wasi_table
            .get(&handle)
            .map_err(|e| format!("resolve SQL handle: {e}"))?;
        let native_params: Vec<HostSqlValue> = params.into_iter().map(Into::into).collect();

        // Re-export every column from every row through the
        // WIT variant. `SqlRow::columns()` borrows the
        // materialized `Vec<SqlValue>` so we can clone the
        // values straight across without going through the
        // typed `FromSqlValue` accessor.
        let rows = entry
            .storage
            .query_map(&sql, &native_params, |row| Ok(row.columns().to_vec()))
            .map_err(|e| format!("query: {e:#}"))?;

        Ok(rows
            .into_iter()
            .map(|row| row.into_iter().map(Into::into).collect())
            .collect())
    }

    fn drop(&mut self, handle: Resource<SqlHandleEntry>) -> wasmtime::Result<()> {
        // Untrack the rep first so `clear_sql_storage`
        // doesn't try to double-delete it on disable. The
        // O(N) `retain` is fine — N is the number of
        // currently-outstanding handles, which for any
        // sensible plugin is a small number.
        let rep = handle.rep();
        self.sql_handle_reps.retain(|&r| r != rep);

        // Removing the entry drops just this resource's
        // clone of the master Arc. The underlying
        // SqlStorage stays alive on PluginState's
        // `sql_storage` field until the plugin is
        // disabled.
        self.wasi_table.delete(handle)?;
        Ok(())
    }
}

// ---------------------------------------------------------
// SqlValue ↔ host SqlValue
// ---------------------------------------------------------

impl From<bindings::torchsnap::plugin::sql::SqlValue> for HostSqlValue {
    fn from(v: bindings::torchsnap::plugin::sql::SqlValue) -> Self {
        use bindings::torchsnap::plugin::sql::SqlValue as Wit;
        match v {
            Wit::Null => HostSqlValue::Null,
            Wit::Integer(i) => HostSqlValue::Integer(i),
            Wit::Real(f) => HostSqlValue::Real(f),
            Wit::Text(s) => HostSqlValue::Text(s),
            Wit::Blob(b) => HostSqlValue::Blob(b),
        }
    }
}

impl From<HostSqlValue> for bindings::torchsnap::plugin::sql::SqlValue {
    fn from(v: HostSqlValue) -> Self {
        use bindings::torchsnap::plugin::sql::SqlValue as Wit;
        match v {
            HostSqlValue::Null => Wit::Null,
            HostSqlValue::Integer(i) => Wit::Integer(i),
            HostSqlValue::Real(f) => Wit::Real(f),
            HostSqlValue::Text(s) => Wit::Text(s),
            HostSqlValue::Blob(b) => Wit::Blob(b),
            // The host's `List` variant is for IN-clause
            // expansion before binding; it never appears in
            // result rows and never crosses the WIT
            // boundary. Plugins expand their own IN clauses
            // per ADR 0031. The arm is unreachable in
            // current code paths — `materialize_row` only
            // produces scalar variants from rusqlite — but
            // a `debug_assert` documents the invariant and
            // catches future violations during development
            // without panicking in release builds.
            HostSqlValue::List(_) => {
                debug_assert!(
                    false,
                    "SqlValue::List should never appear in result rows or cross the WIT boundary",
                );
                Wit::Null
            }
        }
    }
}

// =========================================================
// Opener host import
//
// Enforces the per-plugin scheme allowlist declared in
// `[permissions.opener]` before delegating to the
// `UrlOpenerFn` closure the bridge stashes on `enable()`.
//
// `check_opener_scheme` is a pure free function so the
// permission logic can be unit-tested without a running
// wasmtime instance.
// =========================================================

/// Outcome of a scheme-permission check on a URL passed to
/// `open-url`. Distinct error cases let the Host impl map
/// each into the right `OpenerError` variant for the WIT
/// boundary.
#[derive(Debug)]
enum OpenerSchemeCheckError {
    /// The URL string did not parse.
    InvalidUrl,
    /// The URL parsed but its scheme is not in `allowed`.
    SchemeNotPermitted(String),
}

/// Verify that `url`'s scheme is in `allowed`.
///
/// `allowed` strings are compared case-insensitively against
/// the scheme extracted by the `url` crate (which always
/// lowercases it per RFC 3986).
fn check_opener_scheme(allowed: &[String], url: &str) -> Result<(), OpenerSchemeCheckError> {
    let parsed = url::Url::parse(url).map_err(|_| OpenerSchemeCheckError::InvalidUrl)?;
    let scheme = parsed.scheme();
    if allowed.iter().any(|s| s.eq_ignore_ascii_case(scheme)) {
        Ok(())
    } else {
        Err(OpenerSchemeCheckError::SchemeNotPermitted(scheme.to_string()))
    }
}

impl bindings::torchsnap::plugin::opener::Host for PluginState {
    fn open_url(
        &mut self,
        url: String,
    ) -> Result<(), bindings::torchsnap::plugin::opener::OpenerError> {
        use bindings::torchsnap::plugin::opener::OpenerError;

        match check_opener_scheme(&self.opener_schemes, &url) {
            Ok(()) => {}
            Err(OpenerSchemeCheckError::InvalidUrl) => {
                return Err(OpenerError::InvalidUrl(url));
            }
            Err(OpenerSchemeCheckError::SchemeNotPermitted(scheme)) => {
                return Err(OpenerError::PermissionDenied(format!(
                    "scheme not permitted: {scheme}"
                )));
            }
        }

        let writer = self
            .opener_writer
            .as_ref()
            .ok_or_else(|| OpenerError::BackendFailure("opener not initialized".into()))?;
        writer(&url).map_err(OpenerError::BackendFailure)
    }

    fn open_path(
        &mut self,
        path: String,
    ) -> Result<(), bindings::torchsnap::plugin::opener::OpenerError> {
        use bindings::torchsnap::plugin::opener::OpenerError;

        if !self.opener_open_path {
            return Err(OpenerError::PermissionDenied(
                "open-path not granted: set `[permissions.opener] open-path = true`".into(),
            ));
        }
        let writer = self
            .open_path_writer
            .as_ref()
            .ok_or_else(|| OpenerError::BackendFailure("open-path not initialized".into()))?;
        writer(&path).map_err(OpenerError::BackendFailure)
    }

    fn reveal_path(
        &mut self,
        path: String,
    ) -> Result<(), bindings::torchsnap::plugin::opener::OpenerError> {
        use bindings::torchsnap::plugin::opener::OpenerError;

        if !self.opener_reveal_path {
            return Err(OpenerError::PermissionDenied(
                "reveal-path not granted: set `[permissions.opener] reveal-path = true`".into(),
            ));
        }
        let writer = self
            .reveal_path_writer
            .as_ref()
            .ok_or_else(|| OpenerError::BackendFailure("reveal-path not initialized".into()))?;
        writer(&path).map_err(OpenerError::BackendFailure)
    }
}

// =========================================================
// Command, Platform, Paths host imports — Phase D stubs
//
// The WIT additions in Phase D require Host trait impls so
// `Plugin::add_to_linker` accepts `PluginState`. The full
// implementations land in Phase E (process spawn,
// PathContext stashing, runtime substitution); these
// scaffolds keep the build green in the meantime.
//
// `platform` is small enough that the "stub" is the real
// thing — there is no plugin-state-dependent work for
// returning the host's OS or architecture.
// =========================================================

impl bindings::torchsnap::plugin::command::Host for PluginState {
    fn run(
        &mut self,
        _binary: String,
        _options: bindings::torchsnap::plugin::command::CommandOptions,
    ) -> Result<
        bindings::torchsnap::plugin::command::CommandResult,
        bindings::torchsnap::plugin::command::CommandError,
    > {
        Err(
            bindings::torchsnap::plugin::command::CommandError::PermissionDenied(
                "command::run not yet wired to host runtime".into(),
            ),
        )
    }
}

impl bindings::torchsnap::plugin::platform::Host for PluginState {
    fn current_os(&mut self) -> bindings::torchsnap::plugin::platform::Os {
        use bindings::torchsnap::plugin::platform::Os;

        if cfg!(target_os = "macos") {
            Os::Macos
        } else if cfg!(target_os = "linux") {
            Os::Linux
        } else if cfg!(target_os = "windows") {
            Os::Windows
        } else {
            Os::Other(std::env::consts::OS.to_string())
        }
    }

    fn current_arch(&mut self) -> bindings::torchsnap::plugin::platform::Arch {
        use bindings::torchsnap::plugin::platform::Arch;

        if cfg!(target_arch = "x86_64") {
            Arch::X8664
        } else if cfg!(target_arch = "aarch64") {
            Arch::Aarch64
        } else {
            Arch::Other(std::env::consts::ARCH.to_string())
        }
    }
}

impl bindings::torchsnap::plugin::paths::Host for PluginState {
    fn resolve(
        &mut self,
        template: String,
    ) -> Result<String, bindings::torchsnap::plugin::paths::ResolveError> {
        use super::permission_vars::{substitute_variables, ResolveError};
        use bindings::torchsnap::plugin::paths::ResolveError as WitResolveError;

        let Some(ctx) = self.path_context.as_ref() else {
            // Mirrors the contract of other capability stashes
            // (`http_client`, `clipboard_writer`): if the
            // bridge has not stashed the context yet, surface
            // it as an error rather than panicking.
            return Err(WitResolveError::Unterminated(
                "paths interface not initialized for this plugin instance".into(),
            ));
        };

        substitute_variables(&template, ctx).map_err(|e| match e {
            ResolveError::UnknownVariable(name) => WitResolveError::UnknownVariable(name),
            ResolveError::Unterminated(rest) => WitResolveError::Unterminated(rest),
        })
    }
}

// =========================================================
// HTTP host import
//
// A minimal synchronous HTTP client for WASM plugins. The
// host function body must NOT call `Http::send()` directly
// from within a tokio async task — that would cause
// `Handle::block_on()` inside `Http::send()` to panic.
//
// WASM guest calls are dispatched inline from tokio tasks
// (the bridge task scheduler calls `instance.run_task()`
// without `spawn_blocking`), so we use
// `tokio::task::block_in_place` here. This moves the
// current thread out of the async pool temporarily,
// making the inner `block_on()` safe while letting tokio
// schedule other tasks on remaining threads.
//
// Both `check_http_origin` and `wit_method_to_reqwest` are
// pure free functions for the same testability reason as
// `check_opener_scheme` above.
// =========================================================

/// Internal error type that classifies transport-level
/// failures before mapping to the WIT `http-error` variant.
#[derive(Debug, thiserror::Error)]
enum WasmHttpError {
    #[error("origin not permitted: {0}")]
    PermissionDenied(String),
    #[error("request timed out")]
    Timeout,
    #[error("network error: {0}")]
    Network(String),
}

impl WasmHttpError {
    /// Classify an `anyhow::Error` from `Http::send()` or
    /// `HttpResponse::bytes()` into the three WIT variants.
    ///
    /// `Http` adds context wrappers so `downcast_ref` on the
    /// outer `anyhow::Error` won't find `reqwest::Error`
    /// directly. We try both the direct downcast (for the
    /// unwrapped case) and scan the formatted chain for
    /// timeout indicators (for context-wrapped errors).
    fn from_request_error(e: anyhow::Error) -> Self {
        if e.downcast_ref::<reqwest::Error>()
            .map(|re| re.is_timeout())
            .unwrap_or(false)
        {
            return Self::Timeout;
        }
        let is_timeout = e.chain().any(|cause| {
            let s = cause.to_string();
            s.contains("timed out") || s.contains("deadline has elapsed")
        });
        if is_timeout {
            Self::Timeout
        } else {
            Self::Network(e.to_string())
        }
    }
}

impl From<WasmHttpError> for bindings::torchsnap::plugin::http::HttpError {
    fn from(e: WasmHttpError) -> Self {
        use bindings::torchsnap::plugin::http::HttpError;
        match e {
            WasmHttpError::PermissionDenied(msg) => HttpError::PermissionDenied(msg),
            WasmHttpError::Timeout => HttpError::Timeout,
            WasmHttpError::Network(msg) => HttpError::Network(msg),
        }
    }
}

/// Verify that `url`'s origin is in the `allowed` list.
///
/// `"*"` short-circuits before URL parsing (trust-all). All
/// other entries are compared against the request URL's
/// ASCII-serialized origin, which normalizes scheme,
/// host, and port identically to how the manifest validation
/// stored them.
fn check_http_origin(allowed: &[String], url: &str) -> Result<(), WasmHttpError> {
    if allowed.iter().any(|o| o == "*") {
        return Ok(());
    }
    let parsed =
        url::Url::parse(url).map_err(|e| WasmHttpError::Network(format!("invalid URL: {e}")))?;
    let origin = parsed.origin().ascii_serialization();
    if allowed.iter().any(|o| o == &origin) {
        Ok(())
    } else {
        Err(WasmHttpError::PermissionDenied(origin))
    }
}

/// Map a WIT `http-method` variant to a `reqwest::Method`.
///
/// `other(string)` is validated via `Method::from_bytes` —
/// an invalid method string maps to `WasmHttpError::Network`
/// rather than being sent over the wire.
fn wit_method_to_reqwest(
    method: bindings::torchsnap::plugin::http::HttpMethod,
) -> Result<reqwest::Method, WasmHttpError> {
    use bindings::torchsnap::plugin::http::HttpMethod;
    match method {
        HttpMethod::Get => Ok(reqwest::Method::GET),
        HttpMethod::Post => Ok(reqwest::Method::POST),
        HttpMethod::Put => Ok(reqwest::Method::PUT),
        HttpMethod::Patch => Ok(reqwest::Method::PATCH),
        HttpMethod::Delete => Ok(reqwest::Method::DELETE),
        HttpMethod::Head => Ok(reqwest::Method::HEAD),
        HttpMethod::Other(s) => reqwest::Method::from_bytes(s.as_bytes())
            .map_err(|e| WasmHttpError::Network(format!("invalid HTTP method: {e}"))),
    }
}

impl bindings::torchsnap::plugin::http::Host for PluginState {
    fn fetch(
        &mut self,
        request: bindings::torchsnap::plugin::http::HttpRequest,
    ) -> Result<
        bindings::torchsnap::plugin::http::HttpResponse,
        bindings::torchsnap::plugin::http::HttpError,
    > {
        use bindings::torchsnap::plugin::http::HttpError as WitHttpError;
        use bindings::torchsnap::plugin::http::HttpResponse as WitHttpResponse;

        check_http_origin(&self.http_origins, &request.url).map_err(WitHttpError::from)?;

        let method = wit_method_to_reqwest(request.method).map_err(WitHttpError::from)?;

        let client = self.http_client.as_ref().ok_or_else(|| {
            WitHttpError::from(WasmHttpError::Network("http client not initialized".into()))
        })?;

        let mut builder = client.request(method, &request.url);
        for (k, v) in request.headers {
            builder = builder.header(&k, &v);
        }
        if let Some(body) = request.body {
            builder = builder.body(body);
        }
        if let Some(ms) = request.timeout_ms {
            builder = builder.timeout(Duration::from_millis(ms as u64));
        }
        if let Some(max) = request.max_body_size {
            builder = builder.max_size(max);
        }

        // `Http::send()` uses `Handle::block_on()` internally, which panics
        // if called from within a tokio async task. WASM guest calls are
        // dispatched inline from async tasks (see bridge.rs task scheduler),
        // so we use `block_in_place` to move this thread out of the async
        // pool before blocking.
        let (status, headers, body) =
            tokio::task::block_in_place(|| -> Result<_, WasmHttpError> {
                let mut response = builder.send().map_err(WasmHttpError::from_request_error)?;
                let status = response.status();
                let headers = response.headers().to_vec();
                let body = response
                    .bytes()
                    .map_err(WasmHttpError::from_request_error)?;
                Ok((status, headers, body))
            })
            .map_err(WitHttpError::from)?;

        Ok(WitHttpResponse {
            status,
            headers,
            body,
        })
    }
}

// =========================================================
// Assets host import
//
// Routes guest `assets::read` / `assets::exists` calls through
// the plugin's own `PluginSource` (stashed on the bridge on
// `enable`). Path validation runs on the host side BEFORE
// touching the source, producing the `InvalidPath` variant
// directly; that lets us skip error-string matching on the
// `anyhow::Error` the trait returns.
//
// `NotFound` is surfaced differently in each direction:
// - `read`: pre-probe with `file_exists`. Only call `read_file`
//   on hit, so the "missing" path is reported structurally.
// - `exists`: the source's `file_exists` returns `Ok(false)` on
//   miss, which maps directly to `Ok(false)` in WIT.
//
// The `read` pre-probe has a benign TOCTOU: a file that
// passes `file_exists` can be unlinked between the two calls
// (realistic only for `DirectorySource` during development;
// archive entries never race). In that narrow case the guest
// sees `IoError` rather than `NotFound`. Accept this; the
// fix would require a richer trait error enum.
//
// Any remaining error from the source — filesystem, archive
// lookup, unexpected zip variant — collapses to `IoError`.
// =========================================================

/// Map a `PluginSource` error to the `assets::io-error`
/// variant. Pure function, unit-testable without wasmtime.
fn into_assets_io_error(e: anyhow::Error) -> bindings::torchsnap::plugin::assets::AssetsError {
    bindings::torchsnap::plugin::assets::AssetsError::IoError(format!("{e:#}"))
}

impl bindings::torchsnap::plugin::assets::Host for PluginState {
    fn read(
        &mut self,
        path: String,
    ) -> Result<Vec<u8>, bindings::torchsnap::plugin::assets::AssetsError> {
        use bindings::torchsnap::plugin::assets::AssetsError;

        // Validate first so a structured `InvalidPath`
        // variant is returned without having to grep the
        // trait's `anyhow::Error` for a guard message.
        if let Err(e) = super::source::validate_plugin_path(&path) {
            return Err(AssetsError::InvalidPath(format!("{e:#}")));
        }

        let source = self
            .plugin_source
            .as_ref()
            .ok_or_else(|| AssetsError::IoError("assets not initialized".into()))?;

        // Pre-probe so the "missing" case becomes a
        // structural `NotFound` variant; the alternative —
        // attempting the read and matching on the error
        // string — would be fragile across filesystem /
        // archive backends.
        match source.file_exists(&path) {
            Ok(true) => {}
            Ok(false) => return Err(AssetsError::NotFound),
            Err(e) => return Err(into_assets_io_error(e)),
        }

        source.read_file(&path).map_err(into_assets_io_error)
    }

    fn exists(
        &mut self,
        path: String,
    ) -> Result<bool, bindings::torchsnap::plugin::assets::AssetsError> {
        use bindings::torchsnap::plugin::assets::AssetsError;

        if let Err(e) = super::source::validate_plugin_path(&path) {
            return Err(AssetsError::InvalidPath(format!("{e:#}")));
        }

        let source = self
            .plugin_source
            .as_ref()
            .ok_or_else(|| AssetsError::IoError("assets not initialized".into()))?;

        source.file_exists(&path).map_err(into_assets_io_error)
    }
}

// =========================================================
// WasmRuntime — shared across all plugins
// =========================================================

/// Shared WASM runtime holding the wasmtime Engine and a
/// per-plugin compiled-component cache.
///
/// One instance per application. A cached `Component` is
/// small relative to the `Store` that backs a live instance
/// — it holds native code for the guest's imports/exports
/// but no linear memory, resource tables, or per-enable
/// state — so keeping compiled components around for
/// disabled plugins is cheap.
pub struct WasmRuntime {
    engine: Engine,
    log_sender: LogSender,
    span_registry: Arc<SpanRegistry>,
    /// Keyed by plugin ID. Populated by `compile()`,
    /// consumed by `instantiate()`. Re-compiling the same
    /// ID replaces the existing entry — belt-and-suspenders
    /// for a future hot-reload path, not wired up by any
    /// current code path.
    components: Mutex<HashMap<String, Component>>,
}

impl WasmRuntime {
    /// Create a new runtime with default configuration.
    ///
    /// Returns `Arc<Self>` so every bridge can hold a cheap
    /// clone for on-demand instantiation without borrowing
    /// from the call stack that loaded it.
    pub fn new(
        log_sender: LogSender,
        span_registry: Arc<SpanRegistry>,
    ) -> anyhow::Result<Arc<Self>> {
        let mut config = Config::new();
        config.wasm_component_model(true);

        let engine =
            Engine::new(&config).map_err(|e| anyhow::anyhow!("creating wasmtime engine: {e}"))?;
        Ok(Arc::new(Self {
            engine,
            log_sender,
            span_registry,
            components: Mutex::new(HashMap::new()),
        }))
    }

    /// Create a Logger for a specific plugin, used for
    /// host-side span creation around guest calls.
    fn logger_for(&self, plugin_id: &str) -> Logger {
        Logger::new(
            self.log_sender.clone(),
            Arc::clone(&self.span_registry),
            LogSource::Plugin(plugin_id.to_string()),
        )
    }

    /// Compile a WASM component's bytes and cache the
    /// resulting `Component` under `plugin_id`.
    ///
    /// Must be called before the first `instantiate()` for
    /// that ID. Compilation is the expensive step (parse +
    /// validate + codegen), so running it up front surfaces
    /// broken WASM as a load error and keeps `instantiate()`
    /// cheap enough to repeat on demand.
    ///
    /// Re-compiling an existing entry replaces the cached
    /// `Component` — no current code path triggers this,
    /// but the branch is kept for a future hot-reload path.
    pub fn compile(&self, plugin_id: &str, wasm_bytes: &[u8]) -> anyhow::Result<()> {
        let logger = self.logger_for(plugin_id);
        let _compile_span = logger.span("compile").meta("plugin_id", plugin_id).start();

        let component = Component::new(&self.engine, wasm_bytes)
            .map_err(|e| anyhow::anyhow!("compiling WASM component: {e}"))?;

        self.components
            .lock()
            .expect("wasm component cache not poisoned")
            .insert(plugin_id.to_string(), component);

        Ok(())
    }

    /// Instantiate a previously-compiled WASM plugin.
    ///
    /// Builds a fresh linker + `WasiCtx` + `PluginState` +
    /// `Store` against the cached `Component` and returns a
    /// ready-to-call instance. The linker is rebuilt on
    /// every call because it references `PluginState`, but
    /// linker construction is just map inserts (no WASM
    /// compilation) and `instantiate` is not in a hot path.
    ///
    /// Errors if `plugin_id` has not been compiled — that
    /// is an internal lifecycle bug, not a user-facing
    /// failure mode.
    pub fn instantiate(&self, plugin_id: &str) -> anyhow::Result<WasmPluginInstance> {
        let logger = self.logger_for(plugin_id);
        let _instantiate_span = logger
            .span("instantiate")
            .meta("plugin_id", plugin_id)
            .start();

        // Clone the `Component` out of the cache under the
        // lock (wasmtime's `Component` is an `Arc` internally
        // so cloning is cheap) and drop the guard before the
        // linker/store work, so concurrent plugin loads don't
        // block on each other.
        let component = {
            let cache = self
                .components
                .lock()
                .expect("wasm component cache not poisoned");
            cache.get(plugin_id).cloned().ok_or_else(|| {
                anyhow::anyhow!(
                    "plugin `{plugin_id}` has not been compiled — call WasmRuntime::compile first"
                )
            })?
        };

        // Set up the linker with all host imports.
        let mut linker = Linker::<PluginState>::new(&self.engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
            .map_err(|e| anyhow::anyhow!("linking WASI imports: {e}"))?;
        bindings::Plugin::add_to_linker::<PluginState, HasSelf<PluginState>>(
            &mut linker,
            |state| state,
        )
        .map_err(|e| anyhow::anyhow!("linking plugin imports: {e}"))?;

        // Build the WASI context. Minimal sandbox: no filesystem,
        // no network, no env vars. Stdout/stderr are inherited
        // for now (plugin println! goes to the host's terminal).
        // TODO: Redirect stdout/stderr to the logging system.
        let wasi = WasiCtxBuilder::new()
            .inherit_stdout()
            .inherit_stderr()
            .build();

        let state = PluginState {
            plugin_id: plugin_id.to_string(),
            wasi,
            wasi_table: ResourceTable::new(),
            log_sender: self.log_sender.clone(),
            span_registry: Arc::clone(&self.span_registry),
            // Stashed by the bridge on `enable()` via
            // `WasmPluginInstance::set_settings`. The
            // `settings::get` host import is a no-op until
            // then; the bridge always sets it before the
            // first guest call into `enable()`.
            settings: None,
            // Re-applied on every instantiation by the
            // bridge's `ensure_instance` helper. The
            // materialized `SqlConfig` lives on the bridge
            // (built once from the manifest at bridge
            // construction) and is copied onto each fresh
            // `PluginState` so migration strings and the
            // database path survive disable/re-enable
            // cycles without re-reading the plugin source.
            sql_config: SqlConfig::None,
            sql_storage: None,
            sql_handle_reps: Vec::new(),
            // Stashed by the bridge on `enable()` via
            // `WasmPluginInstance::set_clipboard_writer`,
            // built from the AppHandle. `None` outside an
            // enable lifetime; the host import returns an
            // error in that case.
            clipboard_writer: None,
            // Stashed by the bridge on `enable()` via
            // `WasmPluginInstance::set_frecency`. The
            // `frecency::*` host imports gracefully degrade
            // to "disabled / empty" when the handle is
            // missing — same contract as `settings`.
            frecency: None,
            // Both scheme/origin lists and the opener/http
            // capabilities are stashed by the bridge on
            // `enable()`. Calls outside an enable lifetime
            // degrade gracefully: permission lists are empty
            // (deny-all) and the writer/client are `None`.
            opener_schemes: Vec::new(),
            opener_writer: None,
            opener_open_path: false,
            opener_reveal_path: false,
            open_path_writer: None,
            reveal_path_writer: None,
            http_origins: Vec::new(),
            http_client: None,
            // Stashed by the bridge on `enable()` via
            // `WasmPluginInstance::set_plugin_source`. The
            // `assets::*` host imports return `io-error`
            // until then, matching the capability-stash
            // contract of `http_client` / `clipboard_writer`.
            plugin_source: None,
            path_context: None,
        };

        let mut store = Store::new(&self.engine, state);

        // Instantiate the component and get the typed bindings.
        let plugin = bindings::Plugin::instantiate(&mut store, &component, &linker)
            .map_err(|e| anyhow::anyhow!("instantiating WASM plugin: {e}"))?;

        Ok(WasmPluginInstance {
            store: Mutex::new(store),
            plugin,
            logger,
        })
    }
}

// =========================================================
// WasmPluginInstance — one per loaded plugin
// =========================================================

/// A loaded WASM plugin instance.
///
/// Wraps the wasmtime Store and typed Plugin bindings. All
/// guest calls go through the Mutex-protected Store to
/// satisfy `Send + Sync` requirements.
pub struct WasmPluginInstance {
    store: Mutex<Store<PluginState>>,
    plugin: bindings::Plugin,
    logger: Logger,
}

impl WasmPluginInstance {
    /// Stash a per-plugin `PluginSettings` handle on the
    /// store data so the `settings::get` host import can
    /// resolve reads. Called by the bridge from `enable()`
    /// before the guest's own `enable()` runs.
    ///
    /// Replacing an existing handle is allowed (re-enable
    /// after disable hands in a fresh `PluginContext`).
    pub fn set_settings(&self, settings: PluginSettings) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().settings = Some(settings);
    }

    /// Drop the stashed `PluginSettings` handle. Called by
    /// the bridge from `disable()` so the host import
    /// reverts to "unset" between enable cycles.
    pub fn clear_settings(&self) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().settings = None;
    }

    /// Install the SQL storage configuration on the store
    /// data. Called once by the bridge at construction time
    /// (before any guest call) — the migration strings have
    /// already been read from the plugin source by the
    /// caller, so this is a pure metadata stash with no
    /// I/O.
    pub fn set_sql_config(&self, config: SqlConfig) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().sql_config = config;
    }

    /// Create the database file, configure pragmas, and run
    /// migrations. Called by the bridge during `enable()`
    /// before invoking the guest's own `enable()`, so
    /// `sql::connection()` is ready by the time the plugin
    /// runs. No-op when the plugin has no `[storage.sql]`
    /// declaration.
    pub fn open_sql_storage(&self) -> anyhow::Result<()> {
        let mut store = self.store.lock().expect("store not poisoned");
        let data = store.data_mut();

        let (db_path, migrations) = match &data.sql_config {
            SqlConfig::None => return Ok(()),
            SqlConfig::Configured {
                db_path,
                migrations,
            } => (db_path.clone(), Arc::clone(migrations)),
        };

        if data.sql_storage.is_none() {
            let migration_strs: Vec<&str> = migrations.iter().map(String::as_str).collect();
            let storage = SqlStorage::open(db_path, &migration_strs).context("open SQL storage")?;
            data.sql_storage = Some(Arc::new(storage));
        }

        Ok(())
    }

    /// Drop every outstanding SQL handle and the cached
    /// `Arc<SqlStorage>` so the rusqlite `Connection` is
    /// fully closed on disable.
    ///
    /// Drain order matters: we delete the handle entries
    /// from `wasi_table` first (each delete drops one
    /// `Arc<SqlStorage>` clone), then clear the master
    /// `sql_storage` reference. After both steps the strong
    /// count is zero and `SqlStorage::Drop` runs, closing
    /// the connection and the SQLite file. Without this
    /// drain, a plugin that opened a handle and never
    /// explicitly dropped it would leak the connection
    /// until the entire `WasmPluginInstance` is dropped
    /// (effectively until app shutdown).
    ///
    /// `wasi_table.delete` errors (e.g., a stale rep that
    /// was already deleted) are ignored — the goal is best-
    /// effort reclaim, not strict accounting.
    pub fn clear_sql_storage(&self) {
        let mut store = self.store.lock().expect("store not poisoned");
        let data = store.data_mut();
        let reps = std::mem::take(&mut data.sql_handle_reps);
        for rep in reps {
            // `Resource::new_own(rep)` reconstructs an owned
            // resource handle from the raw rep so we can
            // hand it to `wasi_table.delete`. The original
            // `Resource` returned to the guest is gone (or
            // we wouldn't be in clear-on-disable territory),
            // but the rep alone is sufficient for the
            // table to look up and remove the entry.
            let resource: Resource<SqlHandleEntry> = Resource::new_own(rep);
            let _ = data.wasi_table.delete(resource);
        }
        data.sql_storage = None;
    }

    /// Install a closure that writes a string to the system
    /// clipboard. Called by the bridge on `enable()` from a
    /// closure that captures the `tauri::AppHandle`.
    pub fn set_clipboard_writer(&self, writer: ClipboardWriter) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().clipboard_writer = Some(writer);
    }

    /// Drop the stashed clipboard writer on `disable()` so
    /// any post-disable `clipboard::write-text` call (which
    /// shouldn't happen) errors loudly instead of silently
    /// using a stale closure.
    pub fn clear_clipboard_writer(&self) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().clipboard_writer = None;
    }

    /// Stash a per-plugin `PluginFrecency` handle on the store
    /// data so the `frecency::*` host imports can resolve
    /// reads. Called by the bridge from `enable()` before the
    /// guest's own `enable()` runs — same contract as
    /// `set_settings`.
    pub fn set_frecency(&self, frecency: PluginFrecency) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().frecency = Some(frecency);
    }

    /// Drop the stashed frecency handle on `disable()`. The
    /// host import reverts to "disabled / empty" between
    /// enable cycles, matching the `settings` lifecycle.
    pub fn clear_frecency(&self) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().frecency = None;
    }

    /// Stash the URL scheme allowlist for `opener::open-url`.
    /// Called by the bridge at `enable()` from the manifest's
    /// `[permissions.opener].schemes` list.
    pub fn set_opener_schemes(&self, schemes: Vec<String>) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().opener_schemes = schemes;
    }

    /// Install the closure that opens a URL via the OS default
    /// handler. Called by the bridge at `enable()`.
    pub fn set_opener_writer(&self, writer: UrlOpenerFn) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().opener_writer = Some(writer);
    }

    /// Drop the opener closure on `disable()`.
    pub fn clear_opener_writer(&self) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().opener_writer = None;
    }

    /// Stash the `open-path` / `reveal-path` capability flags from
    /// `[permissions.opener]`. Called by the bridge at `enable()`.
    pub fn set_opener_path_capabilities(&self, open_path: bool, reveal_path: bool) {
        let mut store = self.store.lock().expect("store not poisoned");
        let state = store.data_mut();
        state.opener_open_path = open_path;
        state.opener_reveal_path = reveal_path;
    }

    /// Install the closure that opens a filesystem path via the
    /// OS-registered application. Called by the bridge at
    /// `enable()`.
    pub fn set_open_path_writer(&self, writer: UrlOpenerFn) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().open_path_writer = Some(writer);
    }

    /// Drop the `open-path` closure on `disable()`.
    pub fn clear_open_path_writer(&self) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().open_path_writer = None;
    }

    /// Install the closure that reveals a filesystem path in the
    /// OS file manager. Called by the bridge at `enable()`.
    pub fn set_reveal_path_writer(&self, writer: UrlOpenerFn) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().reveal_path_writer = Some(writer);
    }

    /// Drop the `reveal-path` closure on `disable()`.
    pub fn clear_reveal_path_writer(&self) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().reveal_path_writer = None;
    }

    /// Stash the resolved `${...}` substitution context.
    /// Called by the bridge at `enable()` after computing the
    /// per-plugin paths.
    pub fn set_path_context(&self, ctx: super::permission_vars::PathContext) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().path_context = Some(ctx);
    }

    /// Drop the substitution context on `disable()`.
    pub fn clear_path_context(&self) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().path_context = None;
    }

    /// Stash the origin allowlist for `http::fetch`. Called by
    /// the bridge at `enable()` from the manifest's
    /// `[permissions.http].origins` list (already normalized).
    pub fn set_http_origins(&self, origins: Vec<String>) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().http_origins = origins;
    }

    /// Install the HTTP client. Called by the bridge at
    /// `enable()`.
    pub fn set_http_client(&self, client: Arc<crate::network::Http>) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().http_client = Some(client);
    }

    /// Drop the HTTP client on `disable()` so the connection
    /// pool is released between enable cycles.
    pub fn clear_http_client(&self) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().http_client = None;
    }

    /// Stash the plugin's own `PluginSource` handle. Called
    /// by the bridge on `enable()`. The `assets::*` host
    /// imports use this Arc to read the plugin's bundled
    /// files on demand. Same lifecycle contract as
    /// `set_http_client`.
    pub fn set_plugin_source(&self, source: Arc<dyn super::source::PluginSource + Send + Sync>) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().plugin_source = Some(source);
    }

    /// Drop the plugin source on `disable()`. Eager release
    /// so the underlying `ArchiveSource` file handle (or
    /// the `DirectorySource` path) doesn't linger across
    /// enable cycles.
    pub fn clear_plugin_source(&self) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().plugin_source = None;
    }

    /// Call the guest's `enable` export.
    pub fn enable(&self) -> anyhow::Result<()> {
        let _span = self.logger.span("enable").start();
        let mut store = self.store.lock().expect("store not poisoned");
        self.plugin
            .torchsnap_plugin_lifecycle()
            .call_enable(&mut *store)
            .map_err(|e| anyhow::anyhow!("calling plugin enable(): {e}"))
    }

    /// Call the guest's `on-setting-changed` export. Used by
    /// the bridge's `setting_changed` trait override after the
    /// host's `CoalescingDispatcher` has deduplicated rapid
    /// writes to the same key.
    pub fn on_setting_changed(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let _span = self
            .logger
            .span("on_setting_changed")
            .meta("key", key)
            .start();
        let mut store = self.store.lock().expect("store not poisoned");
        self.plugin
            .torchsnap_plugin_lifecycle()
            .call_on_setting_changed(&mut *store, key, value)
            .map_err(|e| anyhow::anyhow!("calling plugin on_setting_changed(): {e}"))
    }

    /// Call the guest's `tasks::run-task` export.
    ///
    /// Used by the per-plugin scheduler loop to fire a
    /// scheduled task. The `task_id` matches a `[[tasks]]`
    /// entry from the manifest. The plugin dispatches by
    /// name and runs whatever work the task is supposed to
    /// do.
    ///
    /// The outer `Result` is for wasmtime trap /
    /// serialization errors; the inner
    /// `Result<(), String>` is the plugin's own
    /// success/error arm. Returning `Err(string)` is
    /// logged by the bridge — it does not auto-disable the
    /// plugin.
    #[allow(clippy::type_complexity)]
    pub fn run_task(&self, task_id: &str) -> anyhow::Result<Result<(), String>> {
        let _span = self
            .logger
            .span("run_task")
            .meta("task_id", task_id)
            .start();
        let mut store = self.store.lock().expect("store not poisoned");
        self.plugin
            .torchsnap_plugin_tasks()
            .call_run_task(&mut *store, task_id)
            .map_err(|e| anyhow::anyhow!("calling plugin run_task(): {e}"))
    }

    /// Call the guest's `messaging::handle-message` export.
    ///
    /// `payload` is a JSON-encoded string (the plugin parses
    /// it on its side). The returned outer `Result` is for
    /// wasmtime trap / serialization errors; the inner
    /// `Result<String, String>` is the plugin's own
    /// success/error arm. The success arm is the
    /// JSON-encoded response string.
    #[allow(clippy::type_complexity)]
    pub fn handle_message(
        &self,
        method: &str,
        payload: &str,
    ) -> anyhow::Result<Result<String, String>> {
        let _span = self
            .logger
            .span("handle_message")
            .meta("method", method)
            .start();
        let mut store = self.store.lock().expect("store not poisoned");
        self.plugin
            .torchsnap_plugin_messaging()
            .call_handle_message(&mut *store, method, payload)
            .map_err(|e| anyhow::anyhow!("calling plugin handle_message(): {e}"))
    }

    /// Call the guest's `disable` export.
    pub fn disable(&self) -> anyhow::Result<()> {
        let _span = self.logger.span("disable").start();
        let mut store = self.store.lock().expect("store not poisoned");
        self.plugin
            .torchsnap_plugin_lifecycle()
            .call_disable(&mut *store)
            .map_err(|e| anyhow::anyhow!("calling plugin disable(): {e}"))
    }

    /// Call the guest's `entries` export and convert to native types.
    pub fn entries(&self) -> anyhow::Result<Vec<crate::search::types::CatalogEntry>> {
        let _span = self.logger.span("entries").start();
        let mut store = self.store.lock().expect("store not poisoned");
        let wit_entries = self
            .plugin
            .torchsnap_plugin_search()
            .call_entries(&mut *store)
            .map_err(|e| anyhow::anyhow!("calling plugin entries(): {e}"))?;

        Ok(wit_entries.into_iter().map(Into::into).collect())
    }

    /// Call the guest's `search` export and convert to native types.
    pub fn search(
        &self,
        query: &str,
        matched_prefix: Option<&str>,
    ) -> anyhow::Result<crate::search::types::PluginResponse> {
        let _span = self.logger.span("search").meta("query", query).start();
        let mut store = self.store.lock().expect("store not poisoned");
        let response = self
            .plugin
            .torchsnap_plugin_search()
            .call_search(&mut *store, query, matched_prefix)
            .map_err(|e| anyhow::anyhow!("calling plugin search(): {e}"))?;

        Ok(response.into())
    }

    /// Call the guest's `execute` export and convert to native types.
    pub fn execute(
        &self,
        entry_id: &str,
        action_id: &crate::search::types::ActionId,
    ) -> anyhow::Result<crate::search::types::PostAction> {
        let _span = self
            .logger
            .span("execute")
            .meta("entry_id", entry_id)
            .start();
        let mut store = self.store.lock().expect("store not poisoned");

        let wit_action_id: bindings::exports::torchsnap::plugin::search::ActionId =
            action_id.clone().into();

        let result = self
            .plugin
            .torchsnap_plugin_search()
            .call_execute(&mut *store, entry_id, &wit_action_id)
            .map_err(|e| anyhow::anyhow!("calling plugin execute(): {e}"))?;

        match result {
            Ok(post_action) => Ok(post_action.into()),
            Err(msg) => anyhow::bail!("plugin execute() returned error: {msg}"),
        }
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
impl PluginState {
    /// Construct a `PluginState` with all capability fields set
    /// to their "absent" defaults. Tests override specific fields
    /// with struct update syntax (`..PluginState::default_for_test()`).
    fn default_for_test() -> Self {
        let wasi = WasiCtxBuilder::new().build();
        PluginState {
            plugin_id: "test-plugin".to_string(),
            wasi,
            wasi_table: ResourceTable::new(),
            log_sender: LogSender::test_sender(),
            span_registry: Arc::new(SpanRegistry::new()),
            settings: None,
            sql_config: SqlConfig::None,
            sql_storage: None,
            sql_handle_reps: Vec::new(),
            clipboard_writer: None,
            frecency: None,
            opener_schemes: Vec::new(),
            opener_writer: None,
            opener_open_path: false,
            opener_reveal_path: false,
            open_path_writer: None,
            reveal_path_writer: None,
            http_origins: Vec::new(),
            http_client: None,
            plugin_source: None,
            path_context: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes of the committed `minimal-plugin` fixture. The
    /// fixture is a standalone `wasm32-wasip2` crate under
    /// `src-tauri/tests/fixtures/minimal-plugin/` whose guest
    /// implements every WIT export as a no-op. See that
    /// directory's README for rebuild instructions.
    const MINIMAL_PLUGIN_WASM: &[u8] =
        include_bytes!("../../tests/fixtures/minimal-plugin/minimal_plugin.wasm");

    /// Construct a bare `WasmRuntime` for tests. Uses a
    /// discarding `LogSender` and a fresh `SpanRegistry` so
    /// tests do not depend on a running logging task.
    fn test_runtime() -> Arc<WasmRuntime> {
        WasmRuntime::new(LogSender::test_sender(), Arc::new(SpanRegistry::new()))
            .expect("WasmRuntime::new should succeed with default config")
    }

    #[test]
    fn compile_then_instantiate_succeeds() {
        let runtime = test_runtime();
        runtime
            .compile("minimal", MINIMAL_PLUGIN_WASM)
            .expect("compile valid fixture");
        let instance = runtime.instantiate("minimal").expect("instantiate");
        instance.enable().expect("guest enable no-op");
    }

    #[test]
    fn compile_caches_component() {
        // No direct recompilation counter to assert on; we
        // verify the cache retains the entry after two
        // successful instantiates instead.
        let runtime = test_runtime();
        runtime
            .compile("minimal", MINIMAL_PLUGIN_WASM)
            .expect("compile");
        let _first = runtime.instantiate("minimal").expect("first instantiate");
        let _second = runtime.instantiate("minimal").expect("second instantiate");

        assert!(
            runtime
                .components
                .lock()
                .expect("cache lock")
                .contains_key("minimal")
        );
    }

    #[test]
    fn instantiate_without_compile_errors() {
        let runtime = test_runtime();
        let err = runtime
            .instantiate("never-compiled")
            .err()
            .expect("instantiate returns Err for unknown id");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("never-compiled") && msg.contains("compile"),
            "error should name the missing plugin and hint at `compile` (got: {msg})"
        );
    }

    #[test]
    fn compile_invalid_bytes_errors() {
        // Both checks matter: the return is an `Err` AND the
        // cache stays empty, so a subsequent `instantiate`
        // cannot silently succeed against a partial entry.
        let runtime = test_runtime();
        assert!(
            runtime
                .compile("garbage", b"not a wasm component at all")
                .is_err()
        );
        assert!(
            !runtime
                .components
                .lock()
                .expect("cache lock")
                .contains_key("garbage")
        );
        assert!(runtime.instantiate("garbage").is_err());
    }

    #[test]
    fn compile_replaces_existing_entry() {
        // Pins the "re-compile replaces" semantic so a future
        // refactor that accidentally rejects duplicate compiles
        // would trip this assertion.
        let runtime = test_runtime();
        runtime
            .compile("minimal", MINIMAL_PLUGIN_WASM)
            .expect("first compile");
        runtime
            .compile("minimal", MINIMAL_PLUGIN_WASM)
            .expect("second compile replaces");
        runtime.instantiate("minimal").expect("instantiate");
    }

    #[test]
    fn instances_are_independent() {
        // Two instances from the same cached Component must
        // have disjoint PluginStates: operating on one must
        // not disturb the other.
        let runtime = test_runtime();
        runtime
            .compile("minimal", MINIMAL_PLUGIN_WASM)
            .expect("compile");

        let first = runtime.instantiate("minimal").expect("first");
        let second = runtime.instantiate("minimal").expect("second");

        first.clear_sql_storage();
        second
            .enable()
            .expect("second instance untouched by operations on the first");
    }

    /// Bytes of the committed `opener-http-plugin` fixture.
    /// Exercises the `opener` and `http` host interfaces via
    /// `messaging::handle-message` dispatch.
    const OPENER_HTTP_PLUGIN_WASM: &[u8] =
        include_bytes!("../../tests/fixtures/opener-http-plugin/opener_http_plugin.wasm");

    /// Bytes of the committed `assets-plugin` fixture.
    /// Exercises the `assets` host interface via
    /// `messaging::handle-message` dispatch.
    const ASSETS_PLUGIN_WASM: &[u8] =
        include_bytes!("../../tests/fixtures/assets-plugin/assets_plugin.wasm");

    // =========================================================
    // Unit tests for opener/http pure functions
    //
    // These functions have no WASM or tokio dependency so the
    // tests are fast synchronous assertions.
    // =========================================================

    fn strs(ss: &[&str]) -> Vec<String> {
        ss.iter().map(|s| s.to_string()).collect()
    }

    // ---- check_opener_scheme --------------------------------

    #[test]
    fn opener_permitted_scheme_passes() {
        assert!(check_opener_scheme(&strs(&["https"]), "https://example.com").is_ok());
    }

    #[test]
    fn opener_forbidden_scheme_blocked() {
        let err = check_opener_scheme(&strs(&["https"]), "ftp://example.com").unwrap_err();
        match err {
            OpenerSchemeCheckError::SchemeNotPermitted(scheme) => assert_eq!(scheme, "ftp"),
            other => panic!("expected SchemeNotPermitted, got {other:?}"),
        }
    }

    #[test]
    fn opener_empty_allowlist_denies_everything() {
        assert!(check_opener_scheme(&[], "https://example.com").is_err());
    }

    #[test]
    fn opener_unparseable_url_returns_error() {
        assert!(check_opener_scheme(&strs(&["https"]), "not-a-url").is_err());
    }

    #[test]
    fn opener_scheme_check_is_case_insensitive() {
        // The `url` crate normalizes schemes to lowercase, so an
        // uppercase scheme in the URL still matches the allowlist.
        assert!(check_opener_scheme(&strs(&["https"]), "HTTPS://example.com").is_ok());
    }

    #[test]
    fn opener_multiple_schemes_second_matches() {
        assert!(check_opener_scheme(&strs(&["https", "mailto"]), "mailto:user@x.com").is_ok());
    }

    // ---- check_http_origin ----------------------------------

    #[test]
    fn http_exact_origin_match_passes() {
        assert!(
            check_http_origin(&strs(&["https://example.com"]), "https://example.com/path").is_ok()
        );
    }

    #[test]
    fn http_non_matching_origin_denied() {
        let err =
            check_http_origin(&strs(&["https://example.com"]), "https://other.com/x").unwrap_err();
        assert!(matches!(err, WasmHttpError::PermissionDenied(_)));
    }

    #[test]
    fn http_wildcard_allows_any_origin() {
        assert!(check_http_origin(&strs(&["*"]), "https://any-host.example/path").is_ok());
    }

    #[test]
    fn http_empty_origins_denies_everything() {
        assert!(check_http_origin(&[], "https://example.com").is_err());
    }

    #[test]
    fn http_non_default_port_included_in_origin() {
        assert!(
            check_http_origin(
                &strs(&["https://example.com:8443"]),
                "https://example.com:8443/resource"
            )
            .is_ok()
        );
    }

    #[test]
    fn http_default_port_stripped_from_origin() {
        // https:443 normalizes to the same origin as https (no port).
        assert!(
            check_http_origin(
                &strs(&["https://example.com"]),
                "https://example.com:443/resource"
            )
            .is_ok()
        );
    }

    #[test]
    fn http_wrong_port_is_different_origin() {
        let err = check_http_origin(
            &strs(&["https://example.com"]),
            "https://example.com:8080/x",
        )
        .unwrap_err();
        assert!(matches!(err, WasmHttpError::PermissionDenied(_)));
    }

    #[test]
    fn http_unparseable_url_returns_network_error() {
        let err = check_http_origin(&strs(&["https://example.com"]), "not-a-url").unwrap_err();
        assert!(matches!(err, WasmHttpError::Network(_)));
    }

    #[test]
    fn http_wildcard_short_circuits_before_url_parse() {
        // The wildcard check happens before `url::Url::parse`, so an
        // unparseable URL is still allowed when the list contains `"*"`.
        assert!(check_http_origin(&strs(&["*"]), "not-a-url").is_ok());
    }

    // ---- wit_method_to_reqwest ------------------------------

    #[test]
    fn wit_method_named_variants_map_correctly() {
        use bindings::torchsnap::plugin::http::HttpMethod;
        let cases = [
            (HttpMethod::Get, reqwest::Method::GET),
            (HttpMethod::Post, reqwest::Method::POST),
            (HttpMethod::Put, reqwest::Method::PUT),
            (HttpMethod::Patch, reqwest::Method::PATCH),
            (HttpMethod::Delete, reqwest::Method::DELETE),
            (HttpMethod::Head, reqwest::Method::HEAD),
        ];
        for (wit, expected) in cases {
            assert_eq!(wit_method_to_reqwest(wit).unwrap(), expected,);
        }
    }

    #[test]
    fn wit_method_other_valid_string() {
        use bindings::torchsnap::plugin::http::HttpMethod;
        let method = wit_method_to_reqwest(HttpMethod::Other("PROPFIND".into())).unwrap();
        assert_eq!(method.as_str(), "PROPFIND");
    }

    #[test]
    fn wit_method_other_invalid_string_returns_network_error() {
        use bindings::torchsnap::plugin::http::HttpMethod;
        let err = wit_method_to_reqwest(HttpMethod::Other("has space".into())).unwrap_err();
        assert!(matches!(err, WasmHttpError::Network(_)));
    }

    // ---- WasmHttpError → HttpError conversion ---------------

    #[test]
    fn wasm_http_error_conversion_all_variants() {
        use bindings::torchsnap::plugin::http::HttpError;
        let cases: Vec<(WasmHttpError, HttpError)> = vec![
            (
                WasmHttpError::PermissionDenied("origin".into()),
                HttpError::PermissionDenied("origin".into()),
            ),
            (WasmHttpError::Timeout, HttpError::Timeout),
            (
                WasmHttpError::Network("oops".into()),
                HttpError::Network("oops".into()),
            ),
        ];
        for (input, expected) in cases {
            let converted: HttpError = input.into();
            assert!(
                std::mem::discriminant(&converted) == std::mem::discriminant(&expected),
                "discriminant mismatch"
            );
        }
    }

    // ---- httpmock integration tests for http::fetch ---------
    //
    // These tests exercise the full `Http` builder pipeline in
    // `http::Host::fetch` — specifically that headers, body,
    // status, and response headers are forwarded correctly.
    // They don't need a live WASM instance; they call the
    // private pieces directly.

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_forwards_request_headers() {
        use httpmock::MockServer;
        use httpmock::prelude::*;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/test")
                .header("x-custom", "value123");
            then.status(200).body("ok");
        });

        let client = crate::network::Http::new();
        let request = bindings::torchsnap::plugin::http::HttpRequest {
            url: server.url("/test"),
            method: bindings::torchsnap::plugin::http::HttpMethod::Get,
            headers: vec![("x-custom".into(), "value123".into())],
            body: None,
            timeout_ms: None,
            max_body_size: None,
        };

        let mut state = PluginState {
            http_origins: vec!["*".into()],
            http_client: Some(Arc::new(client)),
            ..PluginState::default_for_test()
        };

        use bindings::torchsnap::plugin::http::Host;
        let resp = state.fetch(request).expect("fetch should succeed");
        assert_eq!(resp.status, 200);
        mock.assert();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_forwards_request_body() {
        use httpmock::MockServer;
        use httpmock::prelude::*;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/data").body("hello");
            then.status(201);
        });

        let client = crate::network::Http::new();
        let request = bindings::torchsnap::plugin::http::HttpRequest {
            url: server.url("/data"),
            method: bindings::torchsnap::plugin::http::HttpMethod::Post,
            headers: vec![],
            body: Some(b"hello".to_vec()),
            timeout_ms: None,
            max_body_size: None,
        };

        let mut state = PluginState {
            http_origins: vec!["*".into()],
            http_client: Some(Arc::new(client)),
            ..PluginState::default_for_test()
        };

        use bindings::torchsnap::plugin::http::Host;
        let resp = state.fetch(request).expect("fetch should succeed");
        assert_eq!(resp.status, 201);
        mock.assert();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_returns_response_status_and_headers() {
        use httpmock::MockServer;
        use httpmock::prelude::*;

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/head-test");
            then.status(404).header("x-resp-header", "present");
        });

        let client = crate::network::Http::new();
        let request = bindings::torchsnap::plugin::http::HttpRequest {
            url: server.url("/head-test"),
            method: bindings::torchsnap::plugin::http::HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout_ms: None,
            max_body_size: None,
        };

        let mut state = PluginState {
            http_origins: vec!["*".into()],
            http_client: Some(Arc::new(client)),
            ..PluginState::default_for_test()
        };

        use bindings::torchsnap::plugin::http::Host;
        let resp = state.fetch(request).expect("fetch should succeed");
        assert_eq!(resp.status, 404);
        let has_header = resp
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("x-resp-header") && v == "present");
        assert!(has_header, "expected x-resp-header in response headers");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_returns_response_body() {
        use httpmock::MockServer;
        use httpmock::prelude::*;

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/body");
            then.status(200).body(b"hello body".as_slice());
        });

        let client = crate::network::Http::new();
        let request = bindings::torchsnap::plugin::http::HttpRequest {
            url: server.url("/body"),
            method: bindings::torchsnap::plugin::http::HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout_ms: None,
            max_body_size: None,
        };

        let mut state = PluginState {
            http_origins: vec!["*".into()],
            http_client: Some(Arc::new(client)),
            ..PluginState::default_for_test()
        };

        use bindings::torchsnap::plugin::http::Host;
        let resp = state.fetch(request).expect("fetch should succeed");
        assert_eq!(resp.body, b"hello body");
    }

    // =========================================================
    // WASM integration tests for opener/http via fixture plugin
    //
    // These tests run the actual `opener-http-plugin` fixture
    // WASM and exercise `opener::open-url` and `http::fetch`
    // end-to-end through `handle_message` dispatch, with a mock
    // opener closure and a `httpmock` HTTP server respectively.
    // =========================================================

    fn compile_opener_http_fixture() -> (Arc<WasmRuntime>, WasmPluginInstance) {
        let runtime = test_runtime();
        runtime
            .compile("opener-http-plugin", OPENER_HTTP_PLUGIN_WASM)
            .expect("compile opener-http fixture");
        let instance = runtime
            .instantiate("opener-http-plugin")
            .expect("instantiate opener-http fixture");
        (runtime, instance)
    }

    #[test]
    fn opener_permitted_scheme_calls_writer() {
        let (_runtime, instance) = compile_opener_http_fixture();

        let called_url: std::sync::Arc<Mutex<Option<String>>> =
            std::sync::Arc::new(Mutex::new(None));
        let called_url_clone = called_url.clone();

        instance.set_opener_schemes(vec!["https".into()]);
        instance.set_opener_writer(Box::new(move |url: &str| {
            *called_url_clone.lock().expect("not poisoned") = Some(url.to_string());
            Ok(())
        }));

        instance.enable().expect("enable");

        let result = instance
            .handle_message("opener.open-url", "https://example.com")
            .expect("handle_message call succeeded")
            .expect("guest returned Ok");

        assert_eq!(result, "ok");
        assert_eq!(
            called_url.lock().expect("not poisoned").as_deref(),
            Some("https://example.com")
        );
    }

    #[test]
    fn opener_forbidden_scheme_returns_error() {
        let (_runtime, instance) = compile_opener_http_fixture();

        instance.set_opener_schemes(vec!["https".into()]);
        instance.set_opener_writer(Box::new(|_: &str| Ok(())));

        instance.enable().expect("enable");

        let result = instance
            .handle_message("opener.open-url", "ftp://example.com")
            .expect("handle_message call succeeded");

        let err = result.expect_err("guest should return Err for ftp://");
        assert!(
            err.contains("scheme not permitted: ftp"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn opener_writer_not_set_returns_error() {
        let (_runtime, instance) = compile_opener_http_fixture();

        instance.set_opener_schemes(vec!["https".into()]);
        // Intentionally omit set_opener_writer.

        instance.enable().expect("enable");

        let result = instance
            .handle_message("opener.open-url", "https://example.com")
            .expect("handle_message call succeeded");

        let err = result.expect_err("guest should return Err when writer missing");
        assert!(
            err.contains("opener not initialized"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn http_permitted_origin_proceeds() {
        use httpmock::MockServer;
        use httpmock::prelude::*;

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/ping");
            then.status(200).body("pong");
        });

        let (_runtime, instance) = compile_opener_http_fixture();
        instance.set_http_origins(vec!["*".into()]);
        instance.set_http_client(Arc::new(crate::network::Http::new()));

        instance.enable().expect("enable");

        let result = instance
            .handle_message("http.get", &server.url("/ping"))
            .expect("handle_message call succeeded")
            .expect("guest returned Ok");

        assert_eq!(result, "status:200");
    }

    #[test]
    fn http_blocked_origin_returns_permission_denied() {
        let (_runtime, instance) = compile_opener_http_fixture();
        // Empty origins = deny all.
        instance.set_http_origins(vec![]);
        instance.set_http_client(Arc::new(crate::network::Http::new()));

        instance.enable().expect("enable");

        let result = instance
            .handle_message("http.get", "https://example.com/test")
            .expect("handle_message call succeeded");

        let err = result.expect_err("guest should return Err for blocked origin");
        assert!(err.contains("PermissionDenied"), "unexpected error: {err}");
    }

    // =========================================================
    // Assets host import — unit tests
    //
    // These drive the `assets::Host` impl directly against a
    // `PluginState` built with `default_for_test` and a
    // `DirectorySource` stashed on `plugin_source`. They cover
    // each variant of `AssetsError` plus the happy paths for
    // both `read` and `exists`. The fixture-driven integration
    // tests (step 1f) exercise the same paths through a real
    // WASM guest call.
    // =========================================================

    fn make_plugin_source_dir() -> (
        tempfile::TempDir,
        Arc<dyn super::super::source::PluginSource + Send + Sync>,
    ) {
        use super::super::source::DirectorySource;

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();

        std::fs::write(
            root.join("manifest.toml"),
            r#"
[plugin]
id = "assets-unit-plugin"
name = "Assets Unit Plugin"
description = "Fixture for runtime unit tests"
version = "0.0.0"
wasm = "plugin.wasm"
icon = "heroicons:beaker"
"#,
        )
        .expect("write manifest");
        std::fs::write(root.join("plugin.wasm"), b"wasm").expect("write wasm");
        std::fs::write(root.join("greeting.txt"), b"hello from the fixture\n")
            .expect("write greeting");
        std::fs::create_dir_all(root.join("data")).expect("mkdir data");
        std::fs::write(root.join("data/payload.bin"), [0u8, 1, 2, 3, 255]).expect("write payload");

        let src: Arc<dyn super::super::source::PluginSource + Send + Sync> =
            Arc::new(DirectorySource::open(root).expect("open"));
        (dir, src)
    }

    fn state_with_source(
        src: Arc<dyn super::super::source::PluginSource + Send + Sync>,
    ) -> PluginState {
        PluginState {
            plugin_source: Some(src),
            ..PluginState::default_for_test()
        }
    }

    #[test]
    fn assets_read_returns_bytes_for_existing_file() {
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let bytes = state
            .read("greeting.txt".to_string())
            .expect("read succeeds");
        assert_eq!(bytes, b"hello from the fixture\n");
    }

    #[test]
    fn assets_read_preserves_binary_content() {
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let bytes = state
            .read("data/payload.bin".to_string())
            .expect("read succeeds");
        assert_eq!(bytes, [0u8, 1, 2, 3, 255]);
    }

    #[test]
    fn assets_read_returns_not_found_for_missing_file() {
        use bindings::torchsnap::plugin::assets::AssetsError;
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state
            .read("not-here.json".to_string())
            .expect_err("missing file returns Err");
        assert!(
            matches!(err, AssetsError::NotFound),
            "expected NotFound, got {err:?}"
        );
    }

    #[test]
    fn assets_read_rejects_traversal_path() {
        use bindings::torchsnap::plugin::assets::AssetsError;
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state
            .read("../../../etc/passwd".to_string())
            .expect_err("traversal rejected");
        match err {
            AssetsError::InvalidPath(msg) => assert!(
                msg.contains("escapes"),
                "InvalidPath message should mention escape (got: {msg})"
            ),
            other => panic!("expected InvalidPath, got {other:?}"),
        }
    }

    #[test]
    fn assets_read_rejects_absolute_path() {
        use bindings::torchsnap::plugin::assets::AssetsError;
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state
            .read("/etc/passwd".to_string())
            .expect_err("absolute rejected");
        match err {
            AssetsError::InvalidPath(msg) => assert!(
                msg.contains("relative"),
                "InvalidPath message should mention relative (got: {msg})"
            ),
            other => panic!("expected InvalidPath, got {other:?}"),
        }
    }

    #[test]
    fn assets_read_rejects_empty_path() {
        use bindings::torchsnap::plugin::assets::AssetsError;
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state.read("".to_string()).expect_err("empty rejected");
        assert!(matches!(err, AssetsError::InvalidPath(_)));
    }

    #[test]
    fn assets_read_rejects_nul_byte_path() {
        use bindings::torchsnap::plugin::assets::AssetsError;
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state
            .read("data\0hidden".to_string())
            .expect_err("NUL rejected");
        assert!(matches!(err, AssetsError::InvalidPath(_)));
    }

    #[test]
    fn assets_read_without_plugin_source_returns_io_error() {
        use bindings::torchsnap::plugin::assets::AssetsError;
        use bindings::torchsnap::plugin::assets::Host;

        // Default state has `plugin_source: None` — mirrors
        // calling an asset import outside an enable lifetime.
        let mut state = PluginState::default_for_test();
        let err = state
            .read("greeting.txt".to_string())
            .expect_err("uninitialized returns Err");
        match err {
            AssetsError::IoError(msg) => assert!(
                msg.contains("not initialized"),
                "expected `not initialized` message, got: {msg}"
            ),
            other => panic!("expected IoError, got {other:?}"),
        }
    }

    #[test]
    fn assets_exists_returns_true_when_present() {
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        assert!(
            state
                .exists("greeting.txt".to_string())
                .expect("probe succeeds")
        );
    }

    #[test]
    fn assets_exists_returns_false_when_absent() {
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        assert!(
            !state
                .exists("not-here.json".to_string())
                .expect("probe succeeds")
        );
    }

    #[test]
    fn assets_exists_rejects_traversal_path() {
        use bindings::torchsnap::plugin::assets::AssetsError;
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state
            .exists("../../../etc/passwd".to_string())
            .expect_err("traversal rejected");
        assert!(matches!(err, AssetsError::InvalidPath(_)));
    }

    #[test]
    fn assets_exists_without_plugin_source_returns_io_error() {
        use bindings::torchsnap::plugin::assets::AssetsError;
        use bindings::torchsnap::plugin::assets::Host;

        let mut state = PluginState::default_for_test();
        let err = state
            .exists("greeting.txt".to_string())
            .expect_err("uninitialized returns Err");
        assert!(matches!(err, AssetsError::IoError(_)));
    }

    #[test]
    fn assets_into_io_error_includes_full_chain() {
        // `anyhow::Error::chain` — make sure the `{e:#}`
        // formatting captures the `.context()` prefix so
        // host logs stay diagnostic.
        let err = anyhow::anyhow!("root cause").context("while doing X");
        let mapped = into_assets_io_error(err);
        match mapped {
            bindings::torchsnap::plugin::assets::AssetsError::IoError(msg) => {
                assert!(msg.contains("while doing X"), "missing context: {msg}");
                assert!(msg.contains("root cause"), "missing root: {msg}");
            }
            other => panic!("expected IoError, got {other:?}"),
        }
    }

    // =========================================================
    // WASM integration tests for assets via fixture plugin
    //
    // The assets fixture directory contains `greeting.txt` at
    // the root and `data/payload.bin` nested. These tests
    // compile the fixture, wire it to a `DirectorySource`
    // pointing at the fixture directory, and exercise
    // `assets::read` / `assets::exists` end-to-end through
    // `handle_message` dispatch.
    // =========================================================

    fn compile_assets_fixture() -> (Arc<WasmRuntime>, WasmPluginInstance) {
        let runtime = test_runtime();
        runtime
            .compile("assets-plugin", ASSETS_PLUGIN_WASM)
            .expect("compile assets fixture");
        let instance = runtime
            .instantiate("assets-plugin")
            .expect("instantiate assets fixture");
        (runtime, instance)
    }

    /// Build a `PluginSource` pointing at the committed
    /// assets-plugin fixture directory. This mirrors what
    /// the bridge does in production: `DirectorySource::open`
    /// on the plugin root, wrapped in an `Arc`, then stashed
    /// on the instance via `set_plugin_source`.
    fn assets_fixture_source() -> Arc<dyn super::super::source::PluginSource + Send + Sync> {
        use super::super::source::DirectorySource;
        let fixture_root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/assets-plugin");
        Arc::new(DirectorySource::open(fixture_root).expect("open assets fixture"))
    }

    #[test]
    fn wasm_assets_read_returns_bundled_file_contents() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "greeting.txt")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");

        assert!(
            result.contains("Hello from the assets fixture"),
            "unexpected body: {result}"
        );
    }

    #[test]
    fn wasm_assets_read_binary_preserves_byte_count() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        // `data/payload.bin` was written with a known
        // length; verify the full byte count crosses the
        // WIT boundary without truncation.
        let result = instance
            .handle_message("assets.read-len", "data/payload.bin")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");

        let expected_len = std::fs::metadata(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/assets-plugin/data/payload.bin"),
        )
        .expect("stat fixture")
        .len();

        assert_eq!(result, format!("len:{expected_len}"));
    }

    #[test]
    fn wasm_assets_exists_true_for_bundled_file() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.exists", "greeting.txt")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");
        assert_eq!(result, "true");
    }

    #[test]
    fn wasm_assets_exists_false_for_missing_file() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.exists", "not-here.txt")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");
        assert_eq!(result, "false");
    }

    #[test]
    fn wasm_assets_exists_false_for_nested_missing_file() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.exists", "data/not-here.txt")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");
        assert_eq!(result, "false");
    }

    #[test]
    fn wasm_assets_read_rejects_traversal() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "../../../etc/passwd")
            .expect("dispatch succeeded");
        let err = result.expect_err("traversal must error");
        assert!(
            err.contains("InvalidPath"),
            "expected InvalidPath variant in guest debug output, got: {err}"
        );
    }

    #[test]
    fn wasm_assets_read_rejects_absolute_path() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "/etc/passwd")
            .expect("dispatch succeeded");
        let err = result.expect_err("absolute must error");
        assert!(
            err.contains("InvalidPath"),
            "expected InvalidPath, got: {err}"
        );
    }

    #[test]
    fn wasm_assets_read_returns_not_found_for_missing_file() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "not-present.json")
            .expect("dispatch succeeded");
        let err = result.expect_err("missing must error");
        assert!(
            err.contains("NotFound"),
            "expected NotFound variant in guest debug output, got: {err}"
        );
    }

    #[test]
    fn wasm_assets_read_succeeds_on_nested_path() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "data/payload.bin")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");
        assert!(
            result.contains("nested binary data for tests"),
            "unexpected body: {result}"
        );
    }

    #[test]
    fn wasm_assets_calls_without_plugin_source_return_io_error() {
        // Skip `set_plugin_source` — same shape the bridge
        // would produce if it forgot to stash the source
        // on enable. The host returns `IoError` and the
        // guest bubbles the debug form up to our caller.
        let (_runtime, instance) = compile_assets_fixture();
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "greeting.txt")
            .expect("dispatch succeeded");
        let err = result.expect_err("uninit must error");
        assert!(err.contains("IoError"), "expected IoError, got: {err}");
    }
}
