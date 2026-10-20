---
title: "Gadget SQL can attach any SQLite file and write files anywhere through ATTACH and VACUUM INTO"
kind: bug
component: gadget-host
impact: critical
origin: review
tags: [security, privacy, capabilities]
---
# Gadget SQL can attach any SQLite file and write files anywhere through ATTACH and VACUUM INTO

A gadget with `[storage.sql]` runs its SQL verbatim on its own database connection, and nothing stops `ATTACH DATABASE '<path>'` or `VACUUM INTO '<path>'`. The gadget can then read and write any SQLite database the user can access (other gadgets' databases, Torchsnap's own stores, browser histories) and create new files at any path the user can write. Found on 2026-10-20 while planning the command confinement work; confirmed in the code, the file creation confirmed with `/usr/bin/sqlite3`.

## Problem

- `src-tauri/src/wasm/runtime/host/sql.rs` (`HostSqlHandle::execute` and `query`) passes the guest's SQL string unchanged to `SqlStorage::execute` and `SqlStorage::query_map`.
- `configure_connection` in `src-tauri/src/storage/sql_storage.rs` sets only pragmas (WAL, busy timeout, synchronous, foreign keys, cache size). It installs no authorizer and leaves `SQLITE_LIMIT_ATTACHED` at its default.
- `ATTACH` to an existing SQLite file gives full read and write access to it; `ATTACH` to a new path creates the file. `VACUUM INTO` writes a copy of the database to a new path. Attaching a file that is not a SQLite database fails, so arbitrary existing files cannot be overwritten this way.

## Impact

Under the binding-permissions threat model (decided 2026-10-20 in the command cwd and env todos), `[storage.sql]` is meant to grant the gadget's own database only. This breaks that bound for data: a gadget can read browser history or other apps' SQLite stores the process can open, tamper with other gadgets' or Torchsnap's databases, and plant files at chosen paths. A planted file is a SQLite page image, so strict config parsers reject it, but it undermines the "areas the gadget may write" assumption of the command confinement work.

## Suggested fix

- Install an authorizer on every gadget-facing connection (rusqlite `Connection::authorizer`, `hooks` feature) that denies `SQLITE_ATTACH` and `SQLITE_DETACH`. Check whether `VACUUM INTO` reaches the authorizer as an attach and deny it explicitly if not.
- Additionally set `SQLITE_LIMIT_ATTACHED` to 0 on those connections as a second layer.
- Check host-internal connections that rely on `ATTACH` (migrations, the frecency store) before applying it to every `SqlStorage`; restrict it to gadget connections if any do.
- Tests: `ATTACH`, `DETACH` and `VACUUM INTO` from gadget SQL fail and create no file; the gadget's own statements, migrations and transactions keep working.

## Relations

- Relates to: [Neutralize config loading for config-reading binaries in command grants](01M4HK9E95CG5W914KKQT65ESS-neutralize-config-loading-for-config-reading-binaries-in-command-grants.md), whose structural rules assume the gadget cannot write outside its own areas
