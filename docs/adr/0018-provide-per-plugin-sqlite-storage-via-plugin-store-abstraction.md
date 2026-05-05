# 18. Provide per-plugin storage via SqlStorage and FileStorage

Date: 2026-03-27

## Status

Accepted

## Context

Plugins need persistent storage. The clipboard manager needs to store
clipboard history entries (text, images, metadata). Future plugins will
have similar needs (file search index, frecency data, bookmarks). The
core app itself will need SQLite for cross-plugin concerns like
frecency ranking and selection statistics.

Current persistence is limited to `tauri-plugin-store` (a JSON file for
user settings) and a flat directory of WebP files for icon caching.
Neither is suitable for structured, queryable data at scale.

### Constraints

- **Isolation:** Plugins must not be able to read or modify each other's
  data.
- **WASM future:** All plugins are intended to become WASM modules. The
  storage API must use types that can cross the WASM boundary
  (serializable, no raw pointers or connection handles).
- **Queryability:** Simple key-value is insufficient — the clipboard
  manager needs filtered, sorted, paginated queries over history
  entries.
- **Migration support:** Plugin schemas evolve. Each plugin must be able
  to run its own migrations independently.

## Decision

Plugin storage is split into two independent primitives: `SqlStorage`
for structured/queryable data and `FileStorage` for binary blobs. Both
are scoped per plugin using the plugin ID (ADR 0017).

### Physical layout

```
<app_data_dir>/plugin-home/<plugin-id>/
├── sql/
│   └── storage.sqlite3         # SqlStorage (WASM plugin shape)
```

Directory resolved via Tauri's `app.path().app_data_dir()`. The
top-level split between `plugins/` (plugin *code* — archives and
dev directories, owned by the install/uninstall flow) and
`plugin-home/` (plugin *state* — host-managed, owned by the
runtime) is formalized in ADR 0035. Native plugins follow the
same layout with their own filename under `sql/` (e.g.
`sql/clipboard.sqlite3`, `sql/bangs.sqlite3`).

File-based blob storage lives alongside `sql/` as a reserved
sibling slot under `plugin-home/<plugin-id>/` (ADR 0019).

### Core app DB

`<app_data_dir>/torchsnap.db` is a separate database for cross-plugin
host concerns (frecency, ranking). Managed by the host, not exposed to
plugins.

### SqlStorage

Wraps a `rusqlite::Connection` behind a serializable API. The host
creates the directory, opens the connection with uniform settings (WAL
mode, busy timeout, etc.), and provides the `SqlStorage` to the plugin
— never a raw connection.

**API:** SQL prepared statements with serializable parameters and
results. The plugin sends SQL strings + `Vec<Value>` params, gets back
rows as serializable structures (e.g., `Vec<Map<String, Value>>`). This
is the full interface — query, insert, update, delete all go through
prepared statements.

This is the thinnest useful abstraction: it maps directly to what
`rusqlite` does internally, the types (`String` for SQL, `Vec<Value>`
for params, serializable rows for results) cross the WASM boundary
trivially, and plugins get the full power of SQL without a custom query
DSL.

**Engine:** `rusqlite` + `rusqlite_migration`.

**Migrations:** Each plugin provides its migrations via a trait method:

```rust
fn migrations(&self) -> Vec<Migration>;
```

The host runs migrations during `enable()` before handing the
`SqlStorage` to the plugin. This keeps migration execution centralized
and gives the host visibility into schema changes.

## Consequences

- Plugins get a thin SQL interface that maps directly to WASM host
  functions when the time comes.
- Each plugin's structured data is isolated in its own DB file.
  Removing a user-installed plugin means deleting its
  `plugin-home/<plugin-id>/` subtree (the uninstall command in
  ADR 0035 does exactly this).
- `rusqlite` and `rusqlite_migration` become new dependencies.
- The host is responsible for creating plugin data directories and
  opening connections during `enable()`.
- Binary data (images, files) is handled separately by `FileStorage`
  (ADR 0019), referenced by key in `SqlStorage`.
