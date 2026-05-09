// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};

// =========================================================
// Storage configuration
//
// Gadgets opt into per-gadget storage by declaring a
// `[storage]` table in their manifest. The host materializes
// the requested backends on first use — gadgets that never
// touch their storage never get a database file on disk.
// =========================================================

/// `[storage]` block. Future expansion can add `[storage.kv]`,
/// `[storage.files]`, etc. without breaking existing manifests.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageDef {
    /// `[storage.sql]` — per-gadget SQLite database.
    pub sql: Option<SqlStorageDef>,
}

/// `[storage.sql]` block.
///
/// Migrations are declared as a list of file paths relative
/// to the gadget root. The host reads the file contents via
/// `GadgetSource::read_file` at gadget load time and applies
/// them during `enable()` before the guest runs.
///
/// Single source of truth: the `.sql` files. Gadget tests can
/// `include_str!` the same files the manifest references —
/// no duplication, no drift.
///
/// # Domain conversions
///
/// `From<SqlStorageDef> for SqlStorageConfig` carries migration
/// file paths into the domain type. `From<SqlStorageDef> for
/// CapRequest` wraps that into a `CapRequest::SqlStorage`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SqlStorageDef {
    /// Ordered list of migration file paths. Each path is
    /// relative to the gadget root and should resolve to a
    /// `.sql` text file inside the gadget's directory or
    /// archive.
    #[serde(default)]
    pub migrations: Vec<String>,
}

// ─── Domain conversions ──────────────────────────────────

impl From<SqlStorageDef> for crate::caps::SqlStorageConfig {
    fn from(def: SqlStorageDef) -> Self {
        Self {
            migrations: def.migrations,
        }
    }
}

impl From<SqlStorageDef> for crate::caps::CapRequest {
    fn from(def: SqlStorageDef) -> Self {
        Self::SqlStorage {
            config: def.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::{CapRequest, SqlStorageConfig};

    #[test]
    fn into_sql_storage_config_maps_migrations() {
        let def = SqlStorageDef {
            migrations: vec!["001_init.sql".into(), "002_add_index.sql".into()],
        };
        let config: SqlStorageConfig = def.into();
        assert_eq!(config.migrations, vec!["001_init.sql", "002_add_index.sql"]);
    }

    #[test]
    fn into_sql_storage_config_empty_migrations() {
        let def = SqlStorageDef {
            migrations: vec![],
        };
        let config: SqlStorageConfig = def.into();
        assert!(config.migrations.is_empty());
    }

    #[test]
    fn into_cap_request_produces_sql_storage_variant() {
        let def = SqlStorageDef {
            migrations: vec!["001_init.sql".into()],
        };
        let req: CapRequest = def.into();
        match req {
            CapRequest::SqlStorage { config } => {
                assert_eq!(config.migrations, vec!["001_init.sql"]);
            }
            _ => panic!("expected SqlStorage variant"),
        }
    }
}
