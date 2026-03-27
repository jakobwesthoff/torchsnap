// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Storage Infrastructure
//
// Generic storage primitives shared across plugins. File-
// based blob storage uses a sharded directory layout; SQL
// storage (added later) provides structured per-plugin
// databases.
// =========================================================

mod file_storage;
mod sql_storage;

pub use file_storage::{FileStorage, StorageKey};
// TODO: `SqlRow` is not yet used outside this module — re-export once needed.
pub use sql_storage::{SqlStorage, SqlValue};
