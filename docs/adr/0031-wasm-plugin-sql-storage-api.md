# 31. WASM plugin SQL storage API

Date: 2026-04-08

## Status

Accepted

## Context

WASM plugins need persistent storage for the same reason native plugins
do — history rows, cached metadata, indices, anything where each search
or message handler can't recompute the answer from scratch. ADR 0018
already established the per-plugin SQLite abstraction (`SqlStorage` in
`src-tauri/src/storage/sql_storage.rs`) used by every native plugin
that needs storage. The calculator port (next task in the migration
plan) is the forcing function — its `calc_history` table, dedup-by-hash
inserts, recent-N-with-filter queries, and retention deletes are all
direct SQL.

The native side already gives plugins everything they need. The gap is
exposing the same `SqlStorage` shape to WASM plugins through the WIT
boundary without giving up the type safety, error handling, or
sandboxing of the underlying abstraction.

## Decision

Add a new `interface sql` host import to the WIT world. The interface
exposes a `sql-handle` resource (wasmtime component-model resource type)
with `execute` and `query` methods, plus a top-level `connection()`
function that returns a handle to the host-managed database.

```wit
interface sql {
  variant sql-value {
    null,
    integer(s64),
    real(f64),
    text(string),
    blob(list<u8>),
  }

  connection: func() -> sql-handle;

  resource sql-handle {
    execute: func(sql: string, params: list<sql-value>) -> result<u64, string>;
    query: func(sql: string, params: list<sql-value>)
        -> result<list<list<sql-value>>, string>;
  }
}
```

`sql-value` mirrors SQLite's five storage classes 1:1. The host's
internal `SqlValue::List` variant (used for `IN (?)` clause expansion
before binding) is intentionally **not** exposed — plugins expand
their own `IN` clauses on their side. This keeps the boundary minimal
and predictable.

### Migration declaration: file references in `manifest.toml`

Migrations are declared in `manifest.toml` as a list of file paths
relative to the plugin root:

```toml
[storage.sql]
migrations = [
    "migrations/001_init.sql",
]
```

Plugin layout:

```
plugins/template/
├── manifest.toml          # references migration files
├── migrations/
│   └── 001_init.sql       # plain SQL, syntax-highlighted, diffable
├── src/lib.rs
└── frontend/
```

Single source of truth: the `.sql` files. Plugin tests can
`include_str!` the same files the manifest references — no duplication,
no drift.

Inline strings in TOML (`migrations = ["""CREATE TABLE..."""]`) are
**not** supported. One rule: always file references.

The `[storage]` table is a wrapper namespace so `[storage.kv]` /
`[storage.files]` blocks can be added in the future without breaking
existing manifests.

### File creation timing: host-managed on `enable()`

The host reads the migration file contents at plugin **load** time
via `PluginSource::read_file` — failing fast on missing or
malformed migration files (treat them as a manifest authoring bug).
The actual database file at
`<app_data_dir>/plugins/<plugin-id>/storage.db` is created by the
bridge during `enable()`, before the guest's own `enable()` runs.
Plugins that never declare `[storage.sql]` get no file on disk.

`sql::connection()` returns a fresh handle pointing at the underlying
connection. The wasmtime store mutex serializes every guest call, so
no concurrent access is possible — sequential calls just allocate
another resource entry that backs onto the same `Arc<SqlStorage>`.

### Resource handle: real WIT resource, not opaque integer

The handle is a real WIT `resource sql-handle`, exposed as
`Resource<SqlHandleEntry>` in the wasmtime bindings. The bindgen
`with` option swaps the default placeholder type for our concrete
`SqlHandleEntry { storage: Arc<SqlStorage> }`:

```rust
wasmtime::component::bindgen!({
    path: "../wit",
    world: "plugin",
    with: {
        "torchsnap:plugin/sql.sql-handle": super::runtime::SqlHandleEntry,
    },
});
```

WIT resource drop semantics fire automatically when the plugin lets
its handle go out of scope, releasing the per-handle `ResourceTable`
entry. The master `Arc<SqlStorage>` lives on `PluginState` and is
cleared on `disable()`.

