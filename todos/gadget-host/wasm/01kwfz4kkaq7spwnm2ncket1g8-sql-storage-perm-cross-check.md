# Documented sql-storage/storage.sql coupling is not enforced

**Kind:** possible-bug
**Severity:** low
**Area:** src-tauri/src/wasm/manifest/permissions/mod.rs

## Problem
The `permissions.sql-storage` field documents a cross-field
requirement
(`src-tauri/src/wasm/manifest/permissions/mod.rs:76-79`):

```rust
/// `permissions.sql-storage` — opt-in for SQL storage
/// capability. Requires `[storage.sql]` to also be present.
#[serde(default, rename = "sql-storage")]
pub sql_storage: bool,
```

Neither `validate_permissions` (which only sees the
`PermissionsDef`) nor `Manifest::parse`
(`manifest/mod.rs:279-332`, which has both halves in scope)
checks this. A manifest with `sql-storage = true` and no
`[storage.sql]` table parses successfully; whatever happens next
is decided by the provisioning code, not the documented
contract.

## Impact
The failure surfaces late (at capability provisioning or first
guest `sql` call) with an error that does not point at the
manifest inconsistency. If provisioning happens to create an
empty database instead, the gadget runs with no schema and every
query fails.

## Suggested fix
Enforce in `Manifest::parse` after permission validation:
`sql_storage == true` requires `storage.sql` to be `Some`, with
an error naming both keys. Add the parse test. Also decide and
document the reverse case (`[storage.sql]` present without the
permission): legal-but-unreachable storage or a parse error.
