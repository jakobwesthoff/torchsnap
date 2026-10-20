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

/// Whether and how the gadget's SQL storage is configured.
/// Lives on the bridge across disable/re-enable cycles.
#[derive(Clone)]
pub enum SqlConfig {
    None,
    Configured {
        // Read back only by `WasmGadgetBridge` tests via
        // `sql_config_for_tests()`; the live `SqlStorage` handle lives on
        // `SqlStorageCap` instead.
        #[allow(dead_code)]
        db_path: PathBuf,
        #[allow(dead_code)]
        migrations: Arc<Vec<String>>,
    },
}

/// Internal entry stored in the wasmtime `ResourceTable`.
pub struct SqlHandleEntry {
    storage: Arc<SqlStorage>,
}

impl bindings::torchsnap::gadget::sql_storage::Host for GadgetState {
    fn connection(&mut self) -> Resource<SqlHandleEntry> {
        let sql_cap = self.caps.sql_storage.as_ref().expect(
            "sql::connection() called but no SQL storage is initialized — \
             declare [storage.sql] in manifest.toml",
        );

        let entry = SqlHandleEntry {
            storage: Arc::clone(sql_cap.storage()),
        };
        let handle = self
            .wasi_table
            .push(entry)
            .expect("allocate SQL handle in resource table");

        self.sql_handle_reps.push(handle.rep());

        handle
    }
}

impl bindings::torchsnap::gadget::sql_storage::HostSqlHandle for GadgetState {
    fn execute(
        &mut self,
        handle: Resource<SqlHandleEntry>,
        sql: String,
        params: Vec<bindings::torchsnap::gadget::sql_storage::SqlValue>,
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
        params: Vec<bindings::torchsnap::gadget::sql_storage::SqlValue>,
    ) -> Result<Vec<Vec<bindings::torchsnap::gadget::sql_storage::SqlValue>>, String> {
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
        self.sql_handle_reps.retain(|&r| r != rep);
        self.wasi_table.delete(handle)?;
        Ok(())
    }
}

// ---------------------------------------------------------
// SqlValue ↔ host SqlValue
// ---------------------------------------------------------

impl From<bindings::torchsnap::gadget::sql_storage::SqlValue> for HostSqlValue {
    fn from(v: bindings::torchsnap::gadget::sql_storage::SqlValue) -> Self {
        use bindings::torchsnap::gadget::sql_storage::SqlValue as Wit;
        match v {
            Wit::Null => HostSqlValue::Null,
            Wit::Integer(i) => HostSqlValue::Integer(i),
            Wit::Real(f) => HostSqlValue::Real(f),
            Wit::Text(s) => HostSqlValue::Text(s),
            Wit::Blob(b) => HostSqlValue::Blob(b),
        }
    }
}

impl From<HostSqlValue> for bindings::torchsnap::gadget::sql_storage::SqlValue {
    fn from(v: HostSqlValue) -> Self {
        use bindings::torchsnap::gadget::sql_storage::SqlValue as Wit;
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

#[cfg(test)]
mod tests {
    use super::*;
    use bindings::torchsnap::gadget::sql_storage::{Host, HostSqlHandle, SqlValue as Wit};

    use crate::caps::{ProvisionedCaps, SqlStorageCap};

    /// A gadget state whose only capability is SQL storage, the
    /// one capability these host imports read.
    fn state_with_sql_storage(storage: Arc<SqlStorage>) -> GadgetState {
        GadgetState {
            caps: Arc::new(ProvisionedCaps {
                opener: None,
                http: None,
                filesystem: None,
                command: None,
                clipboard: None,
                sql_storage: Some(Arc::new(SqlStorageCap::new(storage))),
                website_metadata: None,
                icon_cache: None,
                settings: None,
                frecency: None,
                path_resolver: None,
            }),
            ..GadgetState::default_for_test()
        }
    }

    fn items_storage(dir: &tempfile::TempDir) -> Arc<SqlStorage> {
        Arc::new(
            SqlStorage::open(
                dir.path().join("storage.sqlite3"),
                &["CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);"],
            )
            .expect("open database"),
        )
    }

    /// `Resource` is consumed by every call, so tests keep the rep
    /// and hand each call a borrow of the same table entry.
    fn handle(rep: u32) -> Resource<SqlHandleEntry> {
        Resource::new_borrow(rep)
    }

    fn insert(state: &mut GadgetState, rep: u32, name: &str) {
        state
            .execute(
                handle(rep),
                "INSERT INTO items (name) VALUES (?)".to_string(),
                vec![Wit::Text(name.to_string())],
            )
            .expect("insert");
    }

    fn names(state: &mut GadgetState, rep: u32) -> Vec<String> {
        state
            .query(
                handle(rep),
                "SELECT name FROM items ORDER BY name".to_string(),
                vec![],
            )
            .expect("query")
            .into_iter()
            .map(|row| match row.as_slice() {
                [Wit::Text(name)] => name.clone(),
                other => panic!("unexpected row {other:?}"),
            })
            .collect()
    }

    fn attach_sql(dir: &tempfile::TempDir) -> String {
        format!(
            "ATTACH DATABASE '{}' AS other",
            dir.path().join("other.sqlite3").display()
        )
    }

    #[test]
    fn a_gadget_attach_returns_an_error_and_the_handle_keeps_working() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let mut state = state_with_sql_storage(items_storage(&dir));
        let rep = state.connection().rep();

        let err = state
            .execute(handle(rep), attach_sql(&dir), vec![])
            .expect_err("SQLite runs the ATTACH");
        assert!(err.contains("not authorized"), "unexpected error: {err}");
        assert!(!dir.path().join("other.sqlite3").exists());

        insert(&mut state, rep, "after");
        assert_eq!(names(&mut state, rep), vec!["after"]);
    }

    #[test]
    fn a_gadget_query_that_attaches_returns_an_error() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let mut state = state_with_sql_storage(items_storage(&dir));
        let rep = state.connection().rep();

        let err = state
            .query(handle(rep), attach_sql(&dir), vec![])
            .expect_err("SQLite runs the ATTACH");
        assert!(err.contains("not authorized"), "unexpected error: {err}");
        assert!(!dir.path().join("other.sqlite3").exists());
    }

    #[test]
    fn a_gadget_transaction_survives_a_denied_statement() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let mut state = state_with_sql_storage(items_storage(&dir));
        let rep = state.connection().rep();

        state
            .execute(handle(rep), "BEGIN".to_string(), vec![])
            .expect("begin");
        insert(&mut state, rep, "before");
        state
            .execute(handle(rep), attach_sql(&dir), vec![])
            .expect_err("SQLite runs the ATTACH");
        insert(&mut state, rep, "after");
        state
            .execute(handle(rep), "COMMIT".to_string(), vec![])
            .expect("commit");

        assert_eq!(names(&mut state, rep), vec!["after", "before"]);
    }
}
