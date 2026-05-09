// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// SqlStorageCap
//
// Thin capability wrapping a per-gadget SqlStorage instance.
// The binary gate is encoded by the Option on WasmGadgetCaps.
// Resource handle lifecycle (handle_reps, ResourceTable) is
// managed by the WASM bridge, not by this cap.
// =========================================================

use std::sync::Arc;

use crate::storage::SqlStorage;

// =========================================================
// SqlStorageConfig
// =========================================================

/// Configuration for constructing a `SqlStorageCap`. Contains
/// the migration SQL content (already read from the gadget
/// source). The `db_path` is a provisioning concern computed
/// by the host from the gadget ID and app data directory.
pub struct SqlStorageConfig {
    pub migrations: Vec<String>,
}

// =========================================================
// SqlStorageCap
// =========================================================

pub struct SqlStorageCap {
    storage: Arc<SqlStorage>,
}

impl SqlStorageCap {
    pub fn new(storage: Arc<SqlStorage>) -> Self {
        Self { storage }
    }

    pub fn storage(&self) -> &Arc<SqlStorage> {
        &self.storage
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ─── SqlStorageConfig ─────────────────────────────────

    #[test]
    fn sql_storage_config_stores_migrations() {
        let config = SqlStorageConfig {
            migrations: vec!["CREATE TABLE t (id INTEGER);".into()],
        };
        assert_eq!(config.migrations.len(), 1);
        assert_eq!(config.migrations[0], "CREATE TABLE t (id INTEGER);");
    }

    #[test]
    fn sql_storage_config_empty_migrations() {
        let config = SqlStorageConfig {
            migrations: vec![],
        };
        assert!(config.migrations.is_empty());
    }

    // ─── SqlStorageCap ──────────────────────────────────

    #[test]
    fn storage_accessor_returns_inner() {
        let tmp = TempDir::new().expect("temp dir");
        let db_path = tmp.path().join("test.sqlite3");
        let storage = Arc::new(
            SqlStorage::open(db_path, &["CREATE TABLE _init (id INTEGER);"]).expect("open test db"),
        );
        let cap = SqlStorageCap::new(Arc::clone(&storage));
        assert!(Arc::ptr_eq(cap.storage(), &storage));
    }

    #[test]
    fn execute_through_cap() {
        let tmp = TempDir::new().expect("temp dir");
        let db_path = tmp.path().join("test.sqlite3");
        let storage = Arc::new(
            SqlStorage::open(db_path, &["CREATE TABLE t (id INTEGER PRIMARY KEY);"]).expect("open"),
        );
        let cap = SqlStorageCap::new(storage);
        let affected = cap
            .storage()
            .execute("INSERT INTO t (id) VALUES (?1)", &[crate::storage::SqlValue::Integer(42)])
            .expect("insert");
        assert_eq!(affected, 1);
    }

    #[test]
    fn query_through_cap() {
        let tmp = TempDir::new().expect("temp dir");
        let db_path = tmp.path().join("test.sqlite3");
        let storage = Arc::new(
            SqlStorage::open(
                db_path,
                &["CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT);"],
            )
            .expect("open"),
        );
        let cap = SqlStorageCap::new(storage);
        cap.storage()
            .execute(
                "INSERT INTO t (id, name) VALUES (?1, ?2)",
                &[
                    crate::storage::SqlValue::Integer(1),
                    crate::storage::SqlValue::Text("hello".into()),
                ],
            )
            .expect("insert");

        let rows = cap
            .storage()
            .query_map("SELECT id, name FROM t", &[], |row| {
                Ok(row.columns().to_vec())
            })
            .expect("query");
        assert_eq!(rows.len(), 1);
    }
}
