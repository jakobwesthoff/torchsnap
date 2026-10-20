# 65. Confine SQL storage connections to their own database

Date: 2026-10-20

## Status

Accepted

Amends [31. WASM plugin SQL storage API](0031-wasm-plugin-sql-storage-api.md) (gadget SQL no longer has full SQL access)

## Context

ADR 18 requires that gadgets cannot read or modify each other's data,
and `[storage.sql]` grants a gadget its own database file only. ADR 31
gives gadgets full SQL access: the host runs the SQL string a gadget
sends unchanged on the gadget's connection, and a gadget's migrations
are SQL from its archive too.

SQLite can open further files from SQL. `ATTACH DATABASE '<path>'`
gives read and write access to any SQLite file the user can access
(other gadgets' databases, Torchsnap's own stores, other applications'
databases) and creates a new file at a path that does not exist.
`VACUUM INTO '<path>'` writes a copy of the database to a new path.
Both open the other file through an attach: SQLite runs
`ATTACH '<path>'` internally for `VACUUM INTO`, and `ATTACH ''` (a
temporary database) for plain `VACUUM`.

## Decision

Every connection that `SqlStorage::open` creates is confined to its own
database file, before any migration runs:

- A SQLite authorizer denies every attach and every detach. It matches
  the raw action codes `SQLITE_ATTACH` and `SQLITE_DETACH`, since
  rusqlite reports an attach whose file name is not a string literal
  (a bound parameter, an expression) as an unknown action.
- `SQLITE_LIMIT_ATTACHED` is 0, so no database can be attached even
  without the authorizer.
- The authorizer also denies the pragmas `temp_store_directory`,
  `data_store_directory`, `soft_heap_limit` and `hard_heap_limit`,
  matched case-insensitively. They set state shared by every
  connection in the process: the directory for every connection's
  temporary files, and memory limits whose low value makes every
  connection fail.

A denied statement fails to prepare and returns an error to the caller;
it does not run, and the connection and an open transaction stay usable.

The confinement applies to the host's own stores too (frecency, website
metadata cache, clipboard history), which open through the same
constructor and use neither `ATTACH` nor `VACUUM`.

Plain `VACUUM` is not available on these connections.

## Consequences

- Gadget SQL and migrations cannot attach, detach or `VACUUM INTO`,
  nor read or set the four process-wide pragmas; such a statement
  returns an authorization error. A migration that does fails the
  gadget's enable.
- Gadget and host databases cannot be vacuumed. `auto_vacuum` cannot be
  turned on for them either: `SqlStorage::open` sets `journal_mode =
  wal` before migrations run, which writes the database header with
  auto-vacuum off, and SQLite changes that setting only through a
  `VACUUM`. Freed pages are reused for later writes; the files do not
  shrink.