Alternative considered: opaque `u64` handle returned from
`connection()` and threaded through every call. Rejected because it
loses the automatic drop semantics and forces the plugin to remember
to call `close()`.

### Error surfacing: at bridge `enable()`

Storage-related errors — malformed SQL, filesystem permission,
migration apply failure — surface as a host-side `enable()` error
before the guest's own `enable()` runs. The guest never sees storage
initialization failures. Per-statement errors surface from `execute`
/ `query`. Note that **migration file reads happen at bridge
construction time** (before `enable()`), so a missing migration file
actually fails the plugin load with an `anyhow::Context` chain like
`read SQL migration file 'migrations/001_init.sql' → ... not found`.

### Transactions: deferred

The host's `SqlStorage` exposes `transaction()` for save-pointed
transactions, but the WIT interface doesn't expose it yet. Calculator's
writes are all single-statement or two-statement (SELECT-then-INSERT/
UPDATE), and the *current native* code doesn't wrap them in a
transaction either. Adding a transaction sub-resource later is
non-breaking.

### Multiple databases per plugin: no

One DB per plugin (`storage.db`). YAGNI for calculator and any plugin
on the roadmap. If a future plugin needs more, `connection` can grow a
name parameter and gate access to `storage-<name>.db` files inside the
plugin's data dir.

### Bridge wiring

`WasmPluginBridge::new` now takes the `PluginSource` and the host's
`app_data_dir` and reads the migration file contents at construction
time. It passes a `SqlConfig::Configured { db_path, migrations }` (or
`SqlConfig::None` if `[storage.sql]` is absent) into `PluginState`
via the new `WasmPluginInstance::set_sql_config` typed wrapper.

`bridge.disable()` calls `clear_sql_storage` to drop the cached
`Arc<SqlStorage>` so the database file isn't held open between enable
cycles.

`PluginState` gains:

- `sql_config: SqlConfig` — pre-loaded config from the bridge
- `sql_storage: Option<Arc<SqlStorage>>` — materialized by the
  bridge's `enable()` before the guest runs, used by
  `sql::connection()` for subsequent handle allocation

`bindings::torchsnap::plugin::sql::Host` and `HostSqlHandle` are
implemented for `PluginState` and pick up the new linker entries
automatically via `bindings::Plugin::add_to_linker(...)`.

## Alternatives considered

- **A KV-only API** (get/set/delete by string key) — rejected. The
  calculator's queries (most-recent-N with substring filter, dedup
  by content hash) have no efficient KV translation.
- **Document storage** (JSON blobs by ID) — same problem; no efficient
  range scans, no indexing.
- **A fixed schema baked into the host** — rejected as
  least-flexible. Each plugin would have to wedge its data into a
  one-size-fits-all schema.
- **Inline migration strings** in `manifest.toml` instead of file
  references — rejected. File references give plugin authors syntax
  highlighting, diffable history, and the ability to share the same
  files between the runtime and unit tests.
- **No `[storage.sql]` declaration required**, just open the database
  on demand with no migrations — rejected. Forcing the manifest
  declaration means missing schemas surface at plugin load time
  (clearer error path) and sets up the future for migration version
  pinning.

## Consequences

- WASM plugins get full SQL access with the same `SqlStorage`
  abstraction native plugins use. The calculator port can implement
  its history table 1:1 against this API.
- Adding a host import is automatically picked up by
  `bindings::Plugin::add_to_linker(...)` — no per-interface plumbing
  needed at the linker level.
- Plugins that never declare `[storage.sql]` pay no runtime cost (no
  file on disk, no in-memory state).
- The WIT resource model gives us automatic drop semantics —
  plugins that misuse the handle (drop it, then keep using it) get
  a clean error instead of a use-after-free.
- The migration file paths in the manifest are read at bridge
  construction time, which means a typo'd path fails plugin load.
  This is the desired behavior — better to refuse to load than to
  half-load and surprise the user later.
- Adding transactions later requires a new resource type
  (`sql-transaction`) wrapping the same connection. This is purely
  additive to the WIT world.
