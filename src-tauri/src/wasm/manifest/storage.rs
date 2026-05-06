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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SqlStorageDef {
    /// Ordered list of migration file paths. Each path is
    /// relative to the gadget root and should resolve to a
    /// `.sql` text file inside the gadget's directory or
    /// archive.
    #[serde(default)]
    pub migrations: Vec<String>,
}
