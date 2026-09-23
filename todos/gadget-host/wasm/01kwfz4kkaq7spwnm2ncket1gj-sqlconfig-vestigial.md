# Bridge SqlConfig carries a hardcoded empty migrations list

**Kind:** refactor
**Severity:** low
**Area:** src-tauri/src/wasm/bridge.rs

## Problem
`WasmGadgetBridge::new` builds its `SqlConfig` with an
always-empty migrations vector
(`src-tauri/src/wasm/bridge.rs:255-263`):

```rust
let sql_config = match manifest.storage.as_ref().and_then(|s| s.sql.as_ref()) {
    None => SqlConfig::None,
    Some(_) => SqlConfig::Configured {
        db_path: gadget_data.join("sql").join("storage.sqlite3"),
        migrations: Arc::new(vec![]),
    },
};
```

The real migration contents travel through
`cap_requests_from_manifest` → `CapRequest::SqlStorage`
(`bridge.rs:179-205`); the bridge-held `SqlConfig` is, per its
own comment (`:255-256`), only "used by test helpers that verify
SQL lifecycle" (`sql_config_for_tests`, `:780-783`). The
`migrations` field of `SqlConfig::Configured`
(`runtime/host/sql.rs:29-35`) is therefore dead data with a
misleading name: it always says "no migrations" even for gadgets
that declare some.

There is also a second copy of the db-path convention here:
`gadget_data.join("sql").join("storage.sqlite3")` must match
whatever path the caps provisioning uses when it actually opens
the database; if the two ever diverge, the tests validate a path
production never touches.

## Impact
Misleading structure for future readers; a test-only shadow of
production config that can silently diverge from the real one.

## Suggested fix
Either populate `SqlConfig` from the same data the cap request
uses (single source of truth, tests then assert reality) or
shrink it to what the tests actually need (`db_path` presence)
and drop the `migrations` field.
