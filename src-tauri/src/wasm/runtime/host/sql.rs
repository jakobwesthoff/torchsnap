// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// SQL host import
//
// The bridge materializes the per-gadget database during
// caps construction in `enable()` before the guest runs.
// `sql::connection()` hands out lightweight handles backed
// by the same `Arc<SqlStorage>`. Migration strings are
// pre-loaded by the bridge at construction time from the
// manifest's `[storage.sql] migrations = [...]` list.
// =========================================================

use std::path::PathBuf;
use std::sync::Arc;

use wasmtime::component::Resource;

use crate::storage::{SqlStorage, SqlValue as HostSqlValue};
use crate::wasm::bindings;

use super::super::GadgetState;

/// SQL storage state. `config` is set once at bridge
/// construction from the manifest; `storage` and
/// `handle_reps` track per-enable-cycle runtime state.
pub(crate) struct SqlState {
    pub(crate) config: SqlConfig,
    pub(crate) storage: Option<Arc<SqlStorage>>,
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

/// Whether and how the gadget's SQL storage is configured.
#[derive(Clone)]
pub enum SqlConfig {
    None,
    Configured {
        db_path: PathBuf,
        migrations: Arc<Vec<String>>,
    },
}

/// Internal entry stored in the wasmtime `ResourceTable`.
pub struct SqlHandleEntry {
    storage: Arc<SqlStorage>,
}

impl bindings::torchsnap::gadget::sql::Host for GadgetState {
    fn connection(&mut self) -> Resource<SqlHandleEntry> {
        // Access caps and wasi_table through disjoint field
        // borrows. The borrow checker allows this because
        // `self.caps` and `self.wasi_table` are separate
        // fields on GadgetState.
        let caps = self
            .caps
            .as_mut()
            .expect("sql::connection() called outside enable lifetime");
        let storage = caps.sql.storage.as_ref().expect(
            "sql::connection() called but no SQL storage is initialized — \
                     declare [storage.sql] in manifest.toml",
        );

        let entry = SqlHandleEntry {
            storage: Arc::clone(storage),
        };
        let handle = self
            .wasi_table
            .push(entry)
            .expect("allocate SQL handle in resource table");

        // Re-borrow caps after the wasi_table borrow is done.
        let caps = self
            .caps
            .as_mut()
            .expect("caps still present after push");
        caps.sql.handle_reps.push(handle.rep());

        handle
    }
}

impl bindings::torchsnap::gadget::sql::HostSqlHandle for GadgetState {
    fn execute(
        &mut self,
        handle: Resource<SqlHandleEntry>,
        sql: String,
        params: Vec<bindings::torchsnap::gadget::sql::SqlValue>,
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
        params: Vec<bindings::torchsnap::gadget::sql::SqlValue>,
    ) -> Result<Vec<Vec<bindings::torchsnap::gadget::sql::SqlValue>>, String> {
        let entry = self
            .wasi_table
            .get(&handle)
            .map_err(|e| format!("resolve SQL handle: {e}"))?;
        let native_params: Vec<HostSqlValue> = params.into_iter().map(Into::into).collect();

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
        let rep = handle.rep();
        if let Some(caps) = self.caps.as_mut() {
            caps.sql.handle_reps.retain(|&r| r != rep);
        }

        self.wasi_table.delete(handle)?;
        Ok(())
    }
}

// ---------------------------------------------------------
// SqlValue ↔ host SqlValue
// ---------------------------------------------------------

impl From<bindings::torchsnap::gadget::sql::SqlValue> for HostSqlValue {
    fn from(v: bindings::torchsnap::gadget::sql::SqlValue) -> Self {
        use bindings::torchsnap::gadget::sql::SqlValue as Wit;
        match v {
            Wit::Null => HostSqlValue::Null,
            Wit::Integer(i) => HostSqlValue::Integer(i),
            Wit::Real(f) => HostSqlValue::Real(f),
            Wit::Text(s) => HostSqlValue::Text(s),
            Wit::Blob(b) => HostSqlValue::Blob(b),
        }
    }
}

impl From<HostSqlValue> for bindings::torchsnap::gadget::sql::SqlValue {
    fn from(v: HostSqlValue) -> Self {
        use bindings::torchsnap::gadget::sql::SqlValue as Wit;
        match v {
            HostSqlValue::Null => Wit::Null,
            HostSqlValue::Integer(i) => Wit::Integer(i),
            HostSqlValue::Real(f) => Wit::Real(f),
            HostSqlValue::Text(s) => Wit::Text(s),
            HostSqlValue::Blob(b) => Wit::Blob(b),
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
