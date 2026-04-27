// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use wasmtime::component::Resource;

use crate::storage::{SqlStorage, SqlValue as HostSqlValue};
use crate::wasm::bindings;

use super::super::{PluginState, WasmPluginInstance};

/// SQL storage state. `config` is set once at bridge
/// construction from the manifest; `storage` and
/// `handle_reps` track per-enable-cycle runtime state.
pub(crate) struct SqlState {
    /// Storage configuration materialized from the manifest's
    /// `[storage.sql]` block. `SqlConfig::None` when the
    /// plugin declares no SQL storage.
    pub(crate) config: SqlConfig,
    /// The materialized storage handle. The bridge's
    /// `enable()` opens the database and stashes it here;
    /// `sql::connection()` hands out resource handles backed
    /// by the same `Arc<SqlStorage>`.
    pub(crate) storage: Option<Arc<SqlStorage>>,
    /// Resource reps for every `SqlHandleEntry` currently
    /// live in `wasi_table`. Pushed on `sql::connection()`,
    /// removed on the WIT-driven `drop()` of an individual
    /// handle, and drained-and-deleted on `disable()` so
    /// that release cleanly tears down every outstanding
    /// `Arc<SqlStorage>` reference.
    pub(crate) handle_reps: Vec<u32>,
}

impl Default for SqlState {
    fn default() -> Self {
        Self {
            config: SqlConfig::None,
            storage: None,
            handle_reps: Vec::new(),
        }
    }
}

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
/// `SqlState::storage` until the plugin is disabled.
pub struct SqlHandleEntry {
    storage: Arc<SqlStorage>,
}

impl bindings::torchsnap::plugin::sql::Host for PluginState {
    fn connection(&mut self) -> Resource<SqlHandleEntry> {
        // The bridge opens the database before the guest's
        // enable() runs, so sql.storage is always populated
        // for plugins that declared [storage.sql]. A missing
        // storage here means the plugin called connection()
        // without declaring storage — that's a bug, so we
        // trap rather than returning a Result the guest would
        // have to handle on every call.
        let storage = self.sql.storage.as_ref().expect(
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
        self.sql.handle_reps.push(handle.rep());

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
        self.sql.handle_reps.retain(|&r| r != rep);

        // Removing the entry drops just this resource's
        // clone of the master Arc. The underlying
        // SqlStorage stays alive on PluginState's
        // `sql.storage` field until the plugin is
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

impl WasmPluginInstance {
    /// Install the SQL storage configuration on the store
    /// data. Called once by the bridge at construction time
    /// (before any guest call) — the migration strings have
    /// already been read from the plugin source.
    pub fn set_sql_config(&self, config: SqlConfig) {
        self.with_state_mut(|state| state.sql.config = config);
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

        let (db_path, migrations) = match &data.sql.config {
            SqlConfig::None => return Ok(()),
            SqlConfig::Configured {
                db_path,
                migrations,
            } => (db_path.clone(), Arc::clone(migrations)),
        };

        if data.sql.storage.is_none() {
            let migration_strs: Vec<&str> = migrations.iter().map(String::as_str).collect();
            let storage = SqlStorage::open(db_path, &migration_strs).context("open SQL storage")?;
            data.sql.storage = Some(Arc::new(storage));
        }

        Ok(())
    }

    /// Drop every outstanding SQL handle and the cached
    /// `Arc<SqlStorage>` so the rusqlite `Connection` is
    /// closed at disable. Without this drain, a guest that
    /// neglected to release every handle would keep the
    /// connection alive until the whole `WasmPluginInstance`
    /// is dropped.
    pub fn clear_sql_storage(&self) {
        let mut store = self.store.lock().expect("store not poisoned");
        let data = store.data_mut();
        let reps = std::mem::take(&mut data.sql.handle_reps);
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
        data.sql.storage = None;
    }
}
